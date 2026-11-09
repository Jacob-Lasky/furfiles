//! MCP server (#22), driven through the real `/mcp` route of the full router:
//! auth, transport guards, the tool list, and every tool against the fictional
//! demo household.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower::ServiceExt;

use furfiles::mcp::{tool_names, McpToken, MIN_TOKEN_LEN};
use furfiles::{build_router, memory_pool, AppState};

const TOKEN: &str = "test-token-0123456789abcdef0123456789";
const PROTOCOL: &str = "2025-11-25";

async fn seeded() -> SqlitePool {
    let pool = memory_pool().await;
    furfiles::seed::ensure_owner(&pool).await.unwrap();
    furfiles::seed::seed_if_empty(&pool).await.unwrap();
    pool
}

fn router(pool: SqlitePool, token: Option<&str>) -> Router {
    let mut st = AppState::for_test(pool);
    st.mcp_token = McpToken::from_secret(token);
    build_router(st, std::env::temp_dir().join("furfiles-mcp-test-static"))
}

async fn app() -> (Router, SqlitePool) {
    let pool = seeded().await;
    (router(pool.clone(), Some(TOKEN)), pool)
}

/// POST one JSON-RPC message to `/mcp`, returning (status, headers, JSON body).
async fn post(
    app: &Router,
    auth: Option<&str>,
    extra: &[(&str, &str)],
    body: Value,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/mcp")
        // Real clients always send Host; the transport rejects a request without it.
        .header("host", "furfiles.test:9375")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", PROTOCOL);
    if let Some(a) = auth {
        req = req.header("authorization", a);
    }
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let val = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, headers, val)
}

fn bearer() -> String {
    format!("Bearer {TOKEN}")
}

async fn rpc(app: &Router, method: &str, params: Value) -> Value {
    let (status, _, body) = post(
        app,
        Some(&bearer()),
        &[],
        json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{method}: {body}");
    body
}

/// Call a tool and return its `result` (asserting the JSON-RPC call itself worked).
async fn call(app: &Router, name: &str, args: Value) -> Value {
    let body = rpc(app, "tools/call", json!({"name": name, "arguments": args})).await;
    body.get("result")
        .cloned()
        .unwrap_or_else(|| panic!("{name}: no result in {body}"))
}

/// A successful tool call's structured content.
async fn ok(app: &Router, name: &str, args: Value) -> Value {
    let r = call(app, name, args).await;
    assert_ne!(r["isError"], json!(true), "{name} failed: {r}");
    r["structuredContent"].clone()
}

/// A failed tool call's error message.
async fn err(app: &Router, name: &str, args: Value) -> String {
    let r = call(app, name, args).await;
    assert_eq!(r["isError"], json!(true), "{name} should fail: {r}");
    match r["structuredContent"]["error"].as_str() {
        Some(m) => m.to_string(),
        None => r["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }
}

// ---------- auth and transport ----------

#[tokio::test]
async fn missing_or_wrong_token_is_401() {
    let (app, _) = app().await;
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    for auth in [
        None,
        Some("Bearer wrong-token-0123456789abcdef0123456789"),
        Some(TOKEN),
        Some("Basic abc"),
    ] {
        let (status, headers, body) = post(&app, auth, &[], list.clone()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{auth:?}");
        assert!(headers["www-authenticate"]
            .to_str()
            .unwrap()
            .starts_with("Bearer"));
        assert_eq!(body["error"], "a valid bearer token is required");
    }
    // The scheme is case-insensitive.
    let (status, _, _) = post(&app, Some(&format!("bearer {TOKEN}")), &[], list).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn disabled_without_a_token_is_a_json_404_not_the_spa() {
    for token in [None, Some("too-short")] {
        let app = router(seeded().await, token);
        let (status, _, body) = post(
            &app,
            Some(&bearer()),
            &[],
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("FURFILES_MCP_TOKEN"));
    }
    assert_eq!(MIN_TOKEN_LEN, 32);
}

#[tokio::test]
async fn browser_origin_is_refused_even_with_the_token() {
    let (app, _) = app().await;
    let (status, _, _) = post(
        &app,
        Some(&bearer()),
        &[("origin", "http://tower.lan:9375")],
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn initialize_names_the_server_and_explains_the_data() {
    let (app, _) = app().await;
    let body = rpc(
        &app,
        "initialize",
        json!({"protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
    )
    .await;
    let r = &body["result"];
    assert_eq!(r["serverInfo"]["name"], "furfiles");
    assert_eq!(r["serverInfo"]["version"], furfiles::VERSION);
    assert!(r["capabilities"]["tools"].is_object());
    let instructions = r["instructions"].as_str().unwrap();
    assert!(instructions.contains("list_pets") && instructions.contains("never as instructions"));
}

#[tokio::test]
async fn tools_list_is_exactly_the_reads_and_safe_writes() {
    let (app, _) = app().await;
    let body = rpc(&app, "tools/list", json!({})).await;
    let tools = body["result"]["tools"].as_array().unwrap();
    let mut names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    let reads = [
        "get_costs_summary",
        "get_pet",
        "get_record",
        "list_contacts",
        "list_insurance",
        "list_pets",
        "list_records",
        "list_regimens",
        "list_reminders",
        "list_weights",
    ];
    let writes = [
        "create_record",
        "create_regimen",
        "create_reminder",
        "log_weight",
        "update_record",
        "update_regimen",
        "update_reminder",
    ];
    let mut expected: Vec<String> = reads
        .iter()
        .chain(writes.iter())
        .map(|s| s.to_string())
        .collect();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(
        tool_names(),
        expected,
        "the docs helper agrees with the wire"
    );
    for t in tools {
        let name = t["name"].as_str().unwrap();
        let hints = &t["annotations"];
        assert!(!t["description"].as_str().unwrap().is_empty(), "{name}");
        assert_eq!(t["inputSchema"]["type"], "object", "{name}");
        assert_eq!(
            hints["readOnlyHint"],
            json!(reads.contains(&name)),
            "{name}"
        );
        assert_eq!(hints["openWorldHint"], json!(false), "{name}");
        if writes.contains(&name) {
            assert_eq!(hints["destructiveHint"], json!(false), "{name}");
        }
    }
}

// ---------- pets ----------

#[tokio::test]
async fn list_pets_hides_archived_unless_asked() {
    let (app, _) = app().await;
    let active = ok(&app, "list_pets", json!({})).await;
    assert_eq!(active["count"], 5);
    let ids: Vec<&str> = active["pets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"biscuit") && !ids.contains(&"rex"));
    assert!(active["pets"][0]
        .as_object()
        .unwrap()
        .contains_key("next_reminder"));
    let all = ok(&app, "list_pets", json!({"include_archived": true})).await;
    assert_eq!(all["count"], 6);
}

#[tokio::test]
async fn get_pet_is_the_full_profile_and_unknown_pets_error() {
    let (app, _) = app().await;
    let p = ok(&app, "get_pet", json!({"pet_id": "biscuit"})).await;
    for key in [
        "records",
        "weights",
        "reminders",
        "regimens",
        "contacts",
        "allergies",
        "next_reminder",
    ] {
        assert!(p.get(key).is_some(), "missing {key}");
    }
    assert_eq!(p["species"], "Dog");
    assert!(err(&app, "get_pet", json!({"pet_id": "nobody"}))
        .await
        .contains("no pet 'nobody'"));
    // A missing required argument is a tool error, not a crash.
    assert!(err(&app, "get_pet", json!({})).await.contains("pet_id"));
}

// ---------- records ----------

#[tokio::test]
async fn records_list_filter_and_detail() {
    let (app, _) = app().await;
    let all = ok(&app, "list_records", json!({"pet_id": "biscuit"})).await;
    let records = all["records"].as_array().unwrap();
    assert_eq!(all["count"], records.len());
    assert!(records.len() >= 3);
    let dates: Vec<&str> = records
        .iter()
        .map(|r| r["occurred_on"].as_str().unwrap())
        .collect();
    assert!(dates.windows(2).all(|w| w[0] >= w[1]), "newest first");

    let boarding = ok(
        &app,
        "list_records",
        json!({"pet_id": "biscuit", "category": "Boarding"}),
    )
    .await;
    assert!(boarding["count"].as_u64().unwrap() >= 1);
    assert!(boarding["records"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["category"] == "Boarding"));
    assert!(err(
        &app,
        "list_records",
        json!({"pet_id": "biscuit", "category": "Vaccination"})
    )
    .await
    .contains("unknown variant"));

    let with_clinic = records
        .iter()
        .find(|r| r["contact_id"].is_string())
        .unwrap();
    let rid = with_clinic["id"].as_i64().unwrap();
    let detail = ok(
        &app,
        "get_record",
        json!({"pet_id": "biscuit", "record_id": rid}),
    )
    .await;
    assert_eq!(detail["id"], rid);
    assert!(detail["attachments"].is_array());
    assert_eq!(detail["contact"]["id"], with_clinic["contact_id"]);
    // Another pet's id cannot read it.
    assert!(err(
        &app,
        "get_record",
        json!({"pet_id": "miso", "record_id": rid})
    )
    .await
    .contains("for pet 'miso'"));
}

#[tokio::test]
async fn create_and_update_record() {
    let (app, _) = app().await;
    let rec = ok(
        &app,
        "create_record",
        json!({"pet_id": "miso", "occurred_on": "2026-03-04", "title": "  Dental cleaning ",
               "notes": "No extractions.", "cost_cents": 42000, "contact_id": "demo-riverside"}),
    )
    .await;
    assert_eq!(rec["title"], "Dental cleaning");
    assert_eq!(rec["category"], "Appointments", "default category");
    assert_eq!(rec["source"], "mcp");
    let rid = rec["id"].as_i64().unwrap();

    // Partial: absent keeps, null clears.
    let up = ok(
        &app,
        "update_record",
        json!({"pet_id": "miso", "record_id": rid, "category": "Grooming", "notes": null, "cost_cents": 41000}),
    )
    .await;
    assert_eq!(up["title"], "Dental cleaning");
    assert_eq!(up["category"], "Grooming");
    assert!(up["notes"].is_null());
    assert_eq!(up["cost_cents"], 41000);
    assert_eq!(up["contact_id"], "demo-riverside");

    // Validation is the REST handler's.
    assert!(err(
        &app,
        "create_record",
        json!({"pet_id": "miso", "occurred_on": "2026-02-30", "title": "x"})
    )
    .await
    .contains("ISO date"));
    assert!(err(
        &app,
        "create_record",
        json!({"pet_id": "miso", "occurred_on": "2026-02-03", "title": "x", "contact_id": "nope"})
    )
    .await
    .contains("no contact"));
    assert!(err(
        &app,
        "update_record",
        json!({"pet_id": "biscuit", "record_id": rid, "title": "y"})
    )
    .await
    .contains("for pet 'biscuit'"));
}

// ---------- weight ----------

#[tokio::test]
async fn weights_list_and_log() {
    let (app, _) = app().await;
    let before = ok(&app, "list_weights", json!({"pet_id": "comet"})).await["count"]
        .as_u64()
        .unwrap();
    let w = ok(
        &app,
        "log_weight",
        json!({"pet_id": "comet", "recorded_on": "2026-04-01", "value": 1100.5, "unit": "lb"}),
    )
    .await;
    assert_eq!(w["unit"], "lb");
    assert_eq!(w["value"], 1100.5);
    let after = ok(&app, "list_weights", json!({"pet_id": "comet"})).await;
    assert_eq!(after["count"].as_u64().unwrap(), before + 1);
    assert!(err(
        &app,
        "log_weight",
        json!({"pet_id": "comet", "recorded_on": "2026-04-01", "value": -2, "unit": "kg"})
    )
    .await
    .contains("positive"));
    assert!(err(
        &app,
        "log_weight",
        json!({"pet_id": "comet", "recorded_on": "2026-04-01", "value": 2, "unit": "stone"})
    )
    .await
    .contains("unknown variant"));
}

// ---------- reminders ----------

#[tokio::test]
async fn reminders_filter_by_pet_kind_done_and_archived() {
    let (app, _) = app().await;
    let open = ok(&app, "list_reminders", json!({})).await;
    let rems = open["reminders"].as_array().unwrap();
    assert!(rems.len() >= 6);
    assert!(rems.iter().all(|r| r["done"] == false));
    let dues: Vec<&str> = rems.iter().map(|r| r["due_on"].as_str().unwrap()).collect();
    assert!(dues.windows(2).all(|w| w[0] <= w[1]), "soonest first");

    let vaccines = ok(&app, "list_reminders", json!({"kind": "VACCINE"})).await;
    assert!(vaccines["count"].as_u64().unwrap() >= 3);
    assert!(vaccines["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["kind"] == "vaccine"));

    let biscuit = ok(&app, "list_reminders", json!({"pet_id": "biscuit"})).await;
    let b = biscuit["reminders"].as_array().unwrap();
    assert!(b.iter().all(|r| r["pet_id"] == "biscuit"));
    let first = b[0]["id"].as_i64().unwrap();

    // Done ones drop out unless asked for.
    ok(
        &app,
        "update_reminder",
        json!({"pet_id": "biscuit", "reminder_id": first, "done": true}),
    )
    .await;
    let after = ok(&app, "list_reminders", json!({"pet_id": "biscuit"})).await;
    assert_eq!(after["count"].as_u64().unwrap(), b.len() as u64 - 1);
    let with_done = ok(
        &app,
        "list_reminders",
        json!({"pet_id": "biscuit", "include_done": true}),
    )
    .await;
    assert_eq!(with_done["count"].as_u64().unwrap(), b.len() as u64);

    // Archived pets only with include_archived (household-wide).
    let archived_rems = ok(
        &app,
        "list_reminders",
        json!({"include_archived": true, "include_done": true}),
    )
    .await;
    let normal = ok(&app, "list_reminders", json!({"include_done": true})).await;
    assert!(archived_rems["count"].as_u64().unwrap() >= normal["count"].as_u64().unwrap());
    assert!(err(&app, "list_reminders", json!({"pet_id": "nobody"}))
        .await
        .contains("no pet"));
}

#[tokio::test]
async fn create_and_update_reminder() {
    let (app, _) = app().await;
    let r = ok(&app, "create_reminder", json!({"pet_id": "pip", "due_on": "2027-01-15", "title": "Beak trim", "kind": "appointment"})).await;
    assert_eq!(r["source"], "mcp");
    assert_eq!(r["kind"], "appointment");
    let id = r["id"].as_i64().unwrap();
    // Idempotent on pet + day + title.
    let again = ok(
        &app,
        "create_reminder",
        json!({"pet_id": "pip", "due_on": "2027-01-15", "title": "beak trim"}),
    )
    .await;
    assert_eq!(again["id"], id);
    // A blank kind is no kind.
    let nokind = ok(
        &app,
        "create_reminder",
        json!({"pet_id": "pip", "due_on": "2027-02-01", "title": "Nail trim", "kind": " "}),
    )
    .await;
    assert!(nokind["kind"].is_null());

    let moved = ok(&app, "update_reminder", json!({"pet_id": "pip", "reminder_id": id, "due_on": "2027-01-20", "title": " Beak and nail trim "})).await;
    assert_eq!(moved["due_on"], "2027-01-20");
    assert_eq!(moved["title"], "Beak and nail trim");
    assert_eq!(moved["done"], false);

    assert!(err(
        &app,
        "create_reminder",
        json!({"pet_id": "pip", "due_on": "next week", "title": "x"})
    )
    .await
    .contains("ISO date"));
    assert!(err(
        &app,
        "update_reminder",
        json!({"pet_id": "pip", "reminder_id": id, "title": "  "})
    )
    .await
    .contains("cannot be empty"));
    assert!(err(
        &app,
        "update_reminder",
        json!({"pet_id": "pip", "reminder_id": id, "due_on": "2027-13-01"})
    )
    .await
    .contains("ISO date"));
    // Another pet's reminder cannot be edited through this pet.
    assert!(err(
        &app,
        "update_reminder",
        json!({"pet_id": "miso", "reminder_id": id, "done": true})
    )
    .await
    .contains("for pet 'miso'"));
}

// ---------- ongoing care ----------

#[tokio::test]
async fn regimens_list_create_stop_and_restart() {
    let (app, _) = app().await;
    let all = ok(&app, "list_regimens", json!({"pet_id": "biscuit"})).await;
    assert!(all["count"].as_u64().unwrap() >= 3);
    let prev = ok(
        &app,
        "list_regimens",
        json!({"pet_id": "biscuit", "kind": "preventative"}),
    )
    .await;
    assert!(prev["regimens"]
        .as_array()
        .unwrap()
        .iter()
        .all(|g| g["kind"] == "preventative"));

    let med = ok(
        &app,
        "create_regimen",
        json!({"pet_id": "biscuit", "kind": "medication", "product": "Carprofen 75 mg", "amount": "1 tablet",
               "frequency": "2x/day", "started_on": "2026-05-01"}),
    )
    .await;
    let gid = med["id"].as_str().unwrap().to_string();
    assert!(med["ended_on"].is_null());

    let stopped = ok(&app, "update_regimen", json!({"pet_id": "biscuit", "regimen_id": gid, "ended_on": "2026-05-14", "amount": "half a tablet"})).await;
    assert_eq!(stopped["ended_on"], "2026-05-14");
    assert_eq!(stopped["amount"], "half a tablet");
    assert_eq!(stopped["frequency"], "2x/day", "absent keeps");
    let current = ok(
        &app,
        "list_regimens",
        json!({"pet_id": "biscuit", "current_only": true}),
    )
    .await;
    assert!(current["regimens"]
        .as_array()
        .unwrap()
        .iter()
        .all(|g| g["ended_on"].is_null() && g["id"] != gid.as_str()));
    let restarted = ok(
        &app,
        "update_regimen",
        json!({"pet_id": "biscuit", "regimen_id": gid, "ended_on": null}),
    )
    .await;
    assert!(restarted["ended_on"].is_null());

    // Protections only stick to preventatives (the REST rule).
    let p = ok(&app, "create_regimen", json!({"pet_id": "biscuit", "kind": "preventative", "product": "Monthly chew", "protects": ["flea", "tick"]})).await;
    assert_eq!(p["protects"], json!(["flea", "tick"]));
    let f = ok(
        &app,
        "update_regimen",
        json!({"pet_id": "biscuit", "regimen_id": p["id"], "kind": "food"}),
    )
    .await;
    assert!(f["protects"].is_null());
    let back = ok(&app, "update_regimen", json!({"pet_id": "biscuit", "regimen_id": p["id"], "kind": "preventative", "protects": ["heartworm"]})).await;
    assert_eq!(back["protects"], json!(["heartworm"]));

    assert!(err(
        &app,
        "update_regimen",
        json!({"pet_id": "miso", "regimen_id": gid, "notes": "x"})
    )
    .await
    .contains("for pet 'miso'"));
    assert!(err(
        &app,
        "create_regimen",
        json!({"pet_id": "biscuit", "kind": "medication", "product": " "})
    )
    .await
    .contains("product"));
    assert!(err(
        &app,
        "create_regimen",
        json!({"pet_id": "biscuit", "kind": "vaccine", "product": "x"})
    )
    .await
    .contains("unknown variant"));
}

// ---------- contacts, insurance, costs ----------

#[tokio::test]
async fn contacts_all_by_role_and_care_team() {
    let (app, _) = app().await;
    let all = ok(&app, "list_contacts", json!({})).await;
    assert_eq!(all["count"], 6);
    let vets = ok(&app, "list_contacts", json!({"role": "vet"})).await;
    assert!(vets["count"].as_u64().unwrap() >= 2);
    assert!(vets["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["role"] == "vet"));
    let team = ok(&app, "list_contacts", json!({"pet_id": "biscuit"})).await;
    assert_eq!(team["count"], 3);
    let team_boarding = ok(
        &app,
        "list_contacts",
        json!({"pet_id": "biscuit", "role": "boarding"}),
    )
    .await;
    assert!(team_boarding["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["role"] == "boarding"));
    assert!(team_boarding["count"].as_u64().unwrap() < 3);
    assert!(err(&app, "list_contacts", json!({"pet_id": "nobody"}))
        .await
        .contains("no pet"));
}

#[tokio::test]
async fn insurance_and_costs() {
    let (app, _) = app().await;
    let policies = ok(&app, "list_insurance", json!({"pet_id": "biscuit"})).await;
    assert_eq!(policies["count"], 1);
    assert!(policies["policies"][0]["provider"].is_string());
    assert_eq!(
        ok(&app, "list_insurance", json!({"pet_id": "miso"})).await["count"],
        0
    );
    let costs = ok(&app, "get_costs_summary", json!({})).await;
    assert!(costs.is_object() && !costs.as_object().unwrap().is_empty());
}
