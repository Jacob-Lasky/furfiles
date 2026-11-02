//! Settings model picker (#67): `GET/POST /settings/ingestion/models` against a
//! MOCK Anthropic `GET /v1/models`, so pagination, the key-rejected path, error
//! sanitising and the cache all run through the real handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

mod common;
use common::send;

use furfiles::ingestion::{foreign_key_provider, parse_models_page, sort_models, ProviderModel};
use furfiles::{api_router, memory_pool, AppState};

const GOOD: &str = "sk-ant-good";
const OTHER_GOOD: &str = "sk-ant-other";
/// Answers 500 with an error body, to prove a provider failure isn't cached.
const FLAKY: &str = "sk-ant-flaky";
/// Paging that never finishes: a repeated cursor, or a fresh one forever.
const LOOPING: &str = "sk-ant-looping";
const ENDLESS: &str = "sk-ant-endless";

/// One `GET /v1/models` the mock saw: (x-api-key, anthropic-version, after_id).
type Seen = Arc<Mutex<Vec<(String, String, Option<String>)>>>;

/// A fake Models API serving five models in pages of two (deliberately NOT
/// newest first), keyed by `after_id`. Any key but the good ones gets a 401
/// whose message echoes the key, the way a careless provider might.
async fn mock_models() -> (String, Seen) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let app = Router::new().route(
        "/v1/models",
        get(move |headers: HeaderMap, Query(q): Query<HashMap<String, String>>| {
            let seen = seen2.clone();
            async move {
                let key = headers.get("x-api-key").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                let version = headers
                    .get("anthropic-version")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let after = q.get("after_id").cloned();
                seen.lock().unwrap().push((key.clone(), version, after.clone()));
                if key == LOOPING || key == ENDLESS {
                    // LOOPING repeats one cursor; ENDLESS always has another page.
                    let last = if key == LOOPING {
                        "claude-loop".to_string()
                    } else {
                        format!("{}x", after.clone().unwrap_or_default())
                    };
                    return Json(json!({
                        "data": [{"id": last, "display_name": "Loop", "created_at": "2026-01-01T00:00:00Z"}],
                        "has_more": true,
                        "last_id": last,
                    }))
                    .into_response();
                }
                if key == FLAKY {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"type":"error","error":{"type":"api_error","message":"internal detail with sk-ant-flaky"}})),
                    )
                        .into_response();
                }
                if key != GOOD && key != OTHER_GOOD {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"type":"error","error":{"type":"authentication_error","message":format!("invalid x-api-key {key}")}})),
                    )
                        .into_response();
                }
                let all = [
                    ("claude-old", "Claude Old", "2025-01-01T00:00:00Z"),
                    ("claude-newest", "Claude Newest", "2026-10-07T18:00:00Z"),
                    ("claude-sonnet-5", "Claude Sonnet 5", "2026-06-29T00:00:00Z"),
                    ("claude-mid", "", "2026-02-17T00:00:00Z"),
                    ("claude-opus-5", "Claude Opus 5", "2026-07-24T00:00:00Z"),
                ];
                let start = match &after {
                    Some(a) => all.iter().position(|m| m.0 == a).map(|i| i + 1).unwrap_or(all.len()),
                    None => 0,
                };
                let page: Vec<_> = all.iter().skip(start).take(2).collect();
                let has_more = start + page.len() < all.len();
                Json(json!({
                    "data": page.iter().map(|(id, name, at)| json!({"type":"model","id":id,"display_name":name,"created_at":at})).collect::<Vec<_>>(),
                    "has_more": has_more,
                    "first_id": page.first().map(|m| m.0),
                    "last_id": page.last().map(|m| m.0),
                }))
                .into_response()
            }
        }),
    );
    (common::serve(app).await, seen)
}

async fn app_with_mock() -> (Router, Seen) {
    let (base, seen) = mock_models().await;
    let mut st = AppState::for_test(memory_pool().await);
    st.anthropic_base = base;
    (api_router(st), seen)
}

fn ids(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect()
}

const NEWEST_FIRST: [&str; 5] = [
    "claude-newest",
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-mid",
    "claude-old",
];

#[tokio::test]
async fn stored_key_lists_every_page_newest_first_without_leaking_the_key() {
    let (app, seen) = app_with_mock().await;

    // No key saved → a 400 the picker shows, and no provider call.
    let (s, v) = send(&app, "GET", "/settings/ingestion/models", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("No API key"));
    assert!(seen.lock().unwrap().is_empty());

    // Saved but scanning still OFF: choosing a model is part of setting up, so
    // the list must load anyway.
    let (s, _) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key": GOOD})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = send(&app, "GET", "/settings/ingestion/models", None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(ids(&v), NEWEST_FIRST);
    // display_name falls back to the id when the provider leaves it blank.
    assert_eq!(v[3]["display_name"], "claude-mid");
    assert_eq!(v[0]["display_name"], "Claude Newest");
    assert_eq!(v[0]["created_at"], "2026-10-07T18:00:00Z");
    assert!(
        !v.to_string().contains(GOOD),
        "the key must never come back"
    );

    // Three pages followed by cursor, each with the decrypted key + API version.
    let calls = seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 3);
    assert!(calls
        .iter()
        .all(|(k, ver, _)| k == GOOD && ver == "2023-06-01"));
    assert_eq!(calls[0].2, None);
    assert_eq!(calls[1].2.as_deref(), Some("claude-newest"));
    assert_eq!(calls[2].2.as_deref(), Some("claude-mid"));
}

#[tokio::test]
async fn model_list_is_cached_per_key_and_failures_are_not() {
    let (app, seen) = app_with_mock().await;
    send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key": GOOD})),
    )
    .await;

    let (s, first) = send(&app, "GET", "/settings/ingestion/models", None).await;
    assert_eq!(s, StatusCode::OK);
    let after_first = seen.lock().unwrap().len();
    // Same key again, by either entry point → served from the cache.
    let (_, again) = send(&app, "GET", "/settings/ingestion/models", None).await;
    let (_, posted) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": GOOD})),
    )
    .await;
    assert_eq!(first, again);
    assert_eq!(first, posted);
    assert_eq!(
        seen.lock().unwrap().len(),
        after_first,
        "a cached list must not call the provider"
    );

    // A different key is a different cache entry.
    let (s, _) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": OTHER_GOOD})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(seen.lock().unwrap().len() > after_first);

    // A provider failure is never cached: the retry goes back to the provider.
    let before = seen.lock().unwrap().len();
    let (s1, _) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": FLAKY})),
    )
    .await;
    let (s2, _) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": FLAKY})),
    )
    .await;
    assert_eq!((s1, s2), (StatusCode::BAD_GATEWAY, StatusCode::BAD_GATEWAY));
    assert_eq!(seen.lock().unwrap().len(), before + 2);
}

#[tokio::test]
async fn candidate_key_errors_are_clear_and_never_echo_the_key() {
    let (app, seen) = app_with_mock().await;

    // A rejected key → 400 with a plain explanation, and the provider's
    // message (which echoed the key) is NOT relayed.
    let (s, v) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": "sk-ant-wrong"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let msg = v["error"].as_str().unwrap();
    assert!(msg.contains("rejected this API key"), "{msg}");
    assert!(!v.to_string().contains("sk-ant-wrong"));

    // Any other provider failure → 502 with status + error TYPE only.
    let (s, v) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": FLAKY})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert_eq!(v["error"], "The model provider returned 500 (api_error).");

    // Empty key → 400 without calling anyone.
    let before = seen.lock().unwrap().len();
    let (s, _) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": "  "})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Another vendor's key is refused BEFORE it is sent to Anthropic.
    for (key, vendor) in [
        ("sk-proj-abc123", "OpenAI"),
        ("sk-abc123", "OpenAI"),
        ("AIzaSyabc", "Google Gemini"),
    ] {
        let (s, v) = send(
            &app,
            "POST",
            "/settings/ingestion/models",
            Some(json!({"api_key": key})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let msg = v["error"].as_str().unwrap();
        assert!(msg.contains(vendor) && msg.contains("sk-ant-"), "{msg}");
        assert!(!msg.contains(key));
    }
    assert_eq!(
        seen.lock().unwrap().len(),
        before,
        "no foreign or empty key reached the provider"
    );
}

#[tokio::test]
async fn saving_another_vendors_key_is_refused_and_keeps_the_stored_one() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key": GOOD})),
    )
    .await;
    let (s, v) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({"api_key": "sk-proj-abc", "model": "claude-opus-5"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("OpenAI"));
    // Nothing from the refused request is written: not the key, and not the
    // model that came with it.
    let (_, v) = send(&app, "GET", "/settings/ingestion", None).await;
    assert_eq!(v["has_key"], true);
    assert_eq!(v["model"], "claude-sonnet-5");
}

#[tokio::test]
async fn a_foreign_key_saved_before_the_check_is_never_sent_to_anthropic() {
    // Versions before #67 stored any key; plant an OpenAI key the old way.
    let (base, seen) = mock_models().await;
    let pool = memory_pool().await;
    let mut st = AppState::for_test(pool.clone());
    st.anthropic_base = base;
    let blob = furfiles::crypto::encrypt(&st.key, "sk-proj-legacy");
    let app = api_router(st);
    let mut conn = pool.acquire().await.unwrap();
    furfiles::settings::set_api_key_enc(&mut conn, &blob)
        .await
        .unwrap();
    furfiles::settings::set_enabled(&mut conn, true)
        .await
        .unwrap();
    drop(conn);

    let (s, v) = send(&app, "GET", "/settings/ingestion/models", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("OpenAI"), "{v}");
    // Every other model call goes through the same decrypt, so extraction is
    // refused the same way.
    let (s, v) = send(
        &app,
        "POST",
        "/extract",
        Some(json!({"media_type": "application/pdf", "data_base64": "aGVsbG8="})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("OpenAI"), "{v}");
    assert!(!v.to_string().contains("sk-proj-legacy"));
    assert!(
        seen.lock().unwrap().is_empty(),
        "the foreign key reached the provider"
    );
}

#[tokio::test]
async fn paging_that_never_finishes_is_an_error_not_a_partial_list() {
    let (app, seen) = app_with_mock().await;
    let (s, v) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": LOOPING})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert_eq!(
        v["error"],
        "The provider's model list didn't finish loading."
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "stops as soon as the cursor repeats"
    );

    let (s, v) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": ENDLESS})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert_eq!(
        v["error"],
        "The provider's model list didn't finish loading."
    );
    assert_eq!(seen.lock().unwrap().len(), 2 + 10, "the page cap holds");
}

#[tokio::test]
async fn the_key_route_takes_a_small_body() {
    let app = api_router(AppState::for_test(memory_pool().await));
    let huge = "k".repeat(8 * 1024);
    let (s, _) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": huge})),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn unreachable_provider_is_a_502_naming_the_failure() {
    // for_test points at a dead port.
    let app = api_router(AppState::for_test(memory_pool().await));
    let (s, v) = send(
        &app,
        "POST",
        "/settings/ingestion/models",
        Some(json!({"api_key": GOOD})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    let msg = v["error"].as_str().unwrap();
    assert!(
        msg.starts_with("Couldn't reach the model provider"),
        "{msg}"
    );
    assert!(!msg.contains(GOOD));
}

#[test]
fn foreign_key_prefixes() {
    assert_eq!(foreign_key_provider("sk-ant-api03-xyz"), None);
    assert_eq!(foreign_key_provider("  sk-ant-x  "), None);
    assert_eq!(foreign_key_provider("sk-proj-xyz"), Some("OpenAI"));
    assert_eq!(foreign_key_provider("sk-xyz"), Some("OpenAI"));
    assert_eq!(foreign_key_provider("AIzaSyXYZ"), Some("Google Gemini"));
    // Unknown formats are left to the provider to judge.
    assert_eq!(foreign_key_provider("something-else"), None);
}

#[test]
fn models_page_parsing_and_ordering() {
    let page = json!({
        "data": [
            {"id": "a", "display_name": "A", "created_at": "2026-01-01T00:00:00Z"},
            {"id": "  ", "display_name": "blank id is skipped"},
            {"display_name": "no id is skipped"},
            {"id": "b"},
        ],
        "has_more": true,
        "last_id": "b"
    });
    let (models, next) = parse_models_page(&page).unwrap();
    assert_eq!(next.as_deref(), Some("b"));
    assert_eq!(
        models,
        vec![
            ProviderModel {
                id: "a".into(),
                display_name: "A".into(),
                created_at: Some("2026-01-01T00:00:00Z".into())
            },
            ProviderModel {
                id: "b".into(),
                display_name: "b".into(),
                created_at: None
            },
        ]
    );
    // "More to come" with no cursor is an error, not the end of the list.
    assert!(parse_models_page(&json!({"data": [], "has_more": true})).is_err());
    assert!(parse_models_page(&json!({"data": [], "has_more": true, "last_id": ""})).is_err());
    // Last page: no cursor even if last_id is present.
    let (_, next) =
        parse_models_page(&json!({"data": [], "has_more": false, "last_id": "z"})).unwrap();
    assert_eq!(next, None);
    assert!(parse_models_page(&json!({"error": {}})).is_err());

    // Newest first, undated last, duplicates collapsed.
    let m = |id: &str, at: Option<&str>| ProviderModel {
        id: id.into(),
        display_name: id.into(),
        created_at: at.map(Into::into),
    };
    let sorted = sort_models(vec![
        m("undated", None),
        m("old", Some("2025-01-01T00:00:00Z")),
        m("new", Some("2026-01-01T00:00:00Z")),
        m("old", Some("2025-01-01T00:00:00Z")),
    ]);
    let order: Vec<_> = sorted.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(order, ["new", "old", "undated"]);

    // Compared as instants, not text: as strings, "…00Z" sorts after
    // "…00.100Z", and "+02:00" puts an EARLIER instant after a later one.
    let sorted = sort_models(vec![
        m("whole-second", Some("2026-01-01T00:00:00Z")),
        m("tenth-later", Some("2026-01-01T00:00:00.100Z")),
        m("offset-earlier", Some("2026-01-01T01:00:00+02:00")), // 23:00Z the day before
        m("garbled", Some("not a date")),
    ]);
    let order: Vec<_> = sorted.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(
        order,
        ["tenth-later", "whole-second", "offset-earlier", "garbled"]
    );
}

#[test]
fn settings_update_debug_redacts_the_key() {
    let up: furfiles::models::IngestionSettingsUpdate =
        serde_json::from_value(json!({"api_key": "sk-ant-secret", "model": "m"})).unwrap();
    let dbg = format!("{up:?}");
    assert!(!dbg.contains("sk-ant-secret"), "{dbg}");
    assert!(dbg.contains("<redacted>"));
}
