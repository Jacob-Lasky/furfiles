//! Family portrait (#75): `GET /household` + `PUT/GET/DELETE /household/portrait`,
//! driven through the real router.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::send;

use furfiles::{api_router, memory_pool, AppState};

// The smallest byte strings that carry each format's file signature (the
// server checks the signature, not the pixels). Literal base64 on purpose:
// these are the bytes the assertions compare against.
const JPEG: &str = "/9j/4AAQ"; // FF D8 FF E0 00 10
const PNG: &str = "iVBORw0KGgo="; // 89 'PNG' 0D 0A 1A 0A
const WEBP: &str = "UklGRgAAAABXRUJQ"; // 'RIFF' .... 'WEBP'
const GIF: &str = "R0lGODlh"; // 'GIF89a'

/// A household with three pets (ids pedro, lulu, kipper), plus the router and
/// the attachments dir its files land in.
async fn household() -> (axum::Router, std::path::PathBuf) {
    let st = AppState::for_test(memory_pool().await);
    let dir = st.attachments_dir.clone();
    let app = api_router(st);
    for (name, species) in [("Pedro", "Dog"), ("Lulu", "Dog"), ("Kipper", "Cat")] {
        let (s, _) = send(
            &app,
            "POST",
            "/pets",
            Some(json!({"name": name, "species": species})),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    (app, dir)
}

fn portrait_files(dir: &std::path::Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir.join("household"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

async fn get_bytes(app: &axum::Router, uri: &str) -> (StatusCode, String, Vec<u8>) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let ctype = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, ctype, bytes)
}

fn generated(ids: &[&str]) -> Value {
    json!({"media_type":"image/jpeg","data_base64": JPEG,"source":"generated","pet_ids": ids})
}

#[tokio::test]
async fn no_portrait_until_one_is_set() {
    let (app, _) = household().await;
    let (s, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h["portrait"].is_null());
    let (s, _) = send(&app, "GET", "/household/portrait", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Deleting nothing is still fine.
    let (s, h) = send(&app, "DELETE", "/household/portrait", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h["portrait"].is_null());
}

#[tokio::test]
async fn uploaded_portrait_round_trips_and_ignores_the_roster() {
    let (app, dir) = household().await;
    let (s, h) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(json!({"media_type":"image/png","data_base64": PNG})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let p = &h["portrait"];
    assert_eq!(p["source"], "uploaded");
    assert_eq!(p["pet_ids"], json!([]));
    assert_eq!(p["roster_changed"], false);
    assert!(p["updated_at"].as_str().is_some());
    // The stored path never leaves the server.
    assert!(p.get("path").is_none());
    let files = portrait_files(&dir);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], format!("{}.png", p["version"].as_str().unwrap()));

    let (s, ctype, bytes) = get_bytes(&app, "/household/portrait").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ctype, "image/png");
    assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");

    // An uploaded photo isn't tied to the roster: adding a pet changes nothing.
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Comet","species":"Fish"})),
    )
    .await;
    let (_, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(h["portrait"]["roster_changed"], false);
}

#[tokio::test]
async fn generated_portrait_tracks_the_active_roster() {
    let (app, _) = household().await;
    // Ids are stored sorted and de-duplicated, whatever order the client sent.
    let (s, h) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(generated(&["pedro", "kipper", "lulu", "pedro"])),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{h}");
    assert_eq!(h["portrait"]["source"], "generated");
    assert_eq!(h["portrait"]["pet_ids"], json!(["kipper", "lulu", "pedro"]));
    assert_eq!(h["portrait"]["roster_changed"], false);

    // Archiving a pet moves the roster on...
    let (s, _) = send(
        &app,
        "POST",
        "/pets/lulu/archive",
        Some(json!({"kind":"rehomed","archived_at":"2026-10-01"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(h["portrait"]["roster_changed"], true);
    // ...and bringing them back puts it in step again.
    send(&app, "POST", "/pets/lulu/unarchive", None).await;
    let (_, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(h["portrait"]["roster_changed"], false);

    // A new pet, or deleting one, also counts.
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Comet","species":"Fish"})),
    )
    .await;
    let (_, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(h["portrait"]["roster_changed"], true);
    send(&app, "DELETE", "/pets/comet", None).await;
    let (_, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(h["portrait"]["roster_changed"], false);
    send(&app, "DELETE", "/pets/kipper", None).await;
    let (_, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(h["portrait"]["roster_changed"], true);
}

#[tokio::test]
async fn replacing_writes_the_new_file_before_removing_the_old() {
    let (app, dir) = household().await;
    let (_, first) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(generated(&["pedro", "lulu", "kipper"])),
    )
    .await;
    let (s, second) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(json!({"media_type":"image/webp","data_base64": WEBP})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let v1 = first["portrait"]["version"].as_str().unwrap();
    let v2 = second["portrait"]["version"].as_str().unwrap();
    assert_ne!(v1, v2, "version changes so the client cache-busts");
    assert_eq!(second["portrait"]["source"], "uploaded");
    assert_eq!(portrait_files(&dir), vec![format!("{v2}.webp")]);
    let (_, ctype, bytes) = get_bytes(&app, "/household/portrait").await;
    assert_eq!(
        (ctype.as_str(), bytes.as_slice()),
        ("image/webp", &b"RIFF\0\0\0\0WEBP"[..])
    );

    // Delete forgets it and unlinks the file.
    let (s, h) = send(&app, "DELETE", "/household/portrait", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h["portrait"].is_null());
    assert!(portrait_files(&dir).is_empty());
    let (s, _) = send(&app, "GET", "/household/portrait", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bad_portraits_are_rejected_without_touching_the_current_one() {
    let (app, dir) = household().await;
    send(
        &app,
        "PUT",
        "/household/portrait",
        Some(generated(&["pedro"])),
    )
    .await;
    let before = portrait_files(&dir);
    send(
        &app,
        "POST",
        "/pets/kipper/archive",
        Some(json!({"kind":"passed","archived_at":"2026-09-01"})),
    )
    .await;

    let cases = [
        // not an image
        json!({"media_type":"application/pdf","data_base64":"aGVsbG8="}),
        // a non-image, or a real image under the wrong type
        json!({"media_type":"image/jpeg","data_base64":"aGVsbG8="}),
        json!({"media_type":"image/png","data_base64": JPEG}),
        json!({"media_type":"image/webp","data_base64":"UklGRgAAAABBVkkg"}), // RIFF/AVI
        // not base64 / empty
        json!({"media_type":"image/jpeg","data_base64":"not base64 !!"}),
        json!({"media_type":"image/jpeg","data_base64":""}),
        // unknown source
        json!({"media_type":"image/jpeg","data_base64": JPEG,"source":"ai"}),
        // uploaded photos don't carry pet ids
        json!({"media_type":"image/jpeg","data_base64": JPEG,"pet_ids":["pedro"]}),
        // generated needs ids, and only ACTIVE pets
        json!({"media_type":"image/jpeg","data_base64": JPEG,"source":"generated"}),
        generated(&["pedro", "ghost"]),
        generated(&["pedro", "kipper"]),
    ];
    for body in cases {
        let (s, e) = send(&app, "PUT", "/household/portrait", Some(body.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body} → {e}");
        assert!(e["error"].as_str().is_some());
    }
    assert_eq!(portrait_files(&dir), before);

    // Over the 10 MB photo cap → 413.
    let big = "A".repeat(14 * 1024 * 1024);
    let (s, _) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(json!({"media_type":"image/jpeg","data_base64": big})),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(portrait_files(&dir), before);
}

#[tokio::test]
async fn a_missing_file_is_a_404_not_a_500() {
    let (app, dir) = household().await;
    let (_, h) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(json!({"media_type":"image/gif","data_base64": GIF})),
    )
    .await;
    let v = h["portrait"]["version"].as_str().unwrap();
    std::fs::remove_file(dir.join(format!("household/{v}.gif"))).unwrap();
    let (s, _) = send(&app, "GET", "/household/portrait", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // ...and the metadata says so too, so Home shows the invitation rather
    // than a broken image.
    let (s, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h["portrait"].is_null());
}

#[tokio::test]
async fn an_unreadable_stored_value_reads_as_no_portrait() {
    let pool = memory_pool().await;
    let app = api_router(AppState::for_test(pool.clone()));
    sqlx::query("INSERT INTO app_settings (key, value) VALUES ('household.portrait', 'not json')")
        .execute(&pool)
        .await
        .unwrap();
    let (s, h) = send(&app, "GET", "/household", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h["portrait"].is_null());
    // DELETE clears it...
    let (s, _) = send(&app, "DELETE", "/household/portrait", None).await;
    assert_eq!(s, StatusCode::OK);
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM app_settings WHERE key = 'household.portrait'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
    // ...and a PUT simply overwrites it.
    let (s, h) = send(
        &app,
        "PUT",
        "/household/portrait",
        Some(json!({"media_type":"image/jpeg","data_base64": JPEG})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["portrait"]["source"], "uploaded");
}

#[tokio::test]
async fn a_stored_path_outside_household_is_never_served_or_unlinked() {
    let pool = memory_pool().await;
    let st = AppState::for_test(pool.clone());
    let dir = st.attachments_dir.clone();
    let app = api_router(st);
    // Something precious next to the portraits: a hand-edited row must not
    // reach it through GET or DELETE.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("secret.key"), b"top secret").unwrap();
    for value in [
        r#"{"path":"../secret.key","source":"uploaded"}"#,
        r#"{"path":"household/../secret.key","source":"uploaded"}"#,
        r#"{"path":"secret.key","source":"uploaded"}"#,
        r#"{"path":"household/0f0f.exe","source":"uploaded"}"#,
        // a valid path with a source outside the closed set is untrusted too
        r#"{"path":"household/0f0f.jpg","source":"ai"}"#,
    ] {
        std::fs::create_dir_all(dir.join("household")).unwrap();
        std::fs::write(dir.join("household/0f0f.jpg"), b"\xff\xd8\xff").unwrap();
        sqlx::query(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('household.portrait', ?)",
        )
        .bind(value)
        .execute(&pool)
        .await
        .unwrap();
        let (_, h) = send(&app, "GET", "/household", None).await;
        assert!(h["portrait"].is_null(), "{value}");
        let (s, _) = send(&app, "GET", "/household/portrait", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{value}");
        let (s, _) = send(&app, "DELETE", "/household/portrait", None).await;
        assert_eq!(s, StatusCode::OK);
        assert!(dir.join("secret.key").is_file(), "{value} reached the key");
        // DELETE still clears the untrusted row.
        let left: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM app_settings WHERE key = 'household.portrait'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0, "{value}");
    }
}

#[tokio::test]
async fn overlapping_puts_leave_exactly_one_file() {
    let (app, dir) = household().await;
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                "PUT",
                "/household/portrait",
                Some(json!({"media_type":"image/jpeg","data_base64": JPEG})),
            )
            .await
            .0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let (_, h) = send(&app, "GET", "/household", None).await;
    let v = h["portrait"]["version"].as_str().unwrap();
    assert_eq!(portrait_files(&dir), vec![format!("{v}.jpg")]);
}
