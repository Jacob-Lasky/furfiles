use std::path::PathBuf;
use std::str::FromStr;

use axum::extract::{DefaultBodyLimit, FromRef, Request};
use axum::http::header::CACHE_CONTROL;
use axum::http::HeaderValue;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub mod crypto;
pub mod diag;
pub mod error;
pub mod handlers;
pub mod household;
pub mod import;
pub mod ingestion;
pub mod insurance_value;
pub mod mcp;
pub mod models;
pub mod seed;
pub mod settings;
pub mod sex;
pub mod web;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Max request body. Attachments and extraction documents arrive base64-encoded
/// (~1.34x their raw size); this comfortably clears a 20 MB document. Axum's
/// default is only 2 MB, so it must be raised explicitly.
const MAX_BODY_BYTES: usize = 40 * 1024 * 1024;

/// Max body for `POST /settings/ingestion/models`: a JSON object holding one
/// API key (~110 chars today), with generous room for longer key formats.
const MODELS_BODY_BYTES: usize = 4 * 1024;

/// Shared application state. Handlers that only need the database keep extracting
/// `State<SqlitePool>` (via [`FromRef`]); the ingestion/attachment handlers
/// extract the whole `AppState` for the encryption key, HTTP client, provider
/// base URL, and attachments directory.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// 32-byte at-rest encryption key for the stored API key.
    pub key: [u8; 32],
    pub http: reqwest::Client,
    /// LLM provider API origin (overridable for tests).
    pub anthropic_base: String,
    /// Short-lived cache of the provider's model list, per key fingerprint (#67).
    pub models_cache: ingestion::ModelCache,
    /// Directory the attachment bytes are written under.
    pub attachments_dir: PathBuf,
    /// EPHEMERAL diagnostics store (#41): client error reports + the files that
    /// triggered them. A tmpfs in production, wiped on every container restart;
    /// never the `/data` volume.
    pub diag_dir: PathBuf,
    /// The MCP endpoint's bearer token (#22). `None` = `/mcp` is disabled.
    pub mcp_token: Option<mcp::McpToken>,
}

impl AppState {
    pub fn new(pool: SqlitePool, key: [u8; 32], attachments_dir: PathBuf) -> Self {
        Self {
            pool,
            key,
            http: reqwest::Client::new(),
            anthropic_base: "https://api.anthropic.com".to_string(),
            models_cache: ingestion::ModelCache::default(),
            attachments_dir,
            // Off-volume default; `run()` points this at the tmpfs mount.
            diag_dir: std::env::temp_dir().join("furfiles-diag"),
            mcp_token: None,
        }
    }

    /// Test state: a fixed key, a temp attachments dir, and a placeholder
    /// provider base (extraction wire calls are covered by live tests, not unit
    /// tests). Each call gets a unique attachments dir so parallel tests don't
    /// collide.
    #[doc(hidden)]
    pub fn for_test(pool: SqlitePool) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let uniq = format!(
            "{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(format!("furfiles-test-{uniq}"));
        Self {
            pool,
            key: [7u8; 32],
            http: reqwest::Client::new(),
            anthropic_base: "http://127.0.0.1:9".to_string(),
            models_cache: ingestion::ModelCache::default(),
            attachments_dir: dir.join("attachments"),
            diag_dir: dir.join("diag"),
            mcp_token: None,
        }
    }
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> SqlitePool {
        state.pool.clone()
    }
}

/// Connect to SQLite (creating the file if needed), enable foreign keys, and run
/// migrations. `max` bounds the pool: use 1 for an in-memory test database so all
/// queries share the same connection (and therefore the same memory).
pub async fn setup_pool(url: &str, max: u32) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(max)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// A single-connection in-memory pool for tests.
pub async fn memory_pool() -> SqlitePool {
    setup_pool("sqlite::memory:", 1)
        .await
        .expect("memory pool should initialize")
}

/// The `/api/...` sub-router, already carrying its state.
pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/users", get(handlers::list_users))
        .route("/pets", get(handlers::list_pets).post(handlers::create_pet))
        .route(
            "/pets/:id",
            get(handlers::get_pet)
                .patch(handlers::update_pet)
                .delete(handlers::delete_pet),
        )
        // Archive (#3): `passed` or `rehomed`; re-post to edit, unarchive to undo.
        .route("/pets/:id/archive", post(handlers::archive_pet))
        .route("/pets/:id/unarchive", post(handlers::unarchive_pet))
        // Portrait photo (#21): one per pet, backed by the reserved photo_path.
        .route(
            "/pets/:id/photo",
            put(handlers::set_pet_photo)
                .get(handlers::get_pet_photo)
                .delete(handlers::delete_pet_photo),
        )
        // Family portrait (#75): one per household, uploaded or generated.
        .route("/household", get(household::get_household))
        .route(
            "/household/portrait",
            put(household::set_portrait)
                .get(household::get_portrait)
                .delete(household::delete_portrait),
        )
        .route(
            "/pets/:id/records",
            get(handlers::list_records).post(handlers::create_record),
        )
        .route(
            "/pets/:id/records/:rid",
            axum::routing::patch(handlers::update_record).delete(handlers::delete_record),
        )
        .route(
            "/pets/:id/weights",
            get(handlers::list_weights).post(handlers::create_weight),
        )
        .route(
            "/pets/:id/weights/:wid",
            axum::routing::patch(handlers::update_weight).delete(handlers::delete_weight),
        )
        // Shared contacts (#20): the care-team entity + per-pet links.
        .route(
            "/contacts",
            get(handlers::list_contacts).post(handlers::create_contact),
        )
        // Facility web parser (#42): fetch a URL → propose contact + requirements.
        // Registered before `/contacts/:cid` so the static segment wins the match.
        .route("/contacts/parse-url", post(handlers::parse_contact_url))
        .route(
            "/contacts/:cid",
            get(handlers::get_contact)
                .patch(handlers::update_contact)
                .delete(handlers::delete_contact),
        )
        .route(
            "/pets/:id/contacts",
            get(handlers::list_pet_contacts).post(handlers::link_pet_contact),
        )
        .route(
            "/pets/:id/contacts/:cid",
            axum::routing::delete(handlers::unlink_pet_contact),
        )
        // Insurance & wellness plans (#8/#29): pet-level policies + embedded doc.
        // `/insurance/extract` and `/insurance/value` are static, registered before
        // `/insurance/:pid`.
        .route(
            "/pets/:id/insurance",
            get(handlers::list_pet_insurance).post(handlers::create_insurance),
        )
        .route("/insurance/extract", post(handlers::extract_insurance_doc))
        // Insurance value (#74): paid in vs paid back per term, break-even.
        .route("/insurance/value", get(insurance_value::household_value))
        .route(
            "/pets/:id/insurance/value",
            get(insurance_value::pet_insurance_value),
        )
        .route(
            "/insurance/:pid/value",
            get(insurance_value::policy_insurance_value),
        )
        .route(
            "/insurance/:pid",
            get(handlers::get_insurance)
                .patch(handlers::update_insurance)
                .delete(handlers::delete_insurance),
        )
        .route(
            "/insurance/:pid/document",
            put(handlers::set_insurance_document)
                .get(handlers::get_insurance_document)
                .delete(handlers::delete_insurance_document),
        )
        // Ongoing care (#63): food, preventatives, medications, supplements.
        .route(
            "/pets/:id/regimens",
            get(handlers::list_pet_regimens).post(handlers::create_regimen),
        )
        .route(
            "/regimens/:rid",
            get(handlers::get_regimen)
                .patch(handlers::update_regimen)
                .delete(handlers::delete_regimen),
        )
        // Reminders / calendar events (#26)
        .route(
            "/pets/:id/reminders",
            get(handlers::list_reminders).post(handlers::create_reminder),
        )
        .route("/reminders", get(handlers::list_all_reminders))
        .route(
            "/reminders/:rid",
            axum::routing::patch(handlers::update_reminder).delete(handlers::delete_reminder),
        )
        .route("/costs/summary", get(handlers::costs_summary))
        // LLM document ingestion (#4)
        .route(
            "/settings/ingestion",
            get(handlers::get_ingestion)
                .put(handlers::put_ingestion)
                .delete(handlers::delete_ingestion),
        )
        // The provider's model list for the Settings picker (#67): GET uses the
        // stored key, POST a key the person has pasted but not saved yet. Its
        // body is one key, so it gets a small limit instead of the 40 MB
        // document allowance.
        .route(
            "/settings/ingestion/models",
            get(handlers::list_ingestion_models)
                .post(handlers::list_ingestion_models_for_key)
                .layer(DefaultBodyLimit::max(MODELS_BODY_BYTES)),
        )
        // Display preferences (#72): breeder terms for intact dogs and cats.
        .route(
            "/settings/display",
            get(handlers::get_display).put(handlers::put_display),
        )
        .route("/extract", post(handlers::extract))
        .route("/extract/preview", post(handlers::extract_preview))
        // Bulk import (#23): one document → a reviewed plan of records + updates
        // across ANY pets, then commit. Analyze never writes.
        .route("/import/analyze", post(import::analyze))
        .route("/import/commit", post(import::commit))
        // Attachments (#4)
        .route(
            "/records/:rid/attachments",
            get(handlers::list_attachments).post(handlers::create_attachment),
        )
        .route(
            "/attachments/:aid",
            get(handlers::get_attachment).delete(handlers::delete_attachment),
        )
        // Diagnostics capture (#41): client error reports + the triggering file,
        // in an ephemeral store, so a phone-only failure can be pulled + replayed.
        .route(
            "/diag",
            get(handlers::list_diag)
                .post(handlers::create_diag)
                .delete(handlers::clear_diag),
        )
        .route("/diag/:id", get(handlers::get_diag))
        .route("/diag/:id/file", get(handlers::get_diag_file))
        // Unmatched /api/* → JSON 404, not the SPA shell (which the outer fallback
        // would otherwise serve with a 200).
        .fallback(handlers::api_not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Cache policy for served responses. Vite emits content-hashed files under
/// `/assets/` (their bytes never change for a given name), so those are cached
/// hard and immutable. EVERYTHING ELSE — notably `index.html`, the SPA shell —
/// is `no-cache` so the browser always revalidates and picks up a new deploy's
/// bundle. Without this, browsers HEURISTICALLY cache `index.html` (it ships only
/// a `last-modified`, no explicit policy), keep serving a stale shell that points
/// at hashed chunks the new build deleted, and users run old code after a
/// redeploy. That is exactly what made a deployed fix look like it never shipped.
async fn cache_control(req: Request, next: Next) -> Response {
    let immutable = req.uri().path().starts_with("/assets/");
    let mut res = next.run(req).await;
    let policy = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(policy));
    res
}

/// Log any 4xx/5xx response with the request's method, path, status, and
/// user-agent (#41). `AppError::IntoResponse` has no request context, so this
/// middleware is where a failed request becomes visible in `docker logs` — 5xx
/// at warn, client 4xx at info. Never logs bodies or headers beyond the UA, so
/// no secret (e.g. the ingestion key) can leak.
async fn log_errors(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let ua = req
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let res = next.run(req).await;
    let status = res.status();
    if status.is_server_error() {
        tracing::warn!(%method, path, status = status.as_u16(), ua, "request failed");
    } else if status.is_client_error() {
        tracing::info!(%method, path, status = status.as_u16(), ua, "request rejected");
    }
    res
}

/// Full application: API under `/api`, the MCP endpoint at `/mcp` (#22, a JSON
/// 404 unless a token is configured), everything else served from the built
/// frontend with an SPA fallback so deep links (`/pets/biscuit`, `/costs`) work
/// on a hard refresh (#11).
pub fn build_router(state: AppState, static_dir: PathBuf) -> Router {
    let index = static_dir.join("index.html");
    let spa = ServeDir::new(&static_dir).fallback(ServeFile::new(index));
    let mcp = mcp::router(state.clone(), state.mcp_token.clone());
    Router::new()
        .nest("/api", api_router(state))
        .merge(mcp)
        .fallback_service(spa)
        .layer(middleware::from_fn(cache_control))
        .layer(middleware::from_fn(log_errors))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

/// Parse a boolean flag from an env var or query parameter: `1` / `true` / `yes`
/// (any case, surrounding whitespace ignored) is on; anything else, including
/// absence and `0` / `false`, is off. One parser so `?include_archived=0` can't
/// mean "on" just because the key is present.
pub fn truthy(val: Option<&str>) -> bool {
    matches!(
        val.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Turn a pet name into a URL slug: lowercase, alnum runs joined by hyphens.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("pet");
    }
    out
}

/// Default portrait gradient per species (design.md palette) when the caller
/// doesn't supply one.
pub fn default_gradient(species: &str) -> (&'static str, &'static str) {
    match species.to_lowercase().as_str() {
        "dog" => ("#F3B44C", "#E0762F"),
        "cat" => ("#DE93AE", "#B25E85"),
        "horse" => ("#D28F5F", "#A85C39"),
        "bird" => ("#A493E6", "#6E5CC8"),
        "fish" => ("#57C4CC", "#2E8FA8"),
        _ => ("#F2624C", "#DB4E38"),
    }
}

/// Entry point used by the binary.
pub async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "furfiles=info,tower_http=warn".into()),
        )
        .init();

    let db_path = std::env::var("FURFILES_DB").unwrap_or_else(|_| "data/furfiles.db".into());
    let db_dir = std::path::Path::new(&db_path)
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&db_dir).ok();
    let pool = setup_pool(&format!("sqlite://{db_path}"), 5).await?;
    seed::ensure_owner(&pool).await?;
    if seed::seed_enabled(std::env::var("FURFILES_SEED_DEMO").ok().as_deref()) {
        seed::seed_if_empty(&pool).await?;
    }

    // Attachments live on the data volume next to the DB by default (#4).
    let attachments_dir = std::env::var("FURFILES_ATTACHMENTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| db_dir.join("attachments"));
    std::fs::create_dir_all(&attachments_dir).ok();
    // At-rest encryption key for the stored LLM API key.
    let key = crypto::resolve_key(&db_dir);
    let mut state = AppState::new(pool, key, attachments_dir);
    // Ephemeral diagnostics store (#41): a tmpfs mount in production so it is
    // wiped on restart and never lands on the /data volume.
    let diag_dir = std::env::var("FURFILES_DIAG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("furfiles-diag"));
    std::fs::create_dir_all(&diag_dir).ok();
    state.diag_dir = diag_dir;
    state.mcp_token = mcp::McpToken::from_env();
    // Fingerprint attachments stored before migration 0015 (#23), so bulk import
    // recognises documents that were uploaded before it existed.
    match handlers::backfill_attachment_hashes(&state).await {
        Ok(0) => {}
        Ok(n) => tracing::info!("fingerprinted {n} existing attachment(s)"),
        Err(e) => tracing::warn!("attachment fingerprint backfill failed: {e}"),
    }

    let static_dir =
        PathBuf::from(std::env::var("FURFILES_STATIC").unwrap_or_else(|_| "static".into()));
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let app = build_router(state, static_dir);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("FurFiles v{VERSION} listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
