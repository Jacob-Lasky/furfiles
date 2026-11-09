use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::send;

use furfiles::{api_router, build_router, default_gradient, memory_pool, slugify, AppState};

// ---------- pure helpers ----------

#[test]
fn slugify_makes_url_safe_ids() {
    assert_eq!(slugify("Biscuit"), "biscuit");
    assert_eq!(slugify("Mr. Whiskers"), "mr-whiskers");
    assert_eq!(slugify("  Comet  "), "comet");
    assert_eq!(slugify("!!!"), "pet");
}

#[test]
fn default_gradient_covers_species() {
    assert_eq!(default_gradient("Dog").0, "#F3B44C");
    assert_eq!(default_gradient("cat").0, "#DE93AE");
    assert_eq!(default_gradient("Fish").0, "#57C4CC");
    assert_ne!(default_gradient("axolotl").0, ""); // fallback still returns something
}

#[test]
fn truthy_parses_flags_strictly() {
    use furfiles::truthy;
    assert!(truthy(Some("1")) && truthy(Some(" True ")) && truthy(Some("yes")));
    assert!(!truthy(Some("0")) && !truthy(Some("false")) && !truthy(Some("")) && !truthy(None));
}

#[test]
fn seed_enabled_only_for_truthy_flags() {
    use furfiles::seed::seed_enabled;
    assert!(seed_enabled(Some("1")));
    assert!(seed_enabled(Some("true")));
    assert!(seed_enabled(Some(" YES ")));
    assert!(!seed_enabled(Some("0")));
    assert!(!seed_enabled(Some("false")));
    assert!(!seed_enabled(Some("")));
    assert!(!seed_enabled(None)); // default: no demo data
}

// ---------- API against seeded data ----------

#[tokio::test]
async fn ensure_owner_is_idempotent_and_never_seeds_demo() {
    let pool = memory_pool().await;
    furfiles::seed::ensure_owner(&pool).await.unwrap();
    furfiles::seed::ensure_owner(&pool).await.unwrap(); // second call is a no-op
    let app = api_router(AppState::for_test(pool));

    let (_, users) = send(&app, "GET", "/users", None).await;
    let users = users.as_array().unwrap();
    assert_eq!(users.len(), 1, "exactly the one real owner");
    assert_eq!(users[0]["id"], "jake");

    // ensure_owner must not pull in the fictional demo pets
    let (_, pets) = send(&app, "GET", "/pets?include_archived=1", None).await;
    assert!(pets.as_array().unwrap().is_empty(), "no demo pets seeded");
}

#[test]
fn owner_name_defaults_to_a_generic_name() {
    use furfiles::seed::owner_name;
    assert_eq!(owner_name(None), "Owner");
    assert_eq!(owner_name(Some("  ")), "Owner");
    assert_eq!(owner_name(Some(" Sam ")), "Sam");
}

#[test]
fn owner_name_configured_follows_the_same_blank_rule() {
    use furfiles::seed::{owner_name, owner_name_configured};
    assert!(!owner_name_configured(None));
    assert!(!owner_name_configured(Some("")));
    assert!(!owner_name_configured(Some("   ")));
    assert!(owner_name_configured(Some(" Sam ")));
    assert!(owner_name_configured(Some("Owner")), "an explicit 'Owner' counts as chosen");
    assert_eq!(owner_name(Some("   ")), "Owner");
}

#[tokio::test]
async fn demo_household_covers_every_main_screen_and_is_dated_from_today() {
    let pool = memory_pool().await;
    furfiles::seed::ensure_owner(&pool).await.unwrap();
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    furfiles::seed::seed_if_empty(&pool).await.unwrap(); // idempotent once pets exist
    let app = api_router(AppState::for_test(pool));

    let (_, users) = send(&app, "GET", "/users", None).await;
    let owner = users.as_array().unwrap().iter().find(|u| u["id"] == "jake").unwrap();
    assert_eq!(owner["name"], "Alex", "demo owner gets a person's name");

    let (_, contacts) = send(&app, "GET", "/contacts", None).await;
    assert_eq!(contacts.as_array().unwrap().len(), 6);
    assert!(contacts.as_array().unwrap().iter().all(|c| c["phone"].as_str().unwrap().starts_with("555-01")), "fictional numbers only");

    let (_, b) = send(&app, "GET", "/pets/biscuit", None).await;
    assert_eq!(b["contacts"].as_array().unwrap().len(), 3, "care team");
    assert_eq!(b["allergies"][0]["label"], "Chicken");
    assert!(b["regimens"].as_array().unwrap().len() >= 3, "ongoing care");
    assert_eq!(b["insurance_policies"].as_array().unwrap().len(), 1);
    assert!(b["records"].as_array().unwrap().iter().any(|r| r["contact"]["name"] == "Cedar Creek Veterinary Hospital"));
    // Relative dates: on the day it is seeded, the next reminder is ahead.
    let today = utc_today();
    assert!(b["next_reminder"]["due_on"].as_str().unwrap() > today.as_str());
    let (_, reminders) = send(&app, "GET", "/reminders", None).await;
    assert!(reminders.as_array().unwrap().len() >= 8);
}

#[tokio::test]
async fn demo_seed_keeps_an_owner_name_chosen_by_the_household() {
    let pool = memory_pool().await;
    sqlx::query("INSERT INTO users (id, name, color) VALUES ('jake', 'Sam', '#000000')")
        .execute(&pool).await.unwrap();
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));
    let (_, users) = send(&app, "GET", "/users", None).await;
    let owner = users.as_array().unwrap().iter().find(|u| u["id"] == "jake").unwrap();
    assert_eq!(owner["name"], "Sam", "FURFILES_OWNER_NAME is not overwritten by the demo");
}

#[tokio::test]
async fn demo_seed_keeps_a_configured_owner_name_even_when_it_is_the_default() {
    let pool = memory_pool().await;
    sqlx::query("INSERT INTO users (id, name, color) VALUES ('jake', 'Owner', '#000000')")
        .execute(&pool).await.unwrap();
    furfiles::seed::seed_demo(&pool, true).await.unwrap(); // FURFILES_OWNER_NAME=Owner
    let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = 'jake'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(name, "Owner");
}

#[tokio::test]
async fn demo_weights_use_units_the_weight_api_accepts() {
    let pool = memory_pool().await;
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let units: Vec<String> = sqlx::query_scalar("SELECT DISTINCT unit FROM weight_entries")
        .fetch_all(&pool).await.unwrap();
    assert!(units.iter().all(|u| u == "kg" || u == "lb"), "{units:?}");
}

fn utc_today() -> String {
    // SQLite's date('now') is UTC, so compare against UTC today.
    chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

#[tokio::test]
async fn health_reports_ok() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let (status, body) = send(&app, "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn seeded_household_lists_active_pets() {
    let pool = memory_pool().await;
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));

    let (status, body) = send(&app, "GET", "/pets", None).await;
    assert_eq!(status, StatusCode::OK);
    let pets = body.as_array().unwrap();
    assert_eq!(pets.len(), 5, "5 active pets, Rex is archived");
    // The care score is gone (#35); each pet carries a `next_reminder` key
    // ("what's next"), null when caught up. The seed gives most pets one.
    assert!(pets.iter().all(|p| p.get("score").is_none()));
    assert!(pets.iter().all(|p| p.as_object().unwrap().contains_key("next_reminder")));
    let biscuit = pets.iter().find(|p| p["id"] == "biscuit").unwrap();
    assert_eq!(biscuit["next_reminder"]["title"], "Heartworm preventative");

    let (_, all) = send(&app, "GET", "/pets?include_archived=1", None).await;
    assert_eq!(all.as_array().unwrap().len(), 6, "archived pet included");
}

#[tokio::test]
async fn users_are_seeded() {
    let pool = memory_pool().await;
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));
    let (status, body) = send(&app, "GET", "/users", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn pet_detail_includes_records_and_weights() {
    let pool = memory_pool().await;
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));

    let (status, body) = send(&app, "GET", "/pets/biscuit", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Biscuit");
    assert!(body["records"].as_array().unwrap().len() >= 3);
    assert_eq!(body["weights"].as_array().unwrap().len(), 6);
    // "What's next" replaces the care score (#35): the soonest not-done reminder.
    assert!(body.get("score").is_none());
    assert_eq!(body["next_reminder"]["title"], "Heartworm preventative");

    let (missing, _) = send(&app, "GET", "/pets/nope", None).await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn costs_summary_aggregates() {
    let pool = memory_pool().await;
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));
    let (status, body) = send(&app, "GET", "/costs/summary", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["total_cents"].as_i64().unwrap() > 0);
    assert!(body["covered_cents"].as_i64().unwrap() > 0);
    assert!(!body["by_category"].as_array().unwrap().is_empty());
    assert!(!body["by_pet"].as_array().unwrap().is_empty());
}

// ---------- full CRUD lifecycle on an empty db ----------

#[tokio::test]
async fn pet_lifecycle_create_update_archive_delete() {
    let app = api_router(AppState::for_test(memory_pool().await));

    // create
    let (status, pet) = send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name": "Nimbus", "species": "Cat", "breed": "Tabby",
                    "birth_date": "2020-01-15", "gotcha_date": "2020-04-01"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(pet["id"], "nimbus");
    assert_eq!(pet["gradient_a"], "#DE93AE"); // species default applied
    assert_eq!(pet["gotcha_date"], "2020-04-01"); // gotcha day round-trips

    // validation
    let (bad, _) = send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name": "", "species": "Cat"})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // duplicate name gets a distinct slug
    let (_, dup) = send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name": "Nimbus", "species": "Cat"})),
    )
    .await;
    assert_eq!(dup["id"], "nimbus-2");

    // update
    let (status, updated) = send(
        &app,
        "PATCH",
        "/pets/nimbus",
        Some(json!({"weight": 4.2, "weight_unit": "kg", "gotcha_date": "2020-05-05"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["weight"], 4.2);
    assert_eq!(updated["breed"], "Tabby"); // untouched field preserved
    assert_eq!(updated["gotcha_date"], "2020-05-05"); // gotcha day updates
    assert_eq!(updated["birth_date"], "2020-01-15"); // and unrelated dates stay put

    // add a record + weight, confirm they surface
    let (rc, rec) = send(
        &app,
        "POST",
        "/pets/nimbus/records",
        Some(json!({"occurred_on":"2026-07-01","title":"First checkup","cost_cents":9500,"category":"Appointments"})),
    )
    .await;
    assert_eq!(rc, StatusCode::CREATED);
    assert_eq!(rec["title"], "First checkup");

    let (wc, _) = send(
        &app,
        "POST",
        "/pets/nimbus/weights",
        Some(json!({"recorded_on":"2026-07-01","value":4.2})),
    )
    .await;
    assert_eq!(wc, StatusCode::CREATED);

    let (_, detail) = send(&app, "GET", "/pets/nimbus", None).await;
    assert_eq!(detail["records"].as_array().unwrap().len(), 1);
    assert_eq!(detail["weights"].as_array().unwrap().len(), 1);

    // record on a missing pet 404s
    let (miss, _) = send(
        &app,
        "POST",
        "/pets/ghost/records",
        Some(json!({"occurred_on":"2026-07-01","title":"x"})),
    )
    .await;
    assert_eq!(miss, StatusCode::NOT_FOUND);

    // archive (memorialize) with the real date it happened
    let (status, arch) = send(
        &app,
        "POST",
        "/pets/nimbus/archive",
        Some(json!({"memorial_note":"A good cat.","archived_at":"2026-10-20"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(arch["archived_at"], "2026-10-20"); // honest date, not "now"
    assert_eq!(arch["memorial_note"], "A good cat.");
    assert_eq!(arch["archive_kind"], "passed"); // the default kind (#3)

    // archived pet drops off the default list
    let (_, list) = send(&app, "GET", "/pets", None).await;
    assert!(list.as_array().unwrap().iter().all(|p| p["id"] != "nimbus"));

    // delete
    let (del, _) = send(&app, "DELETE", "/pets/nimbus", None).await;
    assert_eq!(del, StatusCode::NO_CONTENT);
    let (gone, _) = send(&app, "GET", "/pets/nimbus", None).await;
    assert_eq!(gone, StatusCode::NOT_FOUND);
    let (del2, _) = send(&app, "DELETE", "/pets/nimbus", None).await;
    assert_eq!(del2, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn missing_pet_paths_all_404() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let cases = [
        ("GET", "/pets/ghost", None),
        ("GET", "/pets/ghost/records", None),
        ("GET", "/pets/ghost/weights", None),
        ("PATCH", "/pets/ghost", Some(json!({"weight": 5.0}))),
        (
            "POST",
            "/pets/ghost/archive",
            Some(json!({"memorial_note": "x"})),
        ),
        (
            "POST",
            "/pets/ghost/weights",
            Some(json!({"recorded_on": "2026-01-01", "value": 5.0})),
        ),
    ];
    for (method, uri, body) in cases {
        let (status, _) = send(&app, method, uri, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} should 404");
    }
}

#[tokio::test]
async fn archive_without_a_date_falls_back_to_today() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name": "Sunny", "species": "Dog"})),
    )
    .await;
    let (status, arch) = send(
        &app,
        "POST",
        "/pets/sunny/archive",
        Some(json!({"memorial_note": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let date = arch["archived_at"].as_str().expect("defaults to today when no date given");
    assert_eq!(date.len(), 10, "a date, not a timestamp: {date}");
}

#[tokio::test]
async fn archive_kinds_edit_and_unarchive() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(&app, "POST", "/pets", Some(json!({"name": "Koa", "species": "Horse"}))).await;
    send(
        &app,
        "POST",
        "/pets/koa/reminders",
        Some(json!({"due_on": "2027-01-01", "title": "Coggins"})),
    )
    .await;
    let (_, before) = send(&app, "GET", "/pets/koa", None).await;
    assert_eq!(before["next_reminder"]["title"], "Coggins");

    // validation: closed kind set, real dates only
    let (bad_kind, err) = send(&app, "POST", "/pets/koa/archive", Some(json!({"kind": "lost"}))).await;
    assert_eq!(bad_kind, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("rehomed"));
    let (bad_date, _) = send(
        &app,
        "POST",
        "/pets/koa/archive",
        Some(json!({"kind": "rehomed", "archived_at": "2026-02-31"})),
    )
    .await;
    assert_eq!(bad_date, StatusCode::BAD_REQUEST);
    let (_, still) = send(&app, "GET", "/pets/koa", None).await;
    assert!(still["archived_at"].is_null(), "a rejected archive writes nothing");

    // rehome: alive, moved on
    let (status, arch) = send(
        &app,
        "POST",
        "/pets/koa/archive",
        Some(json!({"kind": "rehomed", "archived_at": "2026-08-01", "memorial_note": "Retired to a trainer"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(arch["archive_kind"], "rehomed");
    assert_eq!(arch["archived_at"], "2026-08-01");

    // an archived pet stops prompting: no next_reminder, off the calendar feed
    let (_, detail) = send(&app, "GET", "/pets/koa", None).await;
    assert!(detail["next_reminder"].is_null());
    assert_eq!(detail["reminders"].as_array().unwrap().len(), 1, "history kept");
    let (_, list) = send(&app, "GET", "/pets?include_archived=1", None).await;
    assert!(list[0]["next_reminder"].is_null());
    let (_, cal) = send(&app, "GET", "/reminders", None).await;
    assert!(cal.as_array().unwrap().is_empty());
    let (_, cal_all) = send(&app, "GET", "/reminders?include_archived=1", None).await;
    assert_eq!(cal_all.as_array().unwrap().len(), 1);
    // a present-but-false flag is OFF, not "the key exists so on"
    let (_, cal_off) = send(&app, "GET", "/reminders?include_archived=0", None).await;
    assert!(cal_off.as_array().unwrap().is_empty());
    let (_, pets_off) = send(&app, "GET", "/pets?include_archived=false", None).await;
    assert!(pets_off.as_array().unwrap().is_empty());

    // a partial re-post keeps what it omits: a note-only fix must NOT flip a
    // living (rehomed) pet into a memorial or move its date to today
    let (_, note_only) = send(&app, "POST", "/pets/koa/archive", Some(json!({"memorial_note": "With Sam in Riverbend"}))).await;
    assert_eq!(note_only["archive_kind"], "rehomed");
    assert_eq!(note_only["archived_at"], "2026-08-01");
    assert_eq!(note_only["memorial_note"], "With Sam in Riverbend");
    let (_, date_only) = send(&app, "POST", "/pets/koa/archive", Some(json!({"archived_at": "2026-08-02"}))).await;
    assert_eq!(date_only["memorial_note"], "With Sam in Riverbend", "omitted note kept");
    assert_eq!(date_only["archived_at"], "2026-08-02");
    let (blank_kind, _) = send(&app, "POST", "/pets/koa/archive", Some(json!({"kind": "  "}))).await;
    assert_eq!(blank_kind, StatusCode::BAD_REQUEST, "blank kind is an error, not a silent 'passed'");

    // still editable while archived (#37)
    let (patched, p) = send(&app, "PATCH", "/pets/koa", Some(json!({"breed": "Dutch Warmblood"}))).await;
    assert_eq!(patched, StatusCode::OK);
    assert_eq!(p["breed"], "Dutch Warmblood");
    assert_eq!(p["archive_kind"], "rehomed", "a profile edit leaves the archive alone");

    // re-posting edits the archive: blank note clears it
    let (_, edited) = send(
        &app,
        "POST",
        "/pets/koa/archive",
        Some(json!({"kind": "passed", "archived_at": "2026-09-01", "memorial_note": "  "})),
    )
    .await;
    assert_eq!(edited["archive_kind"], "passed");
    assert_eq!(edited["archived_at"], "2026-09-01");
    assert!(edited["memorial_note"].is_null());
    // explicit null date → today (not "keep")
    let (_, nulled) = send(&app, "POST", "/pets/koa/archive", Some(json!({"archived_at": null}))).await;
    assert_ne!(nulled["archived_at"], "2026-09-01");
    assert_eq!(nulled["archived_at"].as_str().unwrap().len(), 10);

    // unarchive restores the pet fully
    let (status, back) = send(&app, "POST", "/pets/koa/unarchive", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(back["archived_at"].is_null());
    assert!(back["archive_kind"].is_null());
    let (_, again) = send(&app, "GET", "/pets/koa", None).await;
    assert_eq!(again["next_reminder"]["title"], "Coggins");
    let (missing, _) = send(&app, "POST", "/pets/ghost/unarchive", None).await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_record_requires_a_title() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name": "T", "species": "Dog"})),
    )
    .await;
    let (status, _) = send(
        &app,
        "POST",
        "/pets/t/records",
        Some(json!({"occurred_on": "2026-01-01", "title": "  "})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---------- SPA routing / deep links ----------

#[tokio::test]
async fn spa_fallback_serves_index_for_deep_links() {
    let dir = std::env::temp_dir().join(format!("furfiles-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>FurFiles</title>",
    )
    .unwrap();

    let app = build_router(AppState::for_test(memory_pool().await), dir.clone());

    // API still routes under /api
    let (api, body) = send(&app, "GET", "/api/health", None).await;
    assert_eq!(api, StatusCode::OK);
    assert_eq!(body["status"], "ok");

    // a client route unknown to the server returns index.html (deep link works)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pets/biscuit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html = res.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&html).contains("FurFiles"));

    // an UNKNOWN /api route returns a JSON 404 — NOT the SPA shell. (A 200-serving
    // index.html for /api/* would hand API clients HTML and hide the error.)
    let (s, body) = send(&app, "GET", "/api/nope-not-a-route", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("no such API route"));

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn cache_policy_keeps_the_shell_fresh_and_assets_immutable() {
    // The SPA shell must never be cached (else a redeploy's new bundle is
    // ignored and users run old code); content-hashed assets are immutable.
    let dir = std::env::temp_dir().join(format!("furfiles-cache-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>FurFiles</title>",
    )
    .unwrap();
    std::fs::write(dir.join("assets/index-abc123.js"), "console.log(1)").unwrap();
    let app = build_router(AppState::for_test(memory_pool().await), dir.clone());

    let cache_of = |app: axum::Router, uri: &'static str| async move {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        res.headers()
            .get("cache-control")
            .map(|v| v.to_str().unwrap().to_string())
    };

    // index.html (root) and a deep-link fallback both revalidate
    assert_eq!(
        cache_of(app.clone(), "/").await.as_deref(),
        Some("no-cache")
    );
    assert_eq!(
        cache_of(app.clone(), "/pets/biscuit").await.as_deref(),
        Some("no-cache")
    );
    // hashed asset is cached hard + immutable
    assert_eq!(
        cache_of(app.clone(), "/assets/index-abc123.js")
            .await
            .as_deref(),
        Some("public, max-age=31536000, immutable")
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ======================= #4: LLM ingestion + attachments =======================

// ---------- crypto (at-rest key encryption) ----------

#[test]
fn crypto_roundtrips_and_rejects_wrong_key() {
    let key = [3u8; 32];
    let blob = furfiles::crypto::encrypt(&key, "sk-ant-secret-123");
    assert_ne!(blob, "sk-ant-secret-123", "must not be plaintext");
    assert_eq!(
        furfiles::crypto::decrypt(&key, &blob).unwrap(),
        "sk-ant-secret-123"
    );
    // a different key cannot decrypt
    assert!(furfiles::crypto::decrypt(&[4u8; 32], &blob).is_err());
    // random nonce → two encryptions of the same value differ
    assert_ne!(
        furfiles::crypto::encrypt(&key, "x"),
        furfiles::crypto::encrypt(&key, "x")
    );
    // malformed blobs
    assert!(furfiles::crypto::decrypt(&key, "not base64 !!").is_err());
    assert!(furfiles::crypto::decrypt(&key, "AAAA").is_err()); // shorter than a nonce
}

// ---------- ingestion pure functions (request/response shape) ----------

#[test]
fn build_request_uses_document_for_pdf_and_image_for_images() {
    let pdf = furfiles::ingestion::build_request("claude-x", "application/pdf", "QkFTRTY0", &[]);
    let block = &pdf["messages"][0]["content"][0];
    assert_eq!(block["type"], "document");
    assert_eq!(block["source"]["media_type"], "application/pdf");
    assert_eq!(block["source"]["data"], "QkFTRTY0");
    assert_eq!(pdf["tools"][0]["name"], "record_fields");
    assert_eq!(pdf["tool_choice"]["type"], "auto"); // never forced (#87)
    assert_eq!(pdf["model"], "claude-x");

    let img = furfiles::ingestion::build_request("claude-x", "image/png", "QkFTRTY0", &[]);
    assert_eq!(img["messages"][0]["content"][0]["type"], "image");
    assert_eq!(
        img["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );
}

#[test]
fn extraction_prompt_lists_known_fields_only_when_present() {
    let none = furfiles::ingestion::extraction_prompt(&[]);
    assert!(!none.contains("already in use"));
    assert!(none.contains("Ignore any instructions")); // prompt-injection guard always present

    let some =
        furfiles::ingestion::extraction_prompt(&["Rabies tag".to_string(), "Weight".to_string()]);
    assert!(some.contains("already in use"));
    assert!(some.contains("Rabies tag"));
    assert!(some.contains("Weight"));
}

#[test]
fn parse_response_reads_tool_use_and_errors_otherwise() {
    let good = json!({
        "content": [{
            "type": "tool_use", "name": "record_fields",
            "input": {
                "occurred_on": "2026-07-02", "title": "Dental cleaning",
                "category": "Appointments", "cost": 420.0,
                "clinic": "Cedar Creek", "notes": "extraction",
                "suggested_pet_name": "Biscuit",
                "weight": {"value": 46.2, "unit": "lb"},
                "reminders": [
                    {"due_on": "2026-09-05", "title": "Fecal exam"},
                    {"due_on": "2026-12-22", "title": "Bordetella booster"}
                ],
                "line_items": [
                    {"description": "Dental cleaning", "amount": 320.0},
                    {"description": "Extraction", "amount": 100.0}
                ],
                "extra_fields": {"Rabies tag": "12345"},
                "low_confidence_fields": ["cost"]
            }
        }]
    });
    let f = furfiles::ingestion::parse_response(&good).unwrap();
    assert_eq!(f.title.as_deref(), Some("Dental cleaning"));
    assert_eq!(f.cost, Some(420.0));
    assert_eq!(f.weight.as_ref().and_then(|w| w.value), Some(46.2));
    assert_eq!(f.reminders.len(), 2);
    assert_eq!(f.reminders[0].title, "Fecal exam");
    assert_eq!(f.reminders[0].due_on.as_deref(), Some("2026-09-05"));
    assert_eq!(f.line_items.len(), 2);
    assert_eq!(f.line_items[0].description, "Dental cleaning");
    assert_eq!(f.line_items[0].amount, Some(320.0));
    assert_eq!(
        f.extra_fields.get("Rabies tag").map(String::as_str),
        Some("12345")
    );
    assert_eq!(f.low_confidence_fields, vec!["cost".to_string()]);

    // no tool_use block → error
    assert!(
        furfiles::ingestion::parse_response(&json!({"content":[{"type":"text","text":"hi"}]}))
            .is_err()
    );
    // missing content → error
    assert!(furfiles::ingestion::parse_response(&json!({})).is_err());
}

#[test]
fn parse_response_drops_reminders_the_scan_sheet_could_not_save() {
    let resp = json!({
        "content": [{
            "type": "tool_use", "name": "record_fields",
            "input": {
                "title": "Exam",
                "reminders": [
                    {"due_on": " 2026-09-05 ", "title": " Fecal exam "},
                    {"due_on": "2026-02-30", "title": "Impossible date"},
                    {"due_on": "next spring", "title": "Not a date"},
                    {"title": "No date"},
                    {"due_on": "2026-10-01", "title": "  "}
                ]
            }
        }]
    });
    let f = furfiles::ingestion::parse_response(&resp).unwrap();
    assert_eq!(f.reminders.len(), 1);
    assert_eq!(f.reminders[0].title, "Fecal exam");
    assert_eq!(f.reminders[0].due_on.as_deref(), Some("2026-09-05"));
}

#[test]
fn media_supported_matches_the_allowed_list() {
    assert!(furfiles::ingestion::media_supported("application/pdf"));
    assert!(furfiles::ingestion::media_supported("image/jpeg"));
    assert!(!furfiles::ingestion::media_supported("text/plain"));
    assert!(!furfiles::ingestion::media_supported("application/zip"));
}

// ---------- ingestion settings endpoint (key is write-only) ----------

#[tokio::test]
async fn ingestion_settings_store_key_write_only_and_toggle() {
    let app = api_router(AppState::for_test(memory_pool().await));

    // defaults
    let (s, v) = send(&app, "GET", "/settings/ingestion", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["enabled"], false);
    assert_eq!(v["has_key"], false);
    assert_eq!(v["provider"], "anthropic");
    assert_eq!(v["model"], "claude-sonnet-5");

    // set key + model + enable
    let (s, v) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key":"sk-ant-xyz","model":"claude-haiku-4-5-20251001","enabled":true})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["has_key"], true);
    assert_eq!(v["enabled"], true);
    assert_eq!(v["model"], "claude-haiku-4-5-20251001");
    assert!(v.get("api_key").is_none(), "key must never be echoed");
    assert!(
        !v.to_string().contains("sk-ant"),
        "key must not leak anywhere in the view"
    );

    // PUT without api_key preserves the stored key
    let (_, v) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"model":"claude-sonnet-5"})),
    )
    .await;
    assert_eq!(v["has_key"], true);
    assert_eq!(v["model"], "claude-sonnet-5");

    // an empty api_key string is a no-op (still has a key)
    let (_, v) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key":""})),
    )
    .await;
    assert_eq!(v["has_key"], true);

    // unsupported provider and empty model are rejected
    let (bad, _) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"provider":"openai"})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    let (bad, _) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"model":"  "})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // DELETE clears the key and disables
    let (s, v) = send(&app, "DELETE", "/settings/ingestion", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["has_key"], false);
    assert_eq!(v["enabled"], false);
}

// ---------- extract endpoint validation (no network on these paths) ----------

#[tokio::test]
async fn extract_requires_setup_then_validates_input() {
    let app = api_router(AppState::for_test(memory_pool().await));

    // not set up → 400 before anything else
    let (s, _) = send(
        &app,
        "POST",
        "/extract",
        Some(json!({"media_type":"application/pdf","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // set up (key + enabled)
    send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key":"sk-ant-xyz","enabled":true})),
    )
    .await;

    // unsupported media type → 400
    let (s, _) = send(
        &app,
        "POST",
        "/extract",
        Some(json!({"media_type":"text/plain","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // invalid base64 → 400
    let (s, _) = send(
        &app,
        "POST",
        "/extract",
        Some(json!({"media_type":"application/pdf","data_base64":"not base64 !!"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // empty file → 400
    let (s, _) = send(
        &app,
        "POST",
        "/extract",
        Some(json!({"media_type":"application/pdf","data_base64":""})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // oversized document → 413 ("A"*N is valid base64 that decodes to just over 20 MiB)
    let big = "A".repeat(27_962_028);
    let (s, _) = send(
        &app,
        "POST",
        "/extract",
        Some(json!({"media_type":"application/pdf","data_base64": big})),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

// ---------- attachments ----------

fn count_files(p: &std::path::Path) -> usize {
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                n += count_files(&path)
            } else {
                n += 1
            }
        }
    }
    n
}

#[tokio::test]
async fn attachments_upload_list_download_delete() {
    let dir = std::env::temp_dir().join(format!("furfiles-att-a-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [1u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;
    let (_, rec) = send(
        &app,
        "POST",
        "/pets/pedro/records",
        Some(json!({"occurred_on":"2026-07-01","title":"Visit"})),
    )
    .await;
    let rid = rec["id"].as_i64().unwrap();

    // upload "hello" as a scanned invoice
    let (s, att) = send(&app, "POST", &format!("/records/{rid}/attachments"),
        Some(json!({"filename":"invoice.pdf","media_type":"application/pdf","data_base64":"aGVsbG8=","source":"scan"}))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(att["filename"], "invoice.pdf");
    assert_eq!(att["source"], "scan");
    assert_eq!(att["size_bytes"], 5);
    assert!(
        att.get("stored_path").is_none(),
        "internal path must not leak"
    );
    let aid = att["id"].as_str().unwrap().to_string();
    assert_eq!(count_files(&dir), 1);

    // list
    let (_, list) = send(&app, "GET", &format!("/records/{rid}/attachments"), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // surfaces in the pet detail under its record
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(detail["records"][0]["attachments"][0]["id"], aid);

    // download returns bytes + content type
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/attachments/{aid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/pdf"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello");

    // delete removes the row and the file
    let (d, _) = send(&app, "DELETE", &format!("/attachments/{aid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, list) = send(&app, "GET", &format!("/records/{rid}/attachments"), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
    assert_eq!(count_files(&dir), 0);

    // 404s: attachment on a missing record, and a missing attachment
    let (nf, _) = send(
        &app,
        "POST",
        "/records/999999/attachments",
        Some(json!({"filename":"x","media_type":"image/png","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(&app, "GET", "/attachments/nope", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn attachment_rejects_bad_and_empty_payloads() {
    let dir = std::env::temp_dir().join(format!("furfiles-att-c-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [5u8; 32], dir.clone()));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Kipper","species":"Cat"})),
    )
    .await;
    let (_, rec) = send(
        &app,
        "POST",
        "/pets/kipper/records",
        Some(json!({"occurred_on":"2026-07-01","title":"Visit"})),
    )
    .await;
    let rid = rec["id"].as_i64().unwrap();

    let (bad, _) = send(
        &app,
        "POST",
        &format!("/records/{rid}/attachments"),
        Some(json!({"filename":"x","media_type":"image/png","data_base64":"not base64 !!"})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    let (empty, _) = send(
        &app,
        "POST",
        &format!("/records/{rid}/attachments"),
        Some(json!({"filename":"x","media_type":"image/png","data_base64":""})),
    )
    .await;
    assert_eq!(empty, StatusCode::BAD_REQUEST);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deleting_a_pet_removes_its_attachment_files() {
    let dir = std::env::temp_dir().join(format!("furfiles-att-b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [2u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Koa","species":"Horse"})),
    )
    .await;
    let (_, rec) = send(
        &app,
        "POST",
        "/pets/koa/records",
        Some(json!({"occurred_on":"2026-07-01","title":"Farrier"})),
    )
    .await;
    let rid = rec["id"].as_i64().unwrap();
    send(
        &app,
        "POST",
        &format!("/records/{rid}/attachments"),
        Some(json!({"filename":"note.png","media_type":"image/png","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(count_files(&dir), 1);

    let (d, _) = send(&app, "DELETE", "/pets/koa", None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    assert_eq!(
        count_files(&dir),
        0,
        "pet delete must unlink attachment files"
    );
    // and the now-empty per-record directory is pruned, not left to accumulate
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "pet delete must prune empty per-record dirs"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn extract_preview_requires_setup_and_validates() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // not set up → 400
    let (s, _) = send(
        &app,
        "POST",
        "/extract/preview",
        Some(json!({"media_type":"application/pdf","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // set up, then unsupported media → 400 (no network reached)
    send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key":"sk-ant-xyz","enabled":true})),
    )
    .await;
    let (s, _) = send(
        &app,
        "POST",
        "/extract/preview",
        Some(json!({"media_type":"text/plain","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ---------- pet portrait photos (#21) ----------

#[tokio::test]
async fn pet_photo_set_get_replace_delete() {
    let dir = std::env::temp_dir().join(format!("furfiles-photo-a-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [3u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;

    // no photo yet → GET 404, and detail carries a null photo_path
    let (nf, _) = send(&app, "GET", "/pets/pedro/photo", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert!(detail["photo_path"].is_null());

    // non-image type is rejected
    let (bad, _) = send(
        &app,
        "PUT",
        "/pets/pedro/photo",
        Some(json!({"media_type":"application/pdf","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    // invalid base64 is rejected
    let (bad, _) = send(
        &app,
        "PUT",
        "/pets/pedro/photo",
        Some(json!({"media_type":"image/png","data_base64":"not base64 !!"})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // bytes that aren't the declared image type are rejected (#75 shares this check)
    let (bad, _) = send(
        &app,
        "PUT",
        "/pets/pedro/photo",
        Some(json!({"media_type":"image/png","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // set a portrait → 200, photo_path recorded, one file on disk
    let (s, pet) = send(
        &app,
        "PUT",
        "/pets/pedro/photo",
        Some(json!({"media_type":"image/png","data_base64":"iVBORw0KGgo="})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let first_path = pet["photo_path"].as_str().unwrap().to_string();
    assert!(first_path.starts_with("photos/pedro/") && first_path.ends_with(".png"));
    assert_eq!(count_files(&dir), 1);

    // GET streams the bytes with the stored content type
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pets/pedro/photo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "image/png"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"\x89PNG\r\n\x1a\n");

    // replacing swaps the file in place (still exactly one), new path differs
    let (s, pet) = send(
        &app,
        "PUT",
        "/pets/pedro/photo",
        Some(json!({"media_type":"image/jpeg","data_base64":"/9j/4AAQ"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let second_path = pet["photo_path"].as_str().unwrap().to_string();
    assert_ne!(second_path, first_path, "replace mints a new file");
    assert!(second_path.ends_with(".jpg"));
    assert_eq!(
        count_files(&dir),
        1,
        "the old portrait file is unlinked on replace"
    );

    // delete clears photo_path and removes the file
    let (s, pet) = send(&app, "DELETE", "/pets/pedro/photo", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(pet["photo_path"].is_null());
    assert_eq!(count_files(&dir), 0);
    let (nf, _) = send(&app, "GET", "/pets/pedro/photo", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    // photo ops on a missing pet 404
    let (nf, _) = send(
        &app,
        "PUT",
        "/pets/ghost/photo",
        Some(json!({"media_type":"image/png","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(&app, "DELETE", "/pets/ghost/photo", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn pet_photo_404s_when_the_stored_file_is_gone() {
    let dir = std::env::temp_dir().join(format!("furfiles-photo-c-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [6u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Koa","species":"Horse"})),
    )
    .await;
    let (_, pet) = send(
        &app,
        "PUT",
        "/pets/koa/photo",
        Some(json!({"media_type":"image/png","data_base64":"iVBORw0KGgo="})),
    )
    .await;
    let rel = pet["photo_path"].as_str().unwrap();

    // the pet still points at the file, but the bytes vanished from the volume
    std::fs::remove_file(dir.join(rel)).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pets/koa/photo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deleting_a_pet_removes_its_photo() {
    let dir = std::env::temp_dir().join(format!("furfiles-photo-b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [4u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Cat"})),
    )
    .await;
    send(
        &app,
        "PUT",
        "/pets/lulu/photo",
        Some(json!({"media_type":"image/webp","data_base64":"UklGRgAAAABXRUJQ"})),
    )
    .await;
    assert_eq!(count_files(&dir), 1);

    let (d, _) = send(&app, "DELETE", "/pets/lulu", None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    assert_eq!(
        count_files(&dir),
        0,
        "pet delete must unlink the portrait file"
    );
    assert!(
        !dir.join("photos/lulu").exists(),
        "and prune the pet's photo dir"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- diagnostics capture (#41) ----------

#[test]
fn diag_valid_id_rejects_traversal() {
    use furfiles::diag::valid_id;
    assert!(valid_id("a1b2-c3d4"));
    assert!(!valid_id("../etc/passwd"));
    assert!(!valid_id("a/b"));
    assert!(!valid_id(""));
}

#[tokio::test]
async fn diag_stores_enriches_lists_and_serves_the_file() {
    let dir = std::env::temp_dir().join(format!("furfiles-diagt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut st = AppState::for_test(memory_pool().await);
    st.diag_dir = dir.clone();
    let app = api_router(st);

    // POST a report WITH the triggering file and a phone user-agent header.
    let body = json!({
        "context": "photo-editor:convert",
        "message": "HEIC decode failed",
        "stack": "Error: boom",
        "url": "http://x/pets/koa",
        "extra": { "step": "heic2any" },
        "file": { "filename": "koa.heif", "media_type": "image/heic", "data_base64": "aGVsbG8=" }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/diag")
        .header("content-type", "application/json")
        .header("user-agent", "Mozilla/5.0 (Android) Firefox/130")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    // server-enriched fields
    assert_eq!(created["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(created["user_agent"], "Mozilla/5.0 (Android) Firefox/130");
    assert_eq!(created["file"]["size_bytes"], 5);
    assert!(
        created["file"].get("stored_name").is_none(),
        "internal name must not leak"
    );

    // list + fetch by id
    let (_, list) = send(&app, "GET", "/diag", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (s, one) = send(&app, "GET", &format!("/diag/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(one["message"], "HEIC decode failed");
    assert_eq!(one["extra"]["step"], "heic2any");

    // the captured file streams back byte-for-byte, with its media type
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/diag/{id}/file"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "image/heic"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"hello");

    // a report with NO file is fine; its file endpoint 404s
    let (s, no_file) = send(
        &app,
        "POST",
        "/diag",
        Some(json!({"context":"window","message":"oops"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let nid = no_file["id"].as_str().unwrap();
    let (nf, _) = send(&app, "GET", &format!("/diag/{nid}/file"), None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    // DELETE clears the whole store
    let (d, _) = send(&app, "DELETE", "/diag", None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, empty) = send(&app, "GET", "/diag", None).await;
    assert!(empty.as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn diag_store_is_ring_buffered() {
    let dir = std::env::temp_dir().join(format!("furfiles-diagr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut st = AppState::for_test(memory_pool().await);
    st.diag_dir = dir.clone();
    let app = api_router(st);

    for i in 0..30 {
        send(
            &app,
            "POST",
            "/diag",
            Some(json!({"context":"c","message":format!("m{i}")})),
        )
        .await;
    }
    let (_, list) = send(&app, "GET", "/diag", None).await;
    assert_eq!(
        list.as_array().unwrap().len(),
        25,
        "store keeps only the last 25 reports"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- record extra (custom fields) ----------

#[tokio::test]
async fn record_extra_fields_round_trip_and_surface_in_detail() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Dog"})),
    )
    .await;

    let (s, rec) = send(&app, "POST", "/pets/lulu/records",
        Some(json!({"occurred_on":"2026-07-01","title":"Rabies booster","cost_cents":8500,
            "extra": {"Rabies tag":"12345","Next dose due":"2027-07-01"},
            "line_items": [{"description":"Visit","amount_cents":5000},{"description":"Vaccine","amount_cents":3500}]}))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(rec["extra"]["Rabies tag"], "12345");
    assert_eq!(rec["line_items"][1]["description"], "Vaccine");
    assert_eq!(rec["line_items"][1]["amount_cents"], 3500);

    let (_, detail) = send(&app, "GET", "/pets/lulu", None).await;
    assert_eq!(detail["records"][0]["extra"]["Next dose due"], "2027-07-01");
    assert_eq!(
        detail["records"][0]["line_items"][0]["description"],
        "Visit"
    );

    // no extra / no breakdown → null (not "{}" / "[]")
    let (_, rec2) = send(
        &app,
        "POST",
        "/pets/lulu/records",
        Some(json!({"occurred_on":"2026-07-02","title":"Plain"})),
    )
    .await;
    assert!(rec2["extra"].is_null());
    assert!(rec2["line_items"].is_null());

    // empty extra map / empty breakdown collapse to null too
    let (_, rec3) = send(
        &app,
        "POST",
        "/pets/lulu/records",
        Some(json!({"occurred_on":"2026-07-03","title":"EmptyExtra","extra":{},"line_items":[]})),
    )
    .await;
    assert!(rec3["extra"].is_null());
    assert!(rec3["line_items"].is_null());
}

// ---------- record edit / delete (#15) ----------

#[tokio::test]
async fn record_update_is_partial_and_can_clear_or_overwrite_fields() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;

    // create with a date range + cost + extra + breakdown
    let (s, rec) = send(&app, "POST", "/pets/pedro/records",
        Some(json!({"occurred_on":"2026-07-01","ends_on":"2026-07-05","title":"Kennel cough course",
            "category":"Prescriptions","cost_cents":4200,"notes":"twice daily",
            "extra":{"Med":"Doxycycline"},"line_items":[{"description":"Doxycycline","amount_cents":4200}]}))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(rec["ends_on"], "2026-07-05");
    let rid = rec["id"].as_i64().unwrap();

    // partial: change only the title; everything else is preserved
    let (s, up) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"title":"Kennel cough — full course"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["title"], "Kennel cough — full course");
    assert_eq!(up["ends_on"], "2026-07-05"); // untouched
    assert_eq!(up["cost_cents"], 4200); // untouched
    assert_eq!(up["extra"]["Med"], "Doxycycline"); // untouched
    assert_eq!(up["category"], "Prescriptions");

    // explicit null CLEARS a nullable field (edit modal blanking the cost / range)
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"cost_cents":null,"ends_on":null})),
    )
    .await;
    assert!(up["cost_cents"].is_null(), "null clears the cost");
    assert!(up["ends_on"].is_null(), "null clears the end date");
    assert_eq!(up["title"], "Kennel cough — full course"); // still untouched

    // overwrite the breakdown, and clear the custom fields with an empty map
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"line_items":[{"description":"Recheck","amount_cents":3000}],"extra":{}})),
    )
    .await;
    assert_eq!(up["line_items"][0]["description"], "Recheck");
    assert!(up["extra"].is_null(), "empty extra collapses to null");

    // empty title is rejected
    let (bad, _) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"title":"  "})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // a record can't be edited via the wrong pet's URL, or when it doesn't exist
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Dog"})),
    )
    .await;
    let (nf, _) = send(
        &app,
        "PATCH",
        &format!("/pets/lulu/records/{rid}"),
        Some(json!({"title":"x"})),
    )
    .await;
    assert_eq!(
        nf,
        StatusCode::NOT_FOUND,
        "wrong pet can't touch the record"
    );
    let (nf, _) = send(
        &app,
        "PATCH",
        "/pets/pedro/records/999999",
        Some(json!({"title":"x"})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    // delete, then it's gone from the timeline; deleting again / wrong pet 404s
    let (d, _) = send(&app, "DELETE", &format!("/pets/pedro/records/{rid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(detail["records"].as_array().unwrap().len(), 0);
    let (nf, _) = send(&app, "DELETE", &format!("/pets/pedro/records/{rid}"), None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(&app, "DELETE", "/pets/lulu/records/1", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_record_removes_its_attachment_files() {
    let dir = std::env::temp_dir().join(format!("furfiles-att-d-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [9u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Koa","species":"Horse"})),
    )
    .await;
    let (_, rec) = send(
        &app,
        "POST",
        "/pets/koa/records",
        Some(json!({"occurred_on":"2026-07-01","title":"Farrier"})),
    )
    .await;
    let rid = rec["id"].as_i64().unwrap();
    send(
        &app,
        "POST",
        &format!("/records/{rid}/attachments"),
        Some(json!({"filename":"note.png","media_type":"image/png","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(count_files(&dir), 1);

    let (d, _) = send(&app, "DELETE", &format!("/pets/koa/records/{rid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    assert_eq!(
        count_files(&dir),
        0,
        "record delete must unlink attachment files"
    );
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "empty per-record dir is pruned"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- reminders / calendar events (#26) ----------

#[tokio::test]
async fn reminders_crud_dedup_and_calendar_list() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Dog"})),
    )
    .await;

    // create
    let (s, r) = send(
        &app,
        "POST",
        "/pets/lulu/reminders",
        Some(json!({"due_on":"2026-09-05","title":"Fecal exam","source":"scan"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(r["title"], "Fecal exam");
    assert_eq!(r["done"], false);
    let rid = r["id"].as_i64().unwrap();

    // dedup: same pet + date + case-insensitive title returns the SAME row (200), no dup
    let (s2, r2) = send(
        &app,
        "POST",
        "/pets/lulu/reminders",
        Some(json!({"due_on":"2026-09-05","title":"fecal EXAM"})),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        r2["id"].as_i64().unwrap(),
        rid,
        "dup collapses to the existing reminder"
    );
    let (_, list) = send(&app, "GET", "/pets/lulu/reminders", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1, "no duplicate created");

    // surfaces in the pet detail
    let (_, detail) = send(&app, "GET", "/pets/lulu", None).await;
    assert_eq!(detail["reminders"].as_array().unwrap().len(), 1);

    // and in the cross-pet calendar list, with the pet name joined
    let (_, cal) = send(&app, "GET", "/reminders", None).await;
    assert_eq!(cal[0]["pet_name"], "Lulu");
    assert_eq!(cal[0]["title"], "Fecal exam");

    // mark done
    let (s3, done) = send(
        &app,
        "PATCH",
        &format!("/reminders/{rid}"),
        Some(json!({"done":true})),
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(done["done"], true);

    // validation + 404s
    let (bad, _) = send(
        &app,
        "POST",
        "/pets/lulu/reminders",
        Some(json!({"due_on":"2026-01-01","title":"  "})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    let (nf, _) = send(
        &app,
        "PATCH",
        "/reminders/999999",
        Some(json!({"done":true})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nfp, _) = send(
        &app,
        "POST",
        "/pets/ghost/reminders",
        Some(json!({"due_on":"2026-01-01","title":"x"})),
    )
    .await;
    assert_eq!(nfp, StatusCode::NOT_FOUND);

    // delete + cascade on pet delete
    let (d, _) = send(&app, "DELETE", &format!("/reminders/{rid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, empty) = send(&app, "GET", "/pets/lulu/reminders", None).await;
    assert_eq!(empty.as_array().unwrap().len(), 0);
    let (dn, _) = send(&app, "DELETE", "/reminders/999999", None).await;
    assert_eq!(dn, StatusCode::NOT_FOUND);
}

/// "What's next" (#35): both the list and detail payloads surface the soonest
/// NOT-done reminder, overdue included, and it advances as reminders are done.
#[tokio::test]
async fn next_reminder_reflects_soonest_not_done() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Nova","species":"Cat"})),
    )
    .await;

    // No reminders yet → next_reminder is null in both payloads.
    let (_, pets) = send(&app, "GET", "/pets", None).await;
    let nova = pets.as_array().unwrap().iter().find(|p| p["id"] == "nova").unwrap().clone();
    assert!(nova["next_reminder"].is_null(), "caught up → null");
    let (_, detail) = send(&app, "GET", "/pets/nova", None).await;
    assert!(detail["next_reminder"].is_null());

    // Add a far reminder, then an overdue one and a nearer one.
    for (due, title) in [
        ("2027-01-01", "Annual exam"),
        ("2020-01-01", "Overdue rabies"),
        ("2026-08-01", "FVRCP booster"),
    ] {
        send(
            &app,
            "POST",
            "/pets/nova/reminders",
            Some(json!({"due_on": due, "title": title})),
        )
        .await;
    }

    // Overdue (earliest due_on) sorts first → it's "what's next".
    let (_, detail) = send(&app, "GET", "/pets/nova", None).await;
    assert_eq!(detail["next_reminder"]["title"], "Overdue rabies");
    let (_, pets) = send(&app, "GET", "/pets", None).await;
    let nova = pets.as_array().unwrap().iter().find(|p| p["id"] == "nova").unwrap().clone();
    assert_eq!(nova["next_reminder"]["title"], "Overdue rabies");

    // Marking it done advances to the next soonest.
    let rid = detail["next_reminder"]["id"].as_i64().unwrap();
    send(
        &app,
        "PATCH",
        &format!("/reminders/{rid}"),
        Some(json!({"done": true})),
    )
    .await;
    let (_, detail) = send(&app, "GET", "/pets/nova", None).await;
    assert_eq!(detail["next_reminder"]["title"], "FVRCP booster");
}

// ---------- shared contacts (#20) ----------

#[tokio::test]
async fn contact_crud_validates_and_edits_partially() {
    let app = api_router(AppState::for_test(memory_pool().await));

    // create
    let (s, c) = send(
        &app,
        "POST",
        "/contacts",
        Some(
            json!({"name":"Cedar Creek Animal Hospital","role":"vet","doctor":"Dr. Okafor",
            "phone":"555-0100","email":"care@cedarcreek.example","address":"12 Oak Rd",
            "website":"https://cedarcreek.example","notes":"Ask for Dr. Okafor"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(c["name"], "Cedar Creek Animal Hospital");
    assert_eq!(c["role"], "vet");
    assert_eq!(c["doctor"], "Dr. Okafor");
    let cid = c["id"].as_str().unwrap().to_string();

    // validation: blank name, unknown role
    let (bad, _) = send(&app, "POST", "/contacts", Some(json!({"name":"  "}))).await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    let (bad, _) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"X","role":"wizard"})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // role defaults to vet when omitted
    let (_, d) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Corner Groomers","role":"groomer"})),
    )
    .await;
    assert_eq!(d["role"], "groomer");
    let (_, e) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Nameless role"})),
    )
    .await;
    assert_eq!(e["role"], "vet");

    // list, and filter by role
    let (_, all) = send(&app, "GET", "/contacts", None).await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    let (_, vets) = send(&app, "GET", "/contacts?role=vet", None).await;
    assert_eq!(
        vets.as_array().unwrap().len(),
        2,
        "the two vet-role contacts"
    );
    let (_, groomers) = send(&app, "GET", "/contacts?role=groomer", None).await;
    assert_eq!(groomers.as_array().unwrap().len(), 1);

    // get one
    let (s, one) = send(&app, "GET", &format!("/contacts/{cid}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(one["phone"], "555-0100");

    // partial edit: change only the phone; name/role/doctor preserved
    let (s, up) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"phone":"555-0199"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["phone"], "555-0199");
    assert_eq!(up["name"], "Cedar Creek Animal Hospital");
    assert_eq!(up["doctor"], "Dr. Okafor");

    // explicit null clears a nullable field; role can be re-typed
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"doctor":null,"role":"emergency"})),
    )
    .await;
    assert!(up["doctor"].is_null());
    assert_eq!(up["role"], "emergency");

    // rejections on edit: empty name, unknown role
    let (bad, _) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"name":"  "})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    let (bad, _) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"role":"wizard"})),
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);

    // 404s on a missing contact
    let (nf, _) = send(&app, "GET", "/contacts/nope", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(&app, "PATCH", "/contacts/nope", Some(json!({"phone":"x"}))).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(&app, "DELETE", "/contacts/nope", None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    // delete
    let (d, _) = send(&app, "DELETE", &format!("/contacts/{cid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (nf, _) = send(&app, "GET", &format!("/contacts/{cid}"), None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pet_care_team_link_is_shared_idempotent_and_unlinks() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // Two pets share one small-animal vet; the horse has a separate equine vet (#20).
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Cat"})),
    )
    .await;
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Koa","species":"Horse"})),
    )
    .await;
    let (_, vet) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Cedar Creek","role":"vet"})),
    )
    .await;
    let (_, equine) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Valley Equine","role":"vet"})),
    )
    .await;
    let vet_id = vet["id"].as_str().unwrap().to_string();
    let equine_id = equine["id"].as_str().unwrap().to_string();

    // link the shared vet to both small animals; the equine vet to the horse
    let (s, team) = send(
        &app,
        "POST",
        "/pets/pedro/contacts",
        Some(json!({"contact_id":vet_id})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(team.as_array().unwrap().len(), 1);
    assert_eq!(team[0]["name"], "Cedar Creek");
    send(
        &app,
        "POST",
        "/pets/lulu/contacts",
        Some(json!({"contact_id":vet_id})),
    )
    .await;
    send(
        &app,
        "POST",
        "/pets/koa/contacts",
        Some(json!({"contact_id":equine_id})),
    )
    .await;

    // linking the same contact again is idempotent (no dup, no error)
    let (s, team) = send(
        &app,
        "POST",
        "/pets/pedro/contacts",
        Some(json!({"contact_id":vet_id})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(team.as_array().unwrap().len(), 1, "re-link is a no-op");

    // the care team surfaces on GET /pets/:id/contacts and in the pet detail
    let (_, list) = send(&app, "GET", "/pets/pedro/contacts", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(detail["contacts"].as_array().unwrap().len(), 1);
    assert_eq!(detail["contacts"][0]["name"], "Cedar Creek");
    let (_, koa) = send(&app, "GET", "/pets/koa", None).await;
    assert_eq!(
        koa["contacts"][0]["name"], "Valley Equine",
        "horse has its own equine vet"
    );

    // deleting the shared contact unlinks it from every pet (cascade), history stays
    let (d, _) = send(&app, "DELETE", &format!("/contacts/{vet_id}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert!(
        detail["contacts"].as_array().unwrap().is_empty(),
        "care-team link cascades on contact delete"
    );
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert!(lulu["contacts"].as_array().unwrap().is_empty());

    // 404s: link to a missing pet, link a missing contact, unlink an unlinked pair
    let (nf, _) = send(
        &app,
        "POST",
        "/pets/ghost/contacts",
        Some(json!({"contact_id":equine_id})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(
        &app,
        "POST",
        "/pets/pedro/contacts",
        Some(json!({"contact_id":"nope"})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (nf, _) = send(
        &app,
        "DELETE",
        &format!("/pets/pedro/contacts/{equine_id}"),
        None,
    )
    .await;
    assert_eq!(
        nf,
        StatusCode::NOT_FOUND,
        "unlinking a pair that isn't linked 404s"
    );

    // a real unlink drops the horse's equine vet
    let (d, _) = send(
        &app,
        "DELETE",
        &format!("/pets/koa/contacts/{equine_id}"),
        None,
    )
    .await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, koa) = send(&app, "GET", "/pets/koa", None).await;
    assert!(koa["contacts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn record_contact_link_validates_resolves_and_survives_contact_delete() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;
    let (_, vet) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Cedar Creek","role":"vet"})),
    )
    .await;
    let cid = vet["id"].as_str().unwrap().to_string();

    // create a record naming the clinic
    let (s, rec) = send(
        &app,
        "POST",
        "/pets/pedro/records",
        Some(json!({"occurred_on":"2026-07-01","title":"Annual exam","contact_id":cid})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(rec["contact_id"], cid);
    let rid = rec["id"].as_i64().unwrap();

    // the detail resolves contact_id to the full contact for display
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(detail["records"][0]["contact"]["name"], "Cedar Creek");
    assert_eq!(detail["records"][0]["contact_id"], cid);

    // a record can't point at a contact that doesn't exist
    let (nf, _) = send(
        &app,
        "POST",
        "/pets/pedro/records",
        Some(json!({"occurred_on":"2026-07-02","title":"x","contact_id":"nope"})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);

    // edit: clear the link with null, then re-set it; validate on set
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"contact_id":null})),
    )
    .await;
    assert!(up["contact_id"].is_null());
    let (nf, _) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"contact_id":"nope"})),
    )
    .await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/pets/pedro/records/{rid}"),
        Some(json!({"contact_id":cid})),
    )
    .await;
    assert_eq!(up["contact_id"], cid);

    // deleting the contact must NOT delete the record — it just clears the link
    let (d, _) = send(&app, "DELETE", &format!("/contacts/{cid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(
        detail["records"].as_array().unwrap().len(),
        1,
        "history is preserved"
    );
    assert!(
        detail["records"][0]["contact_id"].is_null(),
        "ON DELETE SET NULL unlinks the record"
    );
    assert!(detail["records"][0]["contact"].is_null());
}

// ---------- facility web parser (#42) ----------

#[test]
fn is_public_ip_rejects_private_loopback_linklocal_and_cgnat() {
    use furfiles::web::is_public_ip;
    use std::net::IpAddr;
    // public: v4 and a real v6.
    for s in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
        assert!(
            is_public_ip(&s.parse::<IpAddr>().unwrap()),
            "{s} should be public"
        );
    }
    // non-public: private, loopback, link-local (incl. cloud metadata), CGNAT,
    // documentation (203.0.113/24 = TEST-NET-3), v6 loopback/ULA/link-local, and
    // a v4-mapped loopback.
    for s in [
        "127.0.0.1",
        "10.0.0.1",
        "192.168.1.1",
        "172.16.5.4",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "203.0.113.10",
        "::1",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
    ] {
        assert!(
            !is_public_ip(&s.parse::<IpAddr>().unwrap()),
            "{s} should be rejected"
        );
    }
}

#[test]
fn validate_url_shallow_enforces_scheme_and_blocks_internal() {
    use furfiles::web::validate_url_shallow;
    use reqwest::Url;
    let ok = |u: &str| validate_url_shallow(&Url::parse(u).unwrap());
    assert!(ok("https://example.com/policies").is_ok());
    assert!(ok("http://example.com").is_ok());
    // scheme
    assert!(ok("ftp://example.com").is_err());
    assert!(ok("file:///etc/passwd").is_err());
    // blocked names
    assert!(ok("http://localhost/").is_err());
    assert!(ok("http://foo.localhost/").is_err());
    assert!(ok("http://metadata.google.internal/").is_err());
    // private / link-local IP literals
    assert!(ok("http://127.0.0.1/").is_err());
    assert!(ok("http://169.254.169.254/latest/meta-data/").is_err());
    assert!(ok("http://192.168.1.10/").is_err());
    assert!(ok("http://[::1]/").is_err());
}

#[test]
fn html_to_text_strips_scripts_tags_entities_and_truncates() {
    use furfiles::web::html_to_text;
    let html = "<html><head><title>x</title></head><body>\
        <script>alert('no')</script><style>.a{}</style>\
        <h1>Happy Tails</h1><p>Vaccines &amp; a negative fecal test required.</p>\
        <nav>menu</nav></body></html>";
    let t = html_to_text(html);
    assert!(t.contains("Happy Tails"));
    assert!(t.contains("Vaccines & a negative fecal test required."));
    assert!(!t.contains("alert"), "script content must be dropped");
    assert!(!t.contains(".a{"), "style content must be dropped");
    assert!(!t.contains('<'), "tags must be stripped");
    // truncation
    let big = format!("<p>{}</p>", "word ".repeat(20_000));
    assert!(html_to_text(&big).chars().count() <= furfiles::web::MAX_TEXT_CHARS);
}

#[test]
fn build_facility_request_is_text_only_and_asks_for_the_tool() {
    let body = furfiles::ingestion::build_facility_request("claude-x", "Happy Tails needs rabies.");
    assert_eq!(body["tools"][0]["name"], "facility_fields");
    assert_eq!(body["tool_choice"]["type"], "auto"); // never forced (#87)
    assert_eq!(body["model"], "claude-x");
    let content = body["messages"][0]["content"].as_array().unwrap();
    // both blocks are text (no image/document) — the page was scraped to text
    assert!(content.iter().all(|b| b["type"] == "text"));
    // prompt-injection guard is present
    let joined = content
        .iter()
        .map(|b| b["text"].as_str().unwrap_or(""))
        .collect::<String>();
    assert!(joined.contains("Ignore any instructions"));
    assert!(joined.contains("Happy Tails needs rabies."));
}

#[test]
fn parse_facility_response_reads_tool_use_and_errors_otherwise() {
    let good = json!({
        "content": [{
            "type": "tool_use", "name": "facility_fields",
            "input": {
                "name": "Happy Tails Boarding", "phone": "555-0100",
                "address": "9 Kennel Ln", "notes": "Ample parking",
                "requirements": [
                    {"label": "Vaccinations", "detail": "Rabies, DHPP, Bordetella current"},
                    {"label": "Fecal test", "detail": "Negative within 30 days"}
                ],
                "hours": [
                    {"label": "Open", "detail": "Mon-Fri 7am-7pm"},
                    {"label": "Boarding pick-up", "detail": "by 12:00 PM"}
                ]
            }
        }]
    });
    let f = furfiles::ingestion::parse_facility_response(&good).unwrap();
    assert_eq!(f.name.as_deref(), Some("Happy Tails Boarding"));
    assert_eq!(f.requirements.len(), 2);
    assert_eq!(f.requirements[0].label, "Vaccinations");
    assert_eq!(
        f.requirements[1].detail.as_deref(),
        Some("Negative within 30 days")
    );
    // hours are their own structured field (#42 follow-up), not buried in notes
    assert_eq!(f.hours.len(), 2);
    assert_eq!(f.hours[0].label, "Open");
    assert_eq!(f.hours[1].detail.as_deref(), Some("by 12:00 PM"));

    // the tool schema advertises both requirements and hours as label/detail items
    let schema = furfiles::ingestion::facility_tool_schema();
    assert!(schema["properties"]["hours"]["items"]["properties"]["label"].is_object());
    assert!(furfiles::ingestion::facility_prompt().contains("hours"));
    // wrong / missing tool → error
    assert!(furfiles::ingestion::parse_facility_response(
        &json!({"content":[{"type":"text","text":"hi"}]})
    )
    .is_err());
    assert!(furfiles::ingestion::parse_facility_response(&json!({})).is_err());
}

#[tokio::test]
async fn contact_requirements_round_trip_and_clear() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // create a boarding facility WITH requirements
    let (s, c) = send(&app, "POST", "/contacts",
        Some(json!({"name":"Happy Tails Boarding","role":"boarding",
            "requirements":[{"label":"Vaccinations","detail":"Rabies + DHPP"},{"label":"Food","detail":"Pre-portioned"}]}))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(c["requirements"][0]["label"], "Vaccinations");
    assert_eq!(c["requirements"][1]["detail"], "Pre-portioned");
    let cid = c["id"].as_str().unwrap().to_string();

    // empty requirements list collapses to null (not "[]")
    let (_, c2) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Bare","role":"boarding","requirements":[]})),
    )
    .await;
    assert!(c2["requirements"].is_null());
    // omitted → null
    let (_, c3) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"NoReq","role":"vet"})),
    )
    .await;
    assert!(c3["requirements"].is_null());

    // PATCH overwrites the checklist; omitting the key keeps it
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"requirements":[{"label":"Fecal test"}]})),
    )
    .await;
    assert_eq!(up["requirements"].as_array().unwrap().len(), 1);
    assert_eq!(up["requirements"][0]["label"], "Fecal test");
    assert!(up["requirements"][0]["detail"].is_null());
    let (_, up2) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"phone":"555"})),
    )
    .await;
    assert_eq!(
        up2["requirements"].as_array().unwrap().len(),
        1,
        "omitted requirements are kept"
    );
    // explicit empty list clears it
    let (_, up3) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"requirements":[]})),
    )
    .await;
    assert!(up3["requirements"].is_null());
}

#[tokio::test]
async fn contact_hours_round_trip_and_keep_on_unrelated_patch() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // hours apply to any contact, vets included (#42 follow-up).
    let (s, c) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"Oak Hollow Animal Clinic","role":"vet",
            "hours":[{"label":"Open","detail":"Mon-Fri 7am-7pm"},{"label":"Sat","detail":"7am-1pm"}]})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(c["hours"][0]["label"], "Open");
    assert_eq!(c["hours"][1]["detail"], "7am-1pm");
    assert!(
        c["requirements"].is_null(),
        "a vet has hours but no requirements"
    );
    let cid = c["id"].as_str().unwrap().to_string();

    // empty hours list collapses to null; omitted stays null
    let (_, bare) = send(
        &app,
        "POST",
        "/contacts",
        Some(json!({"name":"NoHours","role":"vet","hours":[]})),
    )
    .await;
    assert!(bare["hours"].is_null());

    // an unrelated PATCH keeps hours; explicit [] clears them
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"phone":"555-010-0177"})),
    )
    .await;
    assert_eq!(
        up["hours"].as_array().unwrap().len(),
        2,
        "omitted hours are kept"
    );
    let (_, up2) = send(
        &app,
        "PATCH",
        &format!("/contacts/{cid}"),
        Some(json!({"hours":[]})),
    )
    .await;
    assert!(up2["hours"].is_null());
    // and the pet-detail care team carries hours too
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;
    send(
        &app,
        "POST",
        "/pets/pedro/contacts",
        Some(json!({"contact_id":cid})),
    )
    .await;
    let (_, detail) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(detail["contacts"][0]["name"], "Oak Hollow Animal Clinic");
}

#[tokio::test]
async fn parse_url_gates_on_setup_then_guards_against_ssrf() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // not set up → 400 before anything (no fetch, no tokens)
    let (s, _) = send(
        &app,
        "POST",
        "/contacts/parse-url",
        Some(json!({"url":"https://example.com"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // set up ingestion (key + enabled)
    send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key":"sk-ant-xyz","enabled":true})),
    )
    .await;

    // SSRF + validation: each of these is rejected BEFORE any network fetch, so
    // no real request leaves the process during the test.
    for bad in [
        "http://127.0.0.1/",
        "http://169.254.169.254/latest/meta-data/",
        "http://localhost/policies",
        "http://192.168.1.10/",
        "ftp://example.com/",
        "not a url",
    ] {
        let (s, _) = send(
            &app,
            "POST",
            "/contacts/parse-url",
            Some(json!({"url": bad})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad} must be rejected");
    }
}

// ---------- facility fetch quality (#42 follow-up): keep title/JSON-LD + crawl ----------

#[test]
fn json_ld_and_title_survive_html_to_text() {
    use furfiles::web::html_to_text;
    // Real-world shape: the name/phone/address live in <title> and JSON-LD, not
    // the rendered body. Over-stripping those (the original bug) returned junk.
    let html = "<html><head><title>Happy Tails Daycare - Dog Daycare and Boarding</title>\
        <script>var analytics=1;garbage()</script>\
        <script type=\"application/ld+json\">{\"name\":\"Happy Tails Daycare\",\"telephone\":\"5550100188\",\
        \"address\":\"12 Example Terrace, Springfield\"}</script>\
        <style>.x{color:red}</style></head>\
        <body><nav>Home FAQs Policies</nav><h1>Welcome</h1></body></html>";
    let t = html_to_text(html);
    assert!(
        t.contains("Happy Tails Daycare - Dog Daycare and Boarding"),
        "title kept: {t}"
    );
    assert!(t.contains("Structured data (JSON-LD):"), "json-ld labeled");
    assert!(
        t.contains("5550100188") && t.contains("12 Example Terrace"),
        "json-ld payload kept"
    );
    assert!(t.contains("Welcome"), "body kept");
    assert!(!t.contains("garbage()"), "non-ld script dropped");
    assert!(!t.contains("color:red"), "style dropped");
}

#[test]
fn tag_attr_and_crawl_links_pick_same_origin_requirement_pages() {
    use furfiles::web::{crawl_links, tag_attr};
    use reqwest::Url;
    // tag_attr: quoted, single-quoted, unquoted, and boundary (data-href must not match)
    assert_eq!(
        tag_attr("<a href=\"/policies\">", "href").as_deref(),
        Some("/policies")
    );
    assert_eq!(tag_attr("<a href='/faq'>", "href").as_deref(), Some("/faq"));
    assert_eq!(
        tag_attr("<a href=/rules >", "href").as_deref(),
        Some("/rules")
    );
    assert_eq!(
        tag_attr("<a data-href=\"/x\">", "href"),
        None,
        "boundary: data-href isn't href"
    );

    let base = Url::parse("https://facility.example/").unwrap();
    let html = "\
        <a href=\"/policies\">Policies</a> \
        <a href=\"/faqs\">FAQs</a> \
        <a href=\"https://facility.example/new-client\">New Client Forms</a> \
        <a href=\"/about\">About</a> \
        <a href=\"https://evil.example/policies\">off-site policies</a> \
        <a href=\"/gallery\">See our Boarding suites</a>";
    let links = crawl_links(&base, html, furfiles::web::CRAWL_KEYWORDS, 4, &[]);
    let paths: Vec<_> = links.iter().map(|u| u.path().to_string()).collect();
    assert!(paths.contains(&"/policies".to_string()));
    assert!(paths.contains(&"/faqs".to_string()));
    assert!(paths.contains(&"/new-client".to_string()));
    // matched by link TEXT ("Boarding") even though the href (/gallery) doesn't
    assert!(
        paths.contains(&"/gallery".to_string()),
        "matched on link text"
    );
    // /about doesn't match; off-site is excluded even though its path matches
    assert!(!paths.contains(&"/about".to_string()));
    assert!(
        links
            .iter()
            .all(|u| u.host_str() == Some("facility.example")),
        "same-origin only"
    );
    assert!(links.len() <= 4, "capped");
}

#[test]
fn people_pages_are_crawled_separately_from_requirement_pages() {
    use furfiles::web::crawl_links;
    let base = reqwest::Url::parse("https://barkside.example/").unwrap();
    let html = r#"<a href="/faq">FAQ</a> <a href="/policies">Policies</a>
        <a href="/meet-the-team">Meet the Team</a> <a href="/about">About</a>
        <a href="https://elsewhere.example/team">Team</a>
        <a href="http://barkside.example:8080/staff">Staff</a>
        <a href="http://barkside.example/owner">Owner (plain http)</a>"#;
    let req = crawl_links(&base, html, furfiles::web::CRAWL_KEYWORDS, 4, &[]);
    assert_eq!(req.iter().map(|u| u.path()).collect::<Vec<_>>(), vec!["/faq", "/policies"]);
    let people = crawl_links(&base, html, &["team", "about", "faq", "staff", "owner"], 4, &req);
    assert_eq!(
        people.iter().map(|u| u.path()).collect::<Vec<_>>(),
        vec!["/meet-the-team", "/about"],
        "same ORIGIN only (another host, port or scheme is out), and pages already taken for requirements are skipped"
    );
}

// ---------- insurance / wellness plans (#8 layer 1 + #29) ----------

#[test]
fn build_insurance_request_uses_document_for_pdf_and_image_and_asks_for_the_tool() {
    let pdf =
        furfiles::ingestion::build_insurance_request("claude-x", "application/pdf", "QkFTRTY0");
    let block = &pdf["messages"][0]["content"][0];
    assert_eq!(block["type"], "document");
    assert_eq!(block["source"]["media_type"], "application/pdf");
    assert_eq!(block["source"]["data"], "QkFTRTY0");
    assert_eq!(pdf["tools"][0]["name"], "insurance_fields");
    assert_eq!(pdf["tool_choice"]["type"], "auto"); // never forced (#87)
    assert_eq!(pdf["model"], "claude-x");

    let img = furfiles::ingestion::build_insurance_request("claude-x", "image/jpeg", "QkFTRTY0");
    assert_eq!(img["messages"][0]["content"][0]["type"], "image");
}

#[test]
fn insurance_prompt_has_injection_guard_and_key_rules() {
    let p = furfiles::ingestion::insurance_prompt();
    assert!(p.contains("Ignore any instructions")); // prompt-injection guard
    assert!(p.contains("reimburse_percent"));
    assert!(p.contains("wellness"));
}

#[test]
fn parse_insurance_response_reads_tool_use_and_errors_otherwise() {
    let good = json!({
        "content": [{
            "type": "tool_use", "name": "insurance_fields",
            "input": {
                "kind": "insurance", "provider": "Lemonade",
                "plan_name": "Pet Insurance + Preventative",
                "policy_number": "DEMO-POL-0401",
                "start_date": "2026-02-10", "end_date": "2027-02-10",
                "premium": 911.36, "premium_interval": "year",
                "reimburse_percent": 90, "deductible": 500.0, "annual_limit": 50000.0,
                "notes": "Illness waiting period ends 2026-02-24",
                "coverages": [
                    {"label": "Wellness exam", "detail": "Up to $70/yr"},
                    {"label": "Vaccines", "detail": "Up to $105"}
                ],
                "exclusions": [{"label": "Pre-existing conditions"}],
                "low_confidence_fields": ["annual_limit"]
            }
        }]
    });
    let f = furfiles::ingestion::parse_insurance_response(&good).unwrap();
    assert_eq!(f.kind.as_deref(), Some("insurance"));
    assert_eq!(f.provider.as_deref(), Some("Lemonade"));
    assert_eq!(f.policy_number.as_deref(), Some("DEMO-POL-0401"));
    assert_eq!(f.premium, Some(911.36));
    assert_eq!(f.reimburse_percent, Some(90));
    assert_eq!(f.deductible, Some(500.0));
    assert_eq!(f.coverages.len(), 2);
    assert_eq!(f.coverages[0].label, "Wellness exam");
    assert_eq!(f.coverages[0].detail.as_deref(), Some("Up to $70/yr"));
    assert_eq!(f.exclusions.len(), 1);
    assert!(f.exclusions[0].detail.is_none());
    assert_eq!(f.low_confidence_fields, vec!["annual_limit".to_string()]);

    // wrong tool / empty → error
    assert!(furfiles::ingestion::parse_insurance_response(
        &json!({"content":[{"type":"tool_use","name":"record_fields","input":{}}]})
    )
    .is_err());
    assert!(furfiles::ingestion::parse_insurance_response(&json!({})).is_err());
}

#[tokio::test]
async fn insurance_crud_and_pet_detail() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Dog"})),
    )
    .await;

    // create a full Lemonade-shaped policy
    let (s, p) = send(
        &app,
        "POST",
        "/pets/lulu/insurance",
        Some(json!({
            "provider":"Lemonade","kind":"insurance","plan_name":"Pet + Preventative",
            "policy_number":"DEMO-POL-0401","status":"active",
            "start_date":"2026-02-10","end_date":"2027-02-10",
            "premium_cents":91136,"premium_interval":"year","reimburse_percent":90,
            "deductible_cents":50000,"annual_limit_cents":5000000,
            "notes":"90% after deductible",
            "coverages":[{"label":"Wellness exam","detail":"Up to $70/yr"}],
            "exclusions":[{"label":"Pre-existing"}]
        })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(p["provider"], "Lemonade");
    assert_eq!(p["reimburse_percent"], 90);
    assert_eq!(p["deductible_cents"], 50000);
    assert_eq!(p["coverages"][0]["label"], "Wellness exam");
    assert_eq!(p["exclusions"][0]["label"], "Pre-existing");
    assert!(
        p.get("document_path").is_none(),
        "internal document path must not leak"
    );
    assert!(p["document_name"].is_null(), "no document yet");
    let pid = p["id"].as_str().unwrap().to_string();

    // list on the pet
    let (_, list) = send(&app, "GET", "/pets/lulu/insurance", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // surfaces in the pet detail
    let (_, detail) = send(&app, "GET", "/pets/lulu", None).await;
    assert_eq!(detail["insurance_policies"][0]["id"], pid);

    // get one
    let (s, one) = send(&app, "GET", &format!("/insurance/{pid}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(one["policy_number"], "DEMO-POL-0401");

    // PATCH: set-or-keep provider; clear a nullable numeric with null; keep others by omission
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/insurance/{pid}"),
        Some(json!({"status":"expired","annual_limit_cents":null})),
    )
    .await;
    assert_eq!(up["status"], "expired");
    assert!(up["annual_limit_cents"].is_null(), "explicit null cleared it");
    assert_eq!(up["provider"], "Lemonade", "omitted provider kept");
    assert_eq!(up["reimburse_percent"], 90, "omitted numeric kept");

    // delete
    let (d, _) = send(&app, "DELETE", &format!("/insurance/{pid}"), None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    let (nf, _) = send(&app, "GET", &format!("/insurance/{pid}"), None).await;
    assert_eq!(nf, StatusCode::NOT_FOUND);
    let (_, list) = send(&app, "GET", "/pets/lulu/insurance", None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn insurance_validates_input_and_404s() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;

    // blank provider → 400
    let (s, _) = send(
        &app,
        "POST",
        "/pets/pedro/insurance",
        Some(json!({"provider":"  "})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // unknown kind / status → 400
    let (s, _) = send(
        &app,
        "POST",
        "/pets/pedro/insurance",
        Some(json!({"provider":"X","kind":"gap"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(
        &app,
        "POST",
        "/pets/pedro/insurance",
        Some(json!({"provider":"X","status":"lapsed"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // policy on a missing pet → 404
    let (s, _) = send(
        &app,
        "POST",
        "/pets/ghost/insurance",
        Some(json!({"provider":"X"})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // patch/delete missing policy → 404
    let (s, _) = send(
        &app,
        "PATCH",
        "/insurance/nope",
        Some(json!({"provider":"Y"})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(&app, "DELETE", "/insurance/nope", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // patch cannot blank provider or set a bad status
    let (_, p) = send(
        &app,
        "POST",
        "/pets/pedro/insurance",
        Some(json!({"provider":"Trupanion"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/insurance/{pid}"),
        Some(json!({"provider":"   "})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/insurance/{pid}"),
        Some(json!({"status":"???"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn insurance_coverages_round_trip_and_clear() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Koa","species":"Horse"})),
    )
    .await;
    // empty list collapses to null
    let (_, p) = send(
        &app,
        "POST",
        "/pets/koa/insurance",
        Some(json!({"provider":"X","coverages":[],"exclusions":[{"label":"Colic"}]})),
    )
    .await;
    assert!(p["coverages"].is_null());
    assert_eq!(p["exclusions"][0]["label"], "Colic");
    let pid = p["id"].as_str().unwrap().to_string();

    // PATCH overwrites; omitting keeps
    let (_, up) = send(
        &app,
        "PATCH",
        &format!("/insurance/{pid}"),
        Some(json!({"coverages":[{"label":"Surgery"}]})),
    )
    .await;
    assert_eq!(up["coverages"].as_array().unwrap().len(), 1);
    let (_, up2) = send(
        &app,
        "PATCH",
        &format!("/insurance/{pid}"),
        Some(json!({"notes":"x"})),
    )
    .await;
    assert_eq!(
        up2["coverages"].as_array().unwrap().len(),
        1,
        "omitted coverages kept"
    );
    assert_eq!(up2["exclusions"][0]["label"], "Colic", "omitted kept");
    // explicit empty clears
    let (_, up3) = send(
        &app,
        "PATCH",
        &format!("/insurance/{pid}"),
        Some(json!({"coverages":[],"exclusions":[]})),
    )
    .await;
    assert!(up3["coverages"].is_null());
    assert!(up3["exclusions"].is_null());
}

#[tokio::test]
async fn insurance_document_set_get_replace_delete_and_pet_delete() {
    let dir = std::env::temp_dir().join(format!("furfiles-ins-doc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = api_router(AppState::new(memory_pool().await, [3u8; 32], dir.clone()));

    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Dog"})),
    )
    .await;
    let (_, p) = send(
        &app,
        "POST",
        "/pets/lulu/insurance",
        Some(json!({"provider":"Lemonade"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    // unsupported media → 400
    let (s, _) = send(
        &app,
        "PUT",
        &format!("/insurance/{pid}/document"),
        Some(json!({"filename":"p.txt","media_type":"text/plain","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // no document yet → 404
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/insurance/{pid}/document"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // attach a PDF ("hello")
    let (s, up) = send(
        &app,
        "PUT",
        &format!("/insurance/{pid}/document"),
        Some(json!({"filename":"policy.pdf","media_type":"application/pdf","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["document_name"], "policy.pdf");
    assert_eq!(up["document_media_type"], "application/pdf");
    assert_eq!(up["document_size_bytes"], 5);
    assert!(up.get("document_path").is_none(), "internal path never leaks");
    assert_eq!(count_files(&dir), 1);

    // fetch bytes inline
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/insurance/{pid}/document"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/pdf"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello");

    // replace with a PNG → old file removed, still one file, media type updated
    let (_, up2) = send(
        &app,
        "PUT",
        &format!("/insurance/{pid}/document"),
        Some(json!({"filename":"policy.png","media_type":"image/png","data_base64":"aGk="})),
    )
    .await;
    assert_eq!(up2["document_media_type"], "image/png");
    assert_eq!(up2["document_size_bytes"], 2);
    assert_eq!(count_files(&dir), 1, "replace removes the old file");

    // delete the document (keeps the policy)
    let (_, up3) = send(&app, "DELETE", &format!("/insurance/{pid}/document"), None).await;
    assert!(up3["document_name"].is_null());
    assert!(up3["document_media_type"].is_null());
    assert_eq!(count_files(&dir), 0);

    // re-attach, then delete the PET → the doc tree is cleaned up
    send(
        &app,
        "PUT",
        &format!("/insurance/{pid}/document"),
        Some(json!({"filename":"policy.pdf","media_type":"application/pdf","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(count_files(&dir), 1);
    let (d, _) = send(&app, "DELETE", "/pets/lulu", None).await;
    assert_eq!(d, StatusCode::NO_CONTENT);
    assert_eq!(count_files(&dir), 0, "pet delete unlinks its policy documents");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn insurance_extract_requires_setup_then_validates_input() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // not set up → 400
    let (s, _) = send(
        &app,
        "POST",
        "/insurance/extract",
        Some(json!({"media_type":"application/pdf","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key":"sk-ant-xyz","enabled":true})),
    )
    .await;
    // unsupported media → 400
    let (s, _) = send(
        &app,
        "POST",
        "/insurance/extract",
        Some(json!({"media_type":"text/plain","data_base64":"aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // invalid base64 → 400
    let (s, _) = send(
        &app,
        "POST",
        "/insurance/extract",
        Some(json!({"media_type":"application/pdf","data_base64":"not base64 !!"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // oversized → 413
    let big = "A".repeat(27_962_028);
    let (s, _) = send(
        &app,
        "POST",
        "/insurance/extract",
        Some(json!({"media_type":"application/pdf","data_base64": big})),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

// ---------- Standing care fields + ongoing care (#63) ----------
//
// Driven by a typical vet intake form: colour, known allergies, current diet,
// current medications, and "is your pet current on flea/tick/heartworm
// prevention, and what brand".

/// Create a bare pet and return its id.
async fn a_pet(app: &axum::Router, name: &str) -> String {
    let (status, pet) = send(
        app,
        "POST",
        "/pets",
        Some(json!({ "name": name, "species": "Dog" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    pet["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn pet_carries_colour_and_allergies() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let (status, pet) = send(
        &app,
        "POST",
        "/pets",
        Some(json!({
            "name": "Pedro", "species": "Dog", "breed": "Mix, Australian Shep",
            "color": "Blue Merle",
            "allergies": [{ "label": "Chicken", "detail": "hives, ear infections" }],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(pet["color"], "Blue Merle");
    assert_eq!(pet["allergies"][0]["label"], "Chicken");
    assert_eq!(pet["allergies"][0]["detail"], "hives, ear infections");

    // They survive a round trip through the detail payload.
    let id = pet["id"].as_str().unwrap();
    let (_, detail) = send(&app, "GET", &format!("/pets/{id}"), None).await;
    assert_eq!(detail["color"], "Blue Merle");
    assert_eq!(detail["allergies"][0]["label"], "Chicken");
}

#[tokio::test]
async fn pet_with_no_allergies_stores_null_not_empty_list() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let (_, pet) = send(
        &app,
        "POST",
        "/pets",
        Some(json!({ "name": "Lulu", "species": "Dog", "allergies": [] })),
    )
    .await;
    // An empty list is NULL on the wire — the intake form's "No", not "[]".
    assert!(pet["allergies"].is_null());
    assert!(pet["color"].is_null());
}

#[tokio::test]
async fn pet_edit_can_clear_a_field_not_just_set_it() {
    // Regression: update_pet used blanket COALESCE, so a value could be set but
    // never cleared — a mistyped microchip number was permanent.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Koa").await;

    let (status, pet) = send(
        &app,
        "PATCH",
        &format!("/pets/{id}"),
        Some(json!({ "microchip": "000000000000202", "color": "Bay" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pet["microchip"], "000000000000202");

    // An omitted key keeps the stored value...
    let (_, pet) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "color": "Chestnut" }))).await;
    assert_eq!(pet["microchip"], "000000000000202", "omitted key keeps");
    assert_eq!(pet["color"], "Chestnut");

    // ...and an explicit null clears it.
    let (_, pet) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "microchip": null }))).await;
    assert!(pet["microchip"].is_null(), "explicit null clears");
    assert_eq!(pet["color"], "Chestnut", "other fields untouched");
}

#[tokio::test]
async fn pet_edit_rejects_a_blank_name_or_species() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Rocco").await;
    for body in [json!({ "name": "   " }), json!({ "species": "" })] {
        let (status, _) = send(&app, "PATCH", &format!("/pets/{id}"), Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn pet_allergies_can_be_replaced_and_cleared() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Kipper").await;

    let (_, pet) = send(
        &app,
        "PATCH",
        &format!("/pets/{id}"),
        Some(json!({ "allergies": [{ "label": "Beef", "detail": null }] })),
    )
    .await;
    assert_eq!(pet["allergies"][0]["label"], "Beef");

    // An empty list clears the column rather than storing "[]".
    let (_, pet) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "allergies": [] }))).await;
    assert!(pet["allergies"].is_null());
}

#[tokio::test]
async fn one_product_can_cover_flea_tick_and_heartworm() {
    // The reason ongoing care is a list and not flat prevention columns:
    // Simparica Trio is all three protections in one monthly pill.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Pedro").await;

    let (status, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({
            "kind": "preventative", "product": "Simparica Trio",
            "amount": "1 pill", "frequency": "monthly",
            "protects": ["flea", "tick", "heartworm"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(reg["product"], "Simparica Trio");
    assert_eq!(reg["protects"].as_array().unwrap().len(), 3);
    assert!(reg["ended_on"].is_null(), "no end date means still on it");

    // It answers the intake form's "current on heartworm prevention? brand?".
    let (_, detail) = send(&app, "GET", &format!("/pets/{id}"), None).await;
    let current: Vec<&Value> = detail["regimens"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["ended_on"].is_null())
        .collect();
    let hw = current
        .iter()
        .find(|r| r["protects"].as_array().is_some_and(|p| p.iter().any(|x| x == "heartworm")))
        .expect("a current heartworm preventative");
    assert_eq!(hw["product"], "Simparica Trio");
}

#[tokio::test]
async fn food_and_medication_share_one_shape() {
    // The intake form asks for both identically: "(name, amount, frequency)".
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Lulu").await;

    for body in [
        json!({ "kind": "food", "product": "Hill's Science Diet Puppy", "amount": "1 cup", "frequency": "2x/day" }),
        json!({ "kind": "medication", "product": "Apoquel", "amount": "16mg", "frequency": "daily" }),
        json!({ "kind": "supplement", "product": "Cosequin", "amount": "1 chew", "frequency": "daily" }),
    ] {
        let (status, _) = send(&app, "POST", &format!("/pets/{id}/regimens"), Some(body)).await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let (status, list) = send(&app, "GET", &format!("/pets/{id}/regimens"), None).await;
    assert_eq!(status, StatusCode::OK);
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 3);
    // Food sorts first, then preventative, medication, supplement.
    assert_eq!(list[0]["kind"], "food");
    assert_eq!(list[1]["kind"], "medication");
    assert_eq!(list[2]["kind"], "supplement");
}

#[tokio::test]
async fn ended_care_sorts_below_current_care() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Rocco").await;
    // A food the pet is OFF, and a medication it is ON. Kind order would put
    // the food first; "still on it" must win.
    send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "Puppy Kibble", "ended_on": "2026-01-15" })),
    )
    .await;
    send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "medication", "product": "Apoquel" })),
    )
    .await;
    let (_, list) = send(&app, "GET", &format!("/pets/{id}/regimens"), None).await;
    let list = list.as_array().unwrap();
    assert_eq!(list[0]["product"], "Apoquel", "current care sorts first");
    assert_eq!(list[1]["product"], "Puppy Kibble");
}

#[tokio::test]
async fn regimen_validates_kind_and_protections() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Kipper").await;

    let (status, body) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "vitamins", "product": "X" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("supplement"), "names the valid kinds");

    let (status, _) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "preventative", "product": "X", "protects": ["mosquito"] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "   " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "product is required");
}

#[tokio::test]
async fn protections_are_normalised_and_deduped() {
    // Casing and duplicates must not defeat "is this pet covered for X".
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Pedro").await;
    let (_, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({
            "kind": "preventative", "product": "NexGard",
            "protects": ["Flea", " flea ", "TICK"],
        })),
    )
    .await;
    let p = reg["protects"].as_array().unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(p[0], "flea");
    assert_eq!(p[1], "tick");

    // An all-empty list stores NULL, not "[]".
    let (_, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "Kibble", "protects": [] })),
    )
    .await;
    assert!(reg["protects"].is_null());
}

#[tokio::test]
async fn regimen_edit_keeps_omitted_and_clears_explicit_null() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Lulu").await;
    let (_, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({
            "kind": "preventative", "product": "Simparica Trio",
            "frequency": "monthly", "ended_on": "2026-08-01",
            "protects": ["flea"],
        })),
    )
    .await;
    let rid = reg["id"].as_str().unwrap().to_string();

    // Omitted keys keep their values.
    let (status, reg) = send(&app, "PATCH", &format!("/regimens/{rid}"), Some(json!({ "amount": "1 pill" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reg["frequency"], "monthly");
    assert_eq!(reg["protects"][0], "flea");

    // Clearing `ended_on` puts the pet back ON it.
    let (_, reg) = send(&app, "PATCH", &format!("/regimens/{rid}"), Some(json!({ "ended_on": null }))).await;
    assert!(reg["ended_on"].is_null(), "back on the preventative");

    // A bad kind on edit is rejected, and an empty product is rejected.
    let (status, _) = send(&app, "PATCH", &format!("/regimens/{rid}"), Some(json!({ "kind": "snack" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(&app, "PATCH", &format!("/regimens/{rid}"), Some(json!({ "product": " " }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // GET by id, then delete.
    let (status, one) = send(&app, "GET", &format!("/regimens/{rid}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["product"], "Simparica Trio");
    let (status, _) = send(&app, "DELETE", &format!("/regimens/{rid}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&app, "GET", &format!("/regimens/{rid}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn regimen_endpoints_404_on_a_missing_pet_or_entry() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let (status, _) = send(&app, "GET", "/pets/nobody/regimens", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app,
        "POST",
        "/pets/nobody/regimens",
        Some(json!({ "kind": "food", "product": "Kibble" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, "PATCH", "/regimens/missing", Some(json!({ "product": "X" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, "DELETE", "/regimens/missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_pet_cascades_its_ongoing_care() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Pedro").await;
    let (_, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "Kibble" })),
    )
    .await;
    let rid = reg["id"].as_str().unwrap().to_string();
    let (status, _) = send(&app, "DELETE", &format!("/pets/{id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&app, "GET", &format!("/regimens/{rid}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "care rows cascade with the pet");
}

#[tokio::test]
async fn syncing_an_insurer_clears_a_stale_reimbursement() {
    // The InsuranceSheet "show on profile as insurer" path (#26/#29) sends
    // `insurer_reimburse: null` for a policy with no reimbursement %. Under the
    // old COALESCE semantics that KEPT the previous insurer's percentage, so a
    // pet could show "Lemonade · 90% reimbursement" after moving to a policy
    // that reimburses nothing. Null-vs-absent (#63) is what makes it correct.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Lulu").await;
    send(
        &app,
        "PATCH",
        &format!("/pets/{id}"),
        Some(json!({ "insurer": "Trupanion", "insurer_reimburse": "90%" })),
    )
    .await;

    let (_, pet) = send(
        &app,
        "PATCH",
        &format!("/pets/{id}"),
        Some(json!({ "insurer": "Lemonade", "insurer_reimburse": null })),
    )
    .await;
    assert_eq!(pet["insurer"], "Lemonade");
    assert!(pet["insurer_reimburse"].is_null(), "the old 90% must not survive");
}

// ---------- hardening found by an adversarial review of #63 ----------

#[tokio::test]
async fn protections_are_dropped_for_anything_but_a_preventative() {
    // A bag of kibble claiming to protect against heartworm would be reported
    // as the pet's CURRENT heartworm cover — the exact question a vet intake
    // form asks. The server enforces this, not just the form that hides the
    // checkboxes, because the API is the contract.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Pedro").await;

    let (status, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "Kibble", "protects": ["heartworm"] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(reg["protects"].is_null(), "food cannot carry protections");

    // And demoting a real preventative to a food strips the ones it had, even
    // though the PATCH never mentions `protects`.
    let (_, trio) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "preventative", "product": "Simparica Trio", "protects": ["flea", "heartworm"] })),
    )
    .await;
    let rid = trio["id"].as_str().unwrap().to_string();
    assert_eq!(trio["protects"].as_array().unwrap().len(), 2);
    let (_, now_food) = send(&app, "PATCH", &format!("/regimens/{rid}"), Some(json!({ "kind": "food" }))).await;
    assert!(now_food["protects"].is_null(), "demoting to food drops protections");
}

#[tokio::test]
async fn blank_dates_become_null_not_an_empty_string() {
    // `""` IS NOT NULL in SQL, so a blank end date sorted the entry as STOPPED
    // while the client's falsy test rendered it as current — the two halves
    // disagreeing about the same row.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Lulu").await;
    let (_, reg) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "Kibble", "ended_on": "", "started_on": "  " })),
    )
    .await;
    assert!(reg["ended_on"].is_null(), "blank end date is NULL, i.e. still current");
    assert!(reg["started_on"].is_null());

    let rid = reg["id"].as_str().unwrap().to_string();
    let (_, reg) = send(&app, "PATCH", &format!("/regimens/{rid}"), Some(json!({ "ended_on": "" }))).await;
    assert!(reg["ended_on"].is_null(), "and on the edit path too");

    // A blank pet field is the same as an absent one, never a stored "".
    let (_, pet) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "microchip": "  ", "birth_date": "" }))).await;
    assert!(pet["microchip"].is_null());
    assert!(pet["birth_date"].is_null());
}

#[tokio::test]
async fn impossible_calendar_dates_are_rejected() {
    // "2026-02-31" stored happily and was then rolled forward to March 3 by the
    // browser's Date, silently changing what the user typed.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Koa").await;
    for bad in ["2026-02-31", "not-a-date", "2026-13-01", "2026-00-10", "26-01-01", "2026-1-1"] {
        let (status, _) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "birth_date": bad }))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad} should be rejected");
        let (status, _) = send(
            &app,
            "POST",
            &format!("/pets/{id}/regimens"),
            Some(json!({ "kind": "food", "product": "K", "started_on": bad })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad} should be rejected");
    }
    // Leap years are real dates; 2024 has a Feb 29 and 2026 does not.
    let (status, _) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "birth_date": "2024-02-29" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "birth_date": "2026-02-29" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn absurdly_long_text_is_rejected_not_stored() {
    // The global body cap is 40 MiB, which one `product` string can fill on its
    // own and then bloat every pet-detail response forever.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Pedro").await;
    let huge = "x".repeat(5_000);
    let (status, body) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": huge })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("too long"));

    let (status, _) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "color": huge }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/pets/{id}"),
        Some(json!({ "allergies": [{ "label": huge, "detail": null }] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_bad_created_by_is_the_clients_fault_not_a_500() {
    // The foreign key rejects it; surfacing that as 500 tells the caller the
    // server broke when it was their payload.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Lulu").await;
    let (status, body) = send(
        &app,
        "POST",
        &format!("/pets/{id}/regimens"),
        Some(json!({ "kind": "food", "product": "Kibble", "created_by": "nobody" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("constraint"));
}

#[tokio::test]
async fn an_edit_only_writes_the_fields_it_names() {
    // Regression guard for the read-modify-write window: update_pet merges in
    // SQL, so a PATCH naming one field must leave every other column exactly as
    // the row has it — never as some earlier snapshot had it.
    let app = api_router(AppState::for_test(memory_pool().await));
    let id = a_pet(&app, "Pedro").await;
    send(
        &app,
        "PATCH",
        &format!("/pets/{id}"),
        Some(json!({ "breed": "Australian Shepherd", "color": "Blue Merle", "microchip": "000000000000202" })),
    )
    .await;
    let (_, pet) = send(&app, "PATCH", &format!("/pets/{id}"), Some(json!({ "sex": "male_neutered" }))).await;
    assert_eq!(pet["breed"], "Australian Shepherd");
    assert_eq!(pet["color"], "Blue Merle");
    assert_eq!(pet["microchip"], "000000000000202");
    assert_eq!(pet["sex"], "male_neutered");
    assert_eq!(pet["name"], "Pedro", "name is untouched by an unrelated edit");
}

// ---------- pet sex as a closed set (#72) ----------

#[tokio::test]
async fn sex_is_one_of_five_values() {
    let app = api_router(AppState::for_test(memory_pool().await));
    // Create: free text is refused, the set is accepted, blank is "not recorded".
    let (s, body) = send(&app, "POST", "/pets", Some(json!({ "name": "Koa", "species": "Horse", "sex": "Gelding" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("male_neutered"), "the error names the set");
    let (s, koa) = send(&app, "POST", "/pets", Some(json!({ "name": "Koa", "species": "Horse", "sex": "male_neutered" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(koa["sex"], "male_neutered");
    let (_, lulu) = send(&app, "POST", "/pets", Some(json!({ "name": "Lulu", "species": "Dog", "sex": "" }))).await;
    assert!(lulu["sex"].is_null());

    // Update: same rules; null clears.
    for v in ["male", "female", "male_neutered", "female_spayed", "unknown"] {
        let (s, p) = send(&app, "PATCH", "/pets/lulu", Some(json!({ "sex": v }))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(p["sex"], v);
    }
    for bad in ["Female", "spayed", "FEMALE", "intact female"] {
        let (s, _) = send(&app, "PATCH", "/pets/lulu", Some(json!({ "sex": bad }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (_, p) = send(&app, "PATCH", "/pets/lulu", Some(json!({ "sex": null }))).await;
    assert!(p["sex"].is_null());
}

#[tokio::test]
async fn a_spay_record_suggests_updating_an_intact_sex() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(&app, "POST", "/pets", Some(json!({ "name": "Lulu", "species": "Dog", "sex": "female" }))).await;
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert!(lulu["sex_suggestion"].is_null(), "no record, no suggestion");

    // A consult or estimate is not the procedure.
    send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-08-01", "title": "Spay consult" }))).await;
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert!(lulu["sex_suggestion"].is_null());

    // Lulu's case: a "Spay" record while her sex still says female.
    let (_, rec) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-09-21", "title": "Spay" }))).await;
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    let sug = &lulu["sex_suggestion"];
    assert_eq!(sug["sex"], "female_spayed");
    assert_eq!(sug["record_id"], rec["id"]);
    assert_eq!(sug["record_title"], "Spay");
    assert_eq!(sug["occurred_on"], "2026-09-21");

    // Accepting it (an ordinary PATCH) makes it vanish: derived, never stored.
    send(&app, "PATCH", "/pets/lulu", Some(json!({ "sex": "female_spayed" }))).await;
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert!(lulu["sex_suggestion"].is_null());

    // A billed line item counts too, and the procedure must fit the sex.
    send(&app, "POST", "/pets", Some(json!({ "name": "Pedro", "species": "Dog", "sex": "male" }))).await;
    send(&app, "POST", "/pets/pedro/records", Some(json!({
        "occurred_on": "2025-01-10", "title": "Surgery",
        "line_items": [ { "description": "Anesthesia", "amount_cents": 20000 }, { "description": "Canine neuter 40-60 lb", "amount_cents": 30000 } ]
    }))).await;
    let (_, pedro) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(pedro["sex_suggestion"]["sex"], "male_neutered");
    send(&app, "PATCH", "/pets/pedro", Some(json!({ "sex": "female" }))).await;
    send(&app, "POST", "/pets/pedro/records", Some(json!({ "occurred_on": "2025-02-10", "title": "Castration" }))).await;
    let (_, pedro) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(pedro["sex_suggestion"]["sex"], "female_spayed", "the castration doesn't fit; the older neuter still does");
    assert_eq!(pedro["sex_suggestion"]["record_title"], "Surgery");

    // An archived pet is not prompted.
    send(&app, "POST", "/pets/pedro/archive", Some(json!({ "kind": "rehomed" }))).await;
    let (_, pedro) = send(&app, "GET", "/pets/pedro", None).await;
    assert!(pedro["sex_suggestion"].is_null());
}

#[tokio::test]
async fn breeder_terms_is_a_household_setting() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let (s, d) = send(&app, "GET", "/settings/display", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["breeder_terms"], false, "off by default");
    let (_, d) = send(&app, "PUT", "/settings/display", Some(json!({ "breeder_terms": true }))).await;
    assert_eq!(d["breeder_terms"], true);
    let (_, d) = send(&app, "PUT", "/settings/display", Some(json!({}))).await;
    assert_eq!(d["breeder_terms"], true, "an omitted field is left alone");
    let (_, d) = send(&app, "GET", "/settings/display", None).await;
    assert_eq!(d["breeder_terms"], true);
    let (_, d) = send(&app, "PUT", "/settings/display", Some(json!({ "breeder_terms": false }))).await;
    assert_eq!(d["breeder_terms"], false);
}

/// Migration 0016 maps the free text already stored with SQL; bulk import maps
/// a document's wording with `sex::sex_from_text`. They must agree, so run the
/// real migration over a database at 0015 and compare, string by string.
#[tokio::test]
async fn sex_migration_matches_the_rust_mapping() {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    let migrator = sqlx::migrate!("./migrations");
    let (before, m16): (Vec<_>, Vec<_>) = migrator.iter().partition(|m| m.version < 16);
    assert_eq!(m16[0].version, 16, "0016 is the sex migration");
    for m in before {
        sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
    }
    // Words `sex_from_text` learned after 0016 shipped. The migration is frozen
    // (sqlx checksums it), so a new Rust word is listed here, never added there.
    const ADDED_AFTER_0016: &[&str] = &[];
    // Every word the Rust mapping reacts to, alone and as "<word> female" /
    // "male <word>" (so a word-list drift shows up even where rule order would
    // hide it), so a word added to Rust and not to the SQL fails here.
    let vocab: Vec<String> = furfiles::sex::vocabulary()
        .into_iter()
        .filter(|w| !ADDED_AFTER_0016.contains(w))
        .flat_map(|w| [w.to_string(), format!("{w} female"), format!("Male {w}"), w.to_uppercase()])
        .collect();
    assert!(vocab.len() > 150, "the vocabulary loaded ({} samples)", vocab.len());
    let fixed = [
        // A household's legacy values (Kipper, Pedro, Lulu, Koa, Rocco).
        "Male · neutered", "Neutered male", "Female", "Gelding",
        // Everything else the rules cover, plus things they must not mangle.
        "Female · spayed", "Female · intact", "Spayed female", "Neutered female", "Spayed mare",
        "MN", "M/N", "m / n", "FS", "F/S", "SF", "Male Castrated", "castrated", "Spayed", "Neutered",
        "ovariohysterectomy", "F", "M", "Mare", "Queen", "Bitch", "Filly", "Stallion", "Tom", "Colt",
        "Stud", "Intact male", "Intact female", "MALE", "female_spayed", "male_neutered", "unknown",
        "Kitten", "?", "n/a", "Male (intact)", "Female, spayed 2026", "  Male  ", "Tomcat",
        "Female — spayed", "Male - Neutered", "Male\tneutered",
        // Copied text: non-breaking, thin and narrow no-break spaces.
        "Female\u{a0}neutered", "Male\u{2009}neutered", "F\u{202f}/\u{202f}S",
        // An intact word wins (Codex review): never invert an explicit status.
        "Female, not spayed", "Intact female, spay planned", "Altered male", "Desexed",
    ];
    let samples: Vec<String> = fixed.iter().map(|s| s.to_string()).chain(vocab).collect();
    for (i, t) in samples.iter().enumerate() {
        sqlx::query("INSERT INTO pets (id, name, species, sex, created_at, updated_at) VALUES (?, ?, 'Dog', ?, '', '')")
            .bind(format!("p{i}")).bind(format!("P{i}")).bind(t)
            .execute(&pool).await.unwrap();
    }
    for (id, t) in [("blank", "  "), ("none", "")] {
        sqlx::query("INSERT INTO pets (id, name, species, sex, created_at, updated_at) VALUES (?, ?, 'Dog', ?, '', '')")
            .bind(id).bind(id).bind(t).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO pets (id, name, species, created_at, updated_at) VALUES ('null', 'Null', 'Cat', '', '')")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(&m16[0].sql).execute(&pool).await.unwrap();

    for (i, t) in samples.iter().enumerate() {
        let (sex, original): (Option<String>, String) = sqlx::query_as(
            "SELECT p.sex, l.original FROM pets p JOIN pet_sex_legacy l ON l.pet_id = p.id WHERE p.id = ?",
        )
        .bind(format!("p{i}"))
        .fetch_one(&pool).await.unwrap();
        assert_eq!(sex.as_deref(), furfiles::sex::sex_from_text(t), "{t:?}");
        assert_eq!(original, *t, "the original text is kept");
    }
    // That household lands where it should.
    let sexes: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, sex FROM pets WHERE id IN ('p0','p1','p2','p3') ORDER BY id")
            .fetch_all(&pool).await.unwrap();
    let sexes: Vec<_> = sexes.into_iter().map(|(_, s)| s.unwrap()).collect();
    assert_eq!(sexes, ["male_neutered", "male_neutered", "female", "male_neutered"]);
    // NULL stays NULL (not recorded); blank becomes NULL; neither gets a log row.
    let nulls: i64 = sqlx::query_scalar("SELECT count(*) FROM pets WHERE id IN ('blank','none','null') AND sex IS NULL")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(nulls, 3);
    let logged: i64 = sqlx::query_scalar("SELECT count(*) FROM pet_sex_legacy").fetch_one(&pool).await.unwrap();
    assert_eq!(logged as usize, samples.len());
    // Every stored value is now in the closed set.
    let stray: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pets WHERE sex IS NOT NULL AND sex NOT IN ('male','female','male_neutered','female_spayed','unknown')",
    )
    .fetch_one(&pool).await.unwrap();
    assert_eq!(stray, 0);
}

// ---------- validation reachable by any client (#22) ----------

#[tokio::test]
async fn record_category_must_be_a_known_one() {
    let pool = memory_pool().await;
    furfiles::seed::ensure_owner(&pool).await.unwrap();
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));
    let (s, body) = send(
        &app,
        "POST",
        "/pets/biscuit/records",
        Some(json!({"occurred_on": "2026-01-02", "title": "Shots", "category": "Vaccination"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("unknown category 'Vaccination'"));
    let (s, rec) = send(
        &app,
        "POST",
        "/pets/biscuit/records",
        Some(json!({"occurred_on": "2026-01-02", "title": "Shots", "category": "Prescriptions"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let uri = format!("/pets/biscuit/records/{}", rec["id"]);
    let (s, _) = send(&app, "PATCH", &uri, Some(json!({"category": "Vet"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, up) = send(&app, "PATCH", &uri, Some(json!({"category": "Grooming"}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["category"], "Grooming");
}

#[tokio::test]
async fn reminder_dates_are_real_and_titles_not_blank() {
    let pool = memory_pool().await;
    furfiles::seed::ensure_owner(&pool).await.unwrap();
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    let app = api_router(AppState::for_test(pool));
    for due in ["2026-02-30", "soon", "  "] {
        let (s, _) = send(
            &app,
            "POST",
            "/pets/biscuit/reminders",
            Some(json!({"due_on": due, "title": "Booster"})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{due:?}");
    }
    let (s, r) = send(
        &app,
        "POST",
        "/pets/biscuit/reminders",
        Some(json!({"due_on": " 2026-03-01 ", "title": "Booster"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(r["due_on"], "2026-03-01");
    let uri = format!("/reminders/{}", r["id"]);
    for bad in [json!({"due_on": "2026-04-31"}), json!({"due_on": ""}), json!({"title": "   "})] {
        let (s, _) = send(&app, "PATCH", &uri, Some(bad.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (s, up) = send(&app, "PATCH", &uri, Some(json!({"title": " Rabies booster ", "done": true}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(up["title"], "Rabies booster");
    assert_eq!(up["done"], true);
    assert_eq!(up["due_on"], "2026-03-01");
}
