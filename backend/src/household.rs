//! Household-level media (#75): the family portrait shown at the top of Home.
//!
//! One portrait per household, so it needs no table: its metadata is a single
//! JSON value in the existing `app_settings` key/value store, and the bytes live
//! on the `/data` volume under `household/`, written by the same helpers as pet
//! portraits (#21). A portrait is either `uploaded` (any photo the owner picks)
//! or `generated` (a collage the browser composed from the ACTIVE pets' profile
//! photos). A generated one remembers which pets it was made from, so the API can
//! say when the active roster has moved on and it is worth regenerating.

use axum::extract::State;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::handlers::{decode_image, serve_image, write_image};
use crate::settings::{del_kv, set_kv};
use crate::AppState;

/// Closed set of portrait sources, mirrored as `PortraitSource` in `types.ts`.
pub const PORTRAIT_SOURCES: &[&str] = &["uploaded", "generated"];

const K_PORTRAIT: &str = "household.portrait";

/// Where the portrait lives under the attachments volume. Record attachment
/// directories are numeric ids and pet portraits sit under `photos/`, so this
/// name can't collide with either.
const HOUSEHOLD_SUBDIR: &str = "household";

/// Serialises portrait writes. Each one is read-old, write-new, record, unlink
/// -old; two of them interleaved would orphan a file, or let a DELETE that read
/// the old value erase a PUT that landed in between. One household, so a single
/// process-wide lock is the whole answer.
static PORTRAIT_LOCK: Mutex<()> = Mutex::const_new(());

/// The JSON stored under [`K_PORTRAIT`]. `path` is volume-relative and never
/// leaves the server.
#[derive(Debug, Serialize, Deserialize)]
struct StoredPortrait {
    path: String,
    source: String,
    #[serde(default)]
    pet_ids: Vec<String>,
}

impl StoredPortrait {
    /// Only what [`set_portrait`] writes is trusted: `household/<uuid>.<ext>`
    /// and a known source. The value is read back from the database, so a
    /// hand-edited or corrupted row must not be able to point GET or DELETE at
    /// another file on the volume (`../furfiles.db`, the secret key).
    fn is_valid(&self) -> bool {
        let Some(name) = self.path.strip_prefix(&format!("{HOUSEHOLD_SUBDIR}/")) else {
            return false;
        };
        let Some((stem, ext)) = name.rsplit_once('.') else {
            return false;
        };
        !stem.is_empty()
            && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
            && matches!(ext, "jpg" | "png" | "webp" | "gif")
            && PORTRAIT_SOURCES.contains(&self.source.as_str())
    }
}

/// The portrait as the API reports it.
#[derive(Debug, Serialize)]
pub struct HouseholdPortrait {
    /// `uploaded` | `generated`.
    pub source: String,
    /// For a generated portrait, the sorted ids of the pets it was composed
    /// from; empty for an uploaded one.
    pub pet_ids: Vec<String>,
    /// Changes on every PUT, so clients can cache-bust the stable image URL.
    pub version: String,
    pub updated_at: String,
    /// True when a GENERATED portrait no longer matches the active roster (a pet
    /// was added, archived, rehomed, brought back or deleted). Always false for
    /// an uploaded photo, which isn't tied to the roster.
    pub roster_changed: bool,
}

#[derive(Debug, Serialize)]
pub struct Household {
    pub portrait: Option<HouseholdPortrait>,
}

/// PUT body: the image (same shape as a pet photo) plus where it came from.
#[derive(Debug, Deserialize)]
pub struct NewHouseholdPortrait {
    pub media_type: String,
    pub data_base64: String,
    /// Defaults to `uploaded`.
    #[serde(default)]
    pub source: Option<String>,
    /// Required (non-empty, all active) for `generated`; must be empty for
    /// `uploaded`.
    #[serde(default)]
    pub pet_ids: Vec<String>,
}

/// The stored portrait and its `updated_at`, or None when there is none or the
/// stored value is unreadable/untrusted. An unreadable value reads as "no
/// portrait" rather than a 500 on every Home load; the next PUT or a DELETE
/// replaces it.
async fn load_stored(pool: &SqlitePool) -> AppResult<Option<(StoredPortrait, String)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT value, updated_at FROM app_settings WHERE key = ?")
            .bind(K_PORTRAIT)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(v, at)| {
        serde_json::from_str::<StoredPortrait>(&v)
            .ok()
            .filter(StoredPortrait::is_valid)
            .map(|p| (p, at))
    }))
}

async fn active_pet_ids(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM pets WHERE archived_at IS NULL ORDER BY id")
            .fetch_all(pool)
            .await?;
    Ok(ids)
}

async fn household(st: &AppState) -> AppResult<Household> {
    let Some((stored, updated_at)) = load_stored(&st.pool).await? else {
        return Ok(Household { portrait: None });
    };
    // Metadata for bytes that are gone from the volume would leave Home showing
    // a broken image instead of the invitation; report no portrait instead.
    if !st.attachments_dir.join(&stored.path).is_file() {
        return Ok(Household { portrait: None });
    }
    let roster_changed =
        stored.source == "generated" && stored.pet_ids != active_pet_ids(&st.pool).await?;
    let version = std::path::Path::new(&stored.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(Household {
        portrait: Some(HouseholdPortrait {
            source: stored.source,
            pet_ids: stored.pet_ids,
            version,
            updated_at,
            roster_changed,
        }),
    })
}

/// Validate a PUT's source + pet ids; returns the source and the sorted,
/// de-duplicated ids to store.
async fn check_source(
    pool: &SqlitePool,
    np: &NewHouseholdPortrait,
) -> AppResult<(String, Vec<String>)> {
    let source = np.source.as_deref().unwrap_or("uploaded").to_string();
    if !PORTRAIT_SOURCES.contains(&source.as_str()) {
        return Err(AppError::BadRequest(format!(
            "unknown portrait source '{source}' (expected one of: {})",
            PORTRAIT_SOURCES.join(", ")
        )));
    }
    let mut ids = np.pet_ids.clone();
    ids.sort();
    ids.dedup();
    if source == "uploaded" {
        if !ids.is_empty() {
            return Err(AppError::BadRequest(
                "pet_ids only describe a generated portrait".into(),
            ));
        }
        return Ok((source, ids));
    }
    if ids.is_empty() {
        return Err(AppError::BadRequest(
            "a generated portrait needs the pet_ids it was made from".into(),
        ));
    }
    let active = active_pet_ids(pool).await?;
    if let Some(bad) = ids.iter().find(|id| !active.contains(id)) {
        return Err(AppError::BadRequest(format!("no active pet '{bad}'")));
    }
    Ok((source, ids))
}

/// `GET /household`: the portrait's metadata (null when none is set).
pub async fn get_household(State(st): State<AppState>) -> AppResult<Json<Household>> {
    Ok(Json(household(&st).await?))
}

/// `PUT /household/portrait`: set or replace the portrait. Writes the new file,
/// records it, and only then unlinks the previous one.
pub async fn set_portrait(
    State(st): State<AppState>,
    Json(np): Json<NewHouseholdPortrait>,
) -> AppResult<Json<Household>> {
    let (ext, bytes) = decode_image(&np.media_type, &np.data_base64)?;
    let (source, pet_ids) = check_source(&st.pool, &np).await?;

    let _guard = PORTRAIT_LOCK.lock().await;
    let rel = format!("{HOUSEHOLD_SUBDIR}/{}.{ext}", uuid::Uuid::new_v4());
    write_image(&st, &rel, &bytes)?;
    let old = load_stored(&st.pool).await?.map(|(p, _)| p.path);
    let stored = StoredPortrait {
        path: rel.clone(),
        source,
        pet_ids,
    };
    let value = serde_json::to_string(&stored)
        .map_err(|e| AppError::Internal(format!("could not record portrait: {e}")))?;
    let mut conn = st.pool.acquire().await?;
    if let Err(e) = set_kv(&mut conn, K_PORTRAIT, &value).await {
        // Nothing points at the new file yet; don't leave it orphaned.
        let _ = std::fs::remove_file(st.attachments_dir.join(&rel));
        return Err(e);
    }
    drop(conn);
    if let Some(old) = old.filter(|o| *o != rel) {
        let _ = std::fs::remove_file(st.attachments_dir.join(old));
    }
    Ok(Json(household(&st).await?))
}

/// `GET /household/portrait`: the image bytes; 404 when none is set.
pub async fn get_portrait(State(st): State<AppState>) -> AppResult<Response> {
    let (stored, _) = load_stored(&st.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("no family portrait yet".into()))?;
    serve_image(&st, &stored.path)
}

/// `DELETE /household/portrait`: forget the portrait and unlink its file.
/// Idempotent, and always clears the stored value, so a corrupted one can be
/// reset through the API too.
pub async fn delete_portrait(State(st): State<AppState>) -> AppResult<Json<Household>> {
    let _guard = PORTRAIT_LOCK.lock().await;
    let stored = load_stored(&st.pool).await?;
    let mut conn = st.pool.acquire().await?;
    del_kv(&mut conn, K_PORTRAIT).await?;
    drop(conn);
    if let Some((stored, _)) = stored {
        let _ = std::fs::remove_file(st.attachments_dir.join(stored.path));
    }
    Ok(Json(household(&st).await?))
}
