//! Bulk import (#23), end to end: a mock Anthropic server returns a canned
//! `import_plan` tool call, so `/import/analyze` → `/import/commit` runs through
//! the real handlers, database and attachment store. The model's output is
//! deliberately messy (bogus ids, duplicates, a nickname) to prove the server's
//! own checks hold even when the model slips.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};

mod common;
use common::send;

use furfiles::import::{build_plan, import_tool_schema, parse_import_response, Household, ImportExtraction, ImportPlan};
use furfiles::{api_router, memory_pool, AppState};

/// Start a fake Messages API that answers every request with `response` and
/// records each request body it saw.
async fn mock_anthropic(response: Value) -> (String, Arc<Mutex<Vec<Value>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let app = Router::new().route(
        "/v1/messages",
        post(move |Json(body): Json<Value>| {
            let seen = seen2.clone();
            let response = response.clone();
            async move {
                seen.lock().unwrap().push(body);
                Json(response)
            }
        }),
    );
    (common::serve(app).await, seen)
}

fn tool_reply(input: Value) -> Value {
    json!({
        "stop_reason": "tool_use",
        "content": [{ "type": "tool_use", "name": "import_plan", "input": input }]
    })
}

fn b64(s: &str) -> String {
    B64.encode(s.as_bytes())
}

/// A typical household: two dogs and a cat saved under a nickname,
/// the new vet already saved under a short name, and some history.
async fn household(app: &Router) -> (String, i64) {
    for (name, species, dob) in [
        ("Lulu", "Dog", "2025-11-04"),
        ("Pedro", "Dog", "2024-05-06"),
        ("Kipper", "Cat", "2017-04-18"),
    ] {
        let (s, _) = send(
            app,
            "POST",
            "/pets",
            Some(json!({ "name": name, "species": species, "birth_date": dob, "breed": "Mix" })),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let (_, amc) = send(
        app,
        "POST",
        "/contacts",
        Some(json!({ "name": "AMC of Springfield", "role": "vet", "phone": "555-010-0142" })),
    )
    .await;
    let amc_id = amc["id"].as_str().unwrap().to_string();
    // Lulu's 9/4 visit is already in, with its invoice attached.
    let (_, rec) = send(
        app,
        "POST",
        "/pets/lulu/records",
        Some(json!({ "occurred_on": "2026-09-04", "title": "Comprehensive exam", "cost_cents": 50603 })),
    )
    .await;
    let rid = rec["id"].as_i64().unwrap();
    let (s, _) = send(
        app,
        "POST",
        &format!("/records/{rid}/attachments"),
        Some(json!({ "filename": "invoice.pdf", "media_type": "application/pdf", "data_base64": b64("INVOICE-260743") })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    send(app, "POST", "/pets/lulu/weights", Some(json!({ "recorded_on": "2026-09-11", "value": 55.8, "unit": "lb" }))).await;
    send(app, "POST", "/pets/lulu/weights", Some(json!({ "recorded_on": "2026-09-04", "value": 55.8, "unit": "lb" }))).await;
    send(app, "POST", "/pets/lulu/reminders", Some(json!({ "due_on": "2027-03-05", "title": "Lepto - Annual" }))).await;
    send(app, "POST", "/pets/lulu/reminders", Some(json!({ "due_on": "2026-11-05", "title": "SNAP 4Dx test" }))).await;
    send(
        app,
        "POST",
        "/pets/kipper/regimens",
        Some(json!({ "kind": "food", "product": "Hill's Science Diet 7+" })),
    )
    .await;
    send(app, "PUT", "/settings/ingestion", Some(json!({ "api_key": "sk-ant-test", "enabled": true }))).await;
    (amc_id, rid)
}

async fn app_with_mock(response: Value) -> (Router, Arc<Mutex<Vec<Value>>>) {
    let (base, seen) = mock_anthropic(response).await;
    let mut st = AppState::for_test(memory_pool().await);
    st.anthropic_base = base;
    (api_router(st), seen)
}

fn messy_extraction(snap_reminder_id: i64) -> Value {
    json!({
        "summary": "Invoice for Lulu and Pedro plus a history for Sir Whiskers.",
        "clinic": { "name": "Animal Medical Center of Springfield", "phone": "(555) 010-0142", "role": "vet" },
        "records": [
            // Same pet, date and total as the saved record → duplicate.
            { "pet_id": "lulu", "pet_name_on_document": "Lulu", "occurred_on": "2026-09-04", "title": "Exam + diagnostics", "cost": 506.03 },
            // New, with a priced breakdown.
            { "pet_id": "pedro", "pet_name_on_document": "Pedro", "occurred_on": "2026-09-04", "title": "Exam + vaccines",
              "cost": 189.48, "category": "Appointments",
              "line_items": [ { "description": "Comprehensive Exam", "amount": 73.60 }, { "description": "  " } ],
              "extra_fields": { "Lepto lot": "L123", "": "x" }, "notes": "Bordetella oral + Lepto." },
            // A bogus pet id but the document name resolves to Kipper's saved name? No:
            // it says Sir Whiskers, so it stays unassigned until the user picks.
            { "pet_id": "sir-whiskers", "pet_name_on_document": "Sir Whiskers", "occurred_on": "2025-11-17", "title": "Annual exam", "category": "Nonsense" },
            // Name-only match (case-insensitive) recovers the pet.
            { "pet_name_on_document": "KIPPER", "occurred_on": "2025-04-02", "title": "Hypersalivation exam" },
            // Invalid date → kept for review, not selected.
            { "pet_id": "kipper", "occurred_on": "2025-02-31", "title": "Bad date" }
        ],
        "weights": [
            { "pet_id": "lulu", "recorded_on": "2026-09-04", "value": 55.8, "unit": "lb" },
            { "pet_id": "lulu", "recorded_on": "2026-09-11", "value": 55.4, "unit": "lb" },
            { "pet_id": "pedro", "recorded_on": "2026-09-04", "value": 48.0, "unit": "lb" },
            { "pet_id": "pedro", "recorded_on": "2026-09-04", "value": 99.0, "unit": "lb" },
            { "pet_id": "ghost", "recorded_on": "2026-09-04", "value": 1.0 },
            { "pet_id": "kipper", "recorded_on": "2025-11-17", "value": 6.6, "unit": "kg" },
            { "pet_id": "kipper", "recorded_on": "2026-10-06", "value": 14.5, "unit": "lb", "date_is_inferred": true }
        ],
        "reminders": [
            { "pet_id": "lulu", "due_on": "2027-03-05", "title": "lepto - annual" },
            { "pet_id": "pedro", "due_on": "2027-03-05", "title": "Bordetella - Oral" },
            { "pet_id": "pedro", "due_on": "2027-03-05", "title": "Bordetella - Oral" },
            { "pet_id": "pedro", "due_on": "someday", "title": "Bad" }
        ],
        "completed_reminders": [
            { "reminder_id": snap_reminder_id, "reason": "4Dx run 9/4" },
            { "reminder_id": 99999 },
            // Lepto 2027-03-05 is the NEXT due date, far past the 9/4 visit: refused.
            { "reminder_id": 1, "reason": "last done 03-05-26" }
        ],
        "pet_updates": [
            { "pet_id": "kipper", "field": "microchip", "value": "000000000000201", "reason": "On the record" },
            { "pet_id": "kipper", "field": "name", "value": "Sir Whiskers" },
            { "pet_id": "kipper", "field": "breed", "value": "mix" },
            { "pet_id": "kipper", "field": "insurer", "value": "Lemonade" },
            { "pet_id": "kipper", "field": "birth_date", "value": "Apr 18 2017" },
            { "pet_id": "kipper", "field": "color", "value": "White and Brown Tabby" }
        ],
        "regimens": [
            { "pet_id": "kipper", "kind": "food", "product": "hills science diet 7+" },
            { "pet_id": "kipper", "kind": "preventative", "product": "Revolution Plus", "frequency": "monthly",
              "protects": ["Flea", "tick", "heartworm", "worm", "lice"] },
            { "pet_id": "kipper", "kind": "medication", "product": "Convenia", "protects": ["flea"] },
            { "pet_id": "kipper", "kind": "potion", "product": "Nope" }
        ]
    })
}

#[tokio::test]
async fn analyze_then_commit_end_to_end() {
    // The reminder id is only known after seeding, but the mock is built first,
    // so seed a throwaway app to learn ids deterministically: ids are 1-based in
    // a fresh DB, and the SNAP reminder is the second one created.
    let (app, seen) = app_with_mock(tool_reply(messy_extraction(2))).await;
    let (amc_id, existing_rid) = household(&app).await;

    // ---- analyze: a document already in the library (same bytes) ----
    let (s, dup_plan) = send(
        &app,
        "POST",
        "/import/analyze",
        Some(json!({ "filename": "AVIEE (1).PDF", "media_type": "application/pdf", "data_base64": b64("INVOICE-260743") })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{dup_plan}");
    assert_eq!(dup_plan["file_duplicates"][0]["record_id"], existing_rid);
    assert_eq!(dup_plan["file_duplicates"][0]["pet_name"], "Lulu");
    // Per PET: the file is on Lulu's record, so Lulu's visits from it are not
    // offered again, while other pets' visits in the same file still are.
    for r in dup_plan["records"].as_array().unwrap() {
        if r["pet_id"] == "lulu" {
            assert_eq!(r["selected"], false, "Lulu already has this file: {r}");
        }
    }
    let pedro = dup_plan["records"].as_array().unwrap().iter().find(|r| r["pet_id"] == "pedro").unwrap();
    assert_eq!(pedro["selected"], true, "Pedro's visit in the same file is still offered");

    // ---- analyze: a new document ----
    let (s, plan) = send(
        &app,
        "POST",
        "/import/analyze",
        Some(json!({ "filename": "new.pdf", "media_type": "application/pdf", "data_base64": b64("NEW-DOC") })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{plan}");
    assert_eq!(plan["provider"], "anthropic");
    assert!(plan["file_duplicates"].as_array().unwrap().is_empty());
    assert_eq!(plan["sha256"].as_str().unwrap().len(), 64);

    // The model saw the household, with real ids constrained by enum.
    let req = seen.lock().unwrap().last().unwrap().clone();
    let schema = &req["tools"][0]["input_schema"];
    let pet_enum = &schema["properties"]["records"]["items"]["properties"]["pet_id"]["enum"];
    assert!(pet_enum.as_array().unwrap().contains(&json!("kipper")));
    assert_eq!(req["tools"][0]["name"], "import_plan");
    assert_eq!(req["tool_choice"]["type"], "auto"); // never forced (#87)
    let ctx = req["messages"][0]["content"][2]["text"].as_str().unwrap();
    assert!(ctx.contains("\"existing_records\"") && ctx.contains("Comprehensive exam"));
    assert!(ctx.contains("SNAP 4Dx test"), "open reminders are in context");

    // Clinic matched by phone despite a different name.
    assert_eq!(plan["clinic"]["contact_id"], amc_id);
    assert_eq!(plan["clinic"]["matched_name"], "AMC of Springfield");

    let recs = plan["records"].as_array().unwrap();
    assert_eq!(recs.len(), 5);
    assert_eq!(recs[0]["duplicate_of"]["record_id"], existing_rid, "same pet+date+total");
    assert_eq!(recs[0]["selected"], false);
    assert_eq!(recs[1]["selected"], true);
    assert_eq!(recs[1]["cost_cents"], 18948);
    assert_eq!(recs[1]["line_items"].as_array().unwrap().len(), 1, "blank line dropped");
    assert_eq!(recs[1]["line_items"][0]["amount_cents"], 7360);
    assert_eq!(recs[1]["extra"], json!({ "Lepto lot": "L123" }));
    assert!(recs[2]["pet_id"].is_null(), "bogus id and unknown name stay unassigned");
    assert_eq!(recs[2]["selected"], true, "ticked, so the screen blocks saving until a pet is picked");
    assert_eq!(recs[2]["category"], "Appointments", "unknown category falls back");
    assert_eq!(recs[3]["pet_id"], "kipper", "matched by name");
    assert_eq!(recs[3]["selected"], true);
    assert_eq!(recs[4]["occurred_on"], "", "impossible date blanked");
    assert_eq!(recs[4]["selected"], false);

    // Care team: every pet seen at AMC, Lulu included even though her visit is a
    // duplicate (it still proves AMC is her vet).
    let team: Vec<&str> = plan["care_team"].as_array().unwrap().iter().map(|l| l["pet_id"].as_str().unwrap()).collect();
    assert_eq!(team, vec!["lulu", "pedro", "kipper"]);

    let w = plan["weights"].as_array().unwrap();
    assert_eq!(w.len(), 4, "identical, repeated-date and unknown-pet weights dropped: {w:?}");
    assert_eq!(w[3]["date_is_inferred"], true);
    assert_eq!(w[3]["selected"], false, "a guessed measurement date starts unticked");
    assert_eq!(w[0]["pet_id"], "lulu");
    assert_eq!(w[0]["existing_value"], 55.8);
    assert_eq!(w[0]["selected"], false, "a correction overwrites, so it's opt-in");
    assert_eq!(w[1]["pet_id"], "pedro");
    assert_eq!(w[1]["selected"], true);
    assert_eq!(w[2]["unit"], "kg");

    let r = plan["reminders"].as_array().unwrap();
    assert_eq!(r.len(), 2);
    assert!(r[0]["duplicate_of"].is_i64() && r[0]["selected"] == false, "exact match, any case");
    assert_eq!(r[1]["title"], "Bordetella - Oral");
    assert_eq!(r[1]["selected"], true);

    let done = plan["completed_reminders"].as_array().unwrap();
    assert_eq!(done.len(), 1, "unknown id and next-due-date reminder dropped: {done:?}");
    assert_eq!(done[0]["title"], "SNAP 4Dx test");

    let ups = plan["pet_updates"].as_array().unwrap();
    let by_field = |f: &str| ups.iter().find(|u| u["field"] == f).cloned();
    assert_eq!(by_field("microchip").unwrap()["selected"], true, "filling a blank is on");
    assert_eq!(by_field("name").unwrap()["selected"], false, "overwriting is opt-in");
    assert_eq!(by_field("name").unwrap()["current"], "Kipper");
    assert!(by_field("breed").is_none(), "same value, different case, dropped");
    assert!(by_field("insurer").is_none(), "not an importable field");
    assert!(by_field("birth_date").is_none(), "unparseable date dropped");
    assert!(by_field("color").is_some());

    let regs = plan["regimens"].as_array().unwrap();
    assert_eq!(regs.len(), 3, "unknown kind dropped");
    assert!(regs[0]["duplicate_of"].is_string() && regs[0]["selected"] == false, "same product");
    assert_eq!(regs[1]["protects"], json!(["flea", "tick", "heartworm", "worm"]));
    assert_eq!(regs[2]["protects"], json!([]), "only preventatives protect");

    // ---- commit: assign the unassigned record, accept a correction ----
    let mut plan = plan;
    plan["records"][2]["pet_id"] = json!("kipper");
    plan["records"][2]["selected"] = json!(true);
    plan["weights"][0]["selected"] = json!(true);
    plan["pet_updates"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .for_each(|u| u["selected"] = json!(true));
    let (s, res) = send(
        &app,
        "POST",
        "/import/commit",
        Some(json!({ "filename": "new.pdf", "media_type": "application/pdf", "data_base64": b64("NEW-DOC"), "plan": plan })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(res["records"].as_array().unwrap().len(), 3);
    assert!(res["contact"].is_null(), "existing clinic reused, none created");
    assert_eq!(res["care_team_links"], 3);
    assert_eq!(res["weights_added"], 2);
    assert_eq!(res["weights_corrected"], 1);
    assert_eq!(res["reminders_added"], 1);
    assert_eq!(res["reminders_completed"], 1);
    assert_eq!(res["pet_updates"], 3);
    assert_eq!(res["regimens_added"], 2);

    // The data landed where it should.
    let (_, kipper) = send(&app, "GET", "/pets/kipper", None).await;
    assert_eq!(kipper["name"], "Sir Whiskers");
    assert_eq!(kipper["microchip"], "000000000000201");
    assert_eq!(kipper["records"].as_array().unwrap().len(), 2);
    let rec = &kipper["records"][0];
    assert_eq!(rec["source"], "import");
    assert_eq!(rec["contact"]["id"], amc_id);
    assert_eq!(rec["attachments"][0]["filename"], "new.pdf");
    assert_eq!(rec["attachments"][0]["source"], "scan");
    assert!(kipper["contacts"].as_array().unwrap().iter().any(|c| c["id"] == amc_id));
    assert!(kipper["regimens"].as_array().unwrap().iter().any(|r| r["product"] == "Revolution Plus"));
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    let w911 = lulu["weights"].as_array().unwrap().iter().find(|w| w["recorded_on"] == "2026-09-11").unwrap().clone();
    assert_eq!(w911["value"], 55.4, "corrected in place, not duplicated");
    assert_eq!(lulu["weights"].as_array().unwrap().len(), 2);
    assert!(lulu["reminders"].as_array().unwrap().iter().any(|r| r["title"] == "SNAP 4Dx test" && r["done"] == true));

    // ---- re-analyzing the same file now finds it ----
    let (_, again) = send(
        &app,
        "POST",
        "/import/analyze",
        Some(json!({ "filename": "renamed copy.pdf", "media_type": "application/pdf", "data_base64": b64("NEW-DOC") })),
    )
    .await;
    assert_eq!(again["file_duplicates"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn a_reuploaded_vet_file_keeps_its_clinic_even_with_no_new_visits() {
    // The model rightly reports no new visits for an invoice already saved; the
    // clinic, care team and clinic links must still come through (seen live).
    let (app, _) = app_with_mock(tool_reply(json!({
        "summary": "Already saved.",
        "clinic": { "name": "AMC of Springfield", "phone": "555-010-0142" },
        // The 4Dx on that saved 9/4 visit completes the SNAP reminder due 11/5,
        // even though the model reported no visit records.
        "completed_reminders": [ { "reminder_id": 2 } ]
    })))
    .await;
    let (amc_id, rid) = household(&app).await;
    let (_, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": b64("INVOICE-260743") }))).await;
    assert_eq!(plan["clinic"]["contact_id"], amc_id.as_str(), "{plan}");
    assert_eq!(plan["care_team"][0]["pet_id"], "lulu");
    assert_eq!(plan["clinic_links"][0]["record_id"], rid);
    assert_eq!(plan["completed_reminders"].as_array().unwrap().len(), 1, "the attached visit is the evidence");
    // …while an insurance-only document still never yields a clinic.
    let (app2, _) = app_with_mock(tool_reply(json!({
        "clinic": { "name": "Lemonade" },
        "claims": [ { "pet_id": "lulu", "claim_number": "C1", "treatment_date": "2026-09-04", "total_paid": 1.0 } ]
    })))
    .await;
    household(&app2).await;
    let (_, p2) = send(&app2, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": b64("EOB") }))).await;
    assert!(p2["clinic"].is_null() && p2["care_team"].as_array().unwrap().is_empty(), "{p2}");
}

#[tokio::test]
async fn commit_creates_a_new_clinic_and_refuses_half_baked_plans() {
    let (app, _) = app_with_mock(tool_reply(json!({
        "clinic": { "name": "Oak Hollow Animal Clinic", "phone": "+1 555 010 0177", "role": "made-up" },
        "records": [ { "pet_id": "kipper", "occurred_on": "2026-10-06", "title": "Vaccination summary" } ]
    })))
    .await;
    household(&app).await;
    let (s, plan) = send(
        &app,
        "POST",
        "/import/analyze",
        Some(json!({ "filename": "vax.png", "media_type": "image/png", "data_base64": b64("PNG") })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{plan}");
    assert!(plan["clinic"]["contact_id"].is_null(), "no match → new contact");
    assert_eq!(plan["clinic"]["role"], "vet", "unknown role falls back");
    assert_eq!(plan["care_team"][0]["pet_id"], "kipper");

    let commit = |plan: Value| {
        json!({ "filename": "vax.png", "media_type": "image/png", "data_base64": b64("PNG"), "plan": plan })
    };

    // A selected record with no pet is refused, and NOTHING is written.
    let mut bad = plan.clone();
    bad["records"][0]["pet_id"] = Value::Null;
    let (s, err) = send(&app, "POST", "/import/commit", Some(commit(bad))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("choose which pet"));
    let (_, contacts) = send(&app, "GET", "/contacts", None).await;
    assert_eq!(contacts.as_array().unwrap().len(), 1, "validation runs before any write");

    // Other refusals: unknown pet, bad date, bad field, blank new-clinic name.
    for (path, val) in [
        ("/records/0/pet_id", json!("ghost")),
        ("/records/0/occurred_on", json!("2026-13-01")),
        ("/records/0/title", json!("  ")),
        ("/clinic/name", json!("")),
        ("/clinic/contact_id", json!("no-such-contact")),
    ] {
        let mut p = plan.clone();
        *p.pointer_mut(path).unwrap() = val;
        let (s, body) = send(&app, "POST", "/import/commit", Some(commit(p))).await;
        assert_ne!(s, StatusCode::CREATED, "{path} should be refused: {body}");
    }
    let mut p = plan.clone();
    p["pet_updates"] = json!([{ "selected": true, "pet_id": "kipper", "field": "insurer", "value": "x" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(p))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Sex is a closed set (#72): an edited plan can't smuggle free text in.
    let mut p = plan.clone();
    p["pet_updates"] = json!([{ "selected": true, "pet_id": "kipper", "field": "sex", "value": "Neutered male" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(p))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut p = plan.clone();
    p["regimens"] = json!([{ "selected": true, "pet_id": "kipper", "kind": "potion", "product": "x" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(p))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut p = plan.clone();
    p["weights"] = json!([{ "selected": true, "pet_id": "ghost", "recorded_on": "2026-01-01", "value": 1.0, "unit": "lb" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(p))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut p = plan.clone();
    p["reminders"] = json!([{ "selected": true, "pet_id": "kipper", "due_on": "2026-01-01", "title": " " }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(p))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // The real thing: creates the clinic, links it, attaches the image, and
    // applies the profile updates (every importable field kind).
    let mut plan = plan;
    plan["pet_updates"] = json!([
        { "selected": true, "pet_id": "kipper", "field": "sex", "current": null, "value": "male_neutered" },
        { "selected": true, "pet_id": "kipper", "field": "birth_date", "current": "2017-04-18", "value": "2017-04-17" },
        { "selected": true, "pet_id": "kipper", "field": "breed", "current": "mix", "value": "Domestic Shorthair" },
        { "selected": false, "pet_id": "kipper", "field": "name", "current": "Kipper", "value": "Not applied" }
    ]);
    let (s, res) = send(&app, "POST", "/import/commit", Some(commit(plan.clone()))).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!(res["contact"]["name"], "Oak Hollow Animal Clinic");
    assert_eq!(res["contact"]["phone"], "+1 555 010 0177");
    let (_, kipper) = send(&app, "GET", "/pets/kipper", None).await;
    assert_eq!(kipper["records"][0]["contact"]["name"], "Oak Hollow Animal Clinic");
    assert_eq!(kipper["records"][0]["attachments"][0]["media_type"], "image/png");
    assert_eq!(kipper["sex"], "male_neutered");
    assert_eq!(kipper["birth_date"], "2017-04-17");
    assert_eq!(kipper["breed"], "Domestic Shorthair");
    assert_eq!(kipper["name"], "Kipper", "unselected update not applied");

    // A retry of the SAME commit (the client never saw the first response) must
    // not duplicate the record or the new contact.
    let (s, again) = send(&app, "POST", "/import/commit", Some(commit(plan))).await;
    assert_eq!(s, StatusCode::CREATED, "{again}");
    assert!(again["records"].as_array().unwrap().is_empty(), "already-imported record skipped");
    assert!(again["contact"].is_null(), "existing contact reused, not re-created");
    let (_, kipper) = send(&app, "GET", "/pets/kipper", None).await;
    assert_eq!(kipper["records"].as_array().unwrap().len(), 1);
    let (_, contacts) = send(&app, "GET", "/contacts", None).await;
    assert_eq!(contacts.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn analyze_errors_are_clean() {
    // Not set up → 400 with a hint, before anything else.
    let app = api_router(AppState::for_test(memory_pool().await));
    let (s, e) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": b64("x") }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(e["error"].as_str().unwrap().contains("API key"));

    // Truncated output → 502 telling the user to split the file.
    let (app, _) = app_with_mock(json!({ "stop_reason": "max_tokens", "content": [] })).await;
    send(&app, "PUT", "/settings/ingestion", Some(json!({ "api_key": "sk-ant-test", "enabled": true }))).await;
    let (s, e) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": b64("x") }))).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(e["error"].as_str().unwrap().contains("split"));

    // Unsupported type and bad base64 → 400 without a model call.
    let (s, _) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "text/plain", "data_base64": b64("x") }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": "%%%" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // The provider's own error is relayed.
    let (app, _) = app_with_mock(json!({ "content": [{ "type": "text", "text": "no tool" }] })).await;
    send(&app, "PUT", "/settings/ingestion", Some(json!({ "api_key": "sk-ant-test", "enabled": true }))).await;
    let (s, _) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "image/jpeg", "data_base64": b64("x") }))).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
}

#[test]
fn parse_rejects_unusable_responses() {
    assert!(parse_import_response(&json!({ "content": [] })).is_err());
    let ok = parse_import_response(&tool_reply(json!({ "records": [], "summary": "x" }))).unwrap();
    assert_eq!(ok.summary.as_deref(), Some("x"));
    assert!(parse_import_response(&tool_reply(json!({ "records": "nope" }))).is_err());
    // A wrapped plan (seen live from claude-sonnet-5) is unwrapped, as an
    // object or as a JSON string; a wrapper that isn't a plan still fails.
    let wrapped = parse_import_response(&tool_reply(json!({ "plan": { "summary": "w", "records": [] } }))).unwrap();
    assert_eq!(wrapped.summary.as_deref(), Some("w"));
    let stringly = parse_import_response(&tool_reply(json!({ "plan": "{\"summary\":\"s\"}" }))).unwrap();
    assert_eq!(stringly.summary.as_deref(), Some("s"));
    let e = parse_import_response(&tool_reply(json!({ "plan": "not json" }))).unwrap_err();
    assert!(e.contains("keys: plan"), "{e}");
    assert!(parse_import_response(&tool_reply(json!({ "plan": 3 }))).is_err());
    // Sloppy nulls (seen live: "clinic": "null") read as absent, not as a failure.
    let sloppy = parse_import_response(&tool_reply(json!({
        "summary": "s", "clinic": "null", "records": null,
        "weights": [ null, { "pet_id": "lulu", "recorded_on": "2026-01-01", "value": 1.0, "unit": null } ]
    })))
    .unwrap();
    assert!(sloppy.clinic.is_none() && sloppy.records.is_empty());
    assert_eq!(sloppy.weights.len(), 1);
    assert!(sloppy.weights[0].unit.is_none());
    // An empty tool call is a failed read, not an empty document.
    let e = parse_import_response(&tool_reply(json!({}))).unwrap_err();
    assert!(e.contains("empty answer") && e.contains("tool_use"), "{e}");
    let mut refused = tool_reply(json!({}));
    refused["stop_reason"] = json!("refusal");
    assert!(parse_import_response(&refused).unwrap_err().contains("declined"));
}

#[test]
fn plan_with_an_empty_household_proposes_nothing_assignable() {
    let x: ImportExtraction = serde_json::from_value(json!({
        "clinic": { "name": "  " },
        "records": [ { "pet_id": "rex", "occurred_on": "2026-01-01", "title": "Visit", "cost": 12.345 },
                     { "occurred_on": "2026-01-02", "title": "Unpriced", "cost": 0 },
                     { "occurred_on": "2026-01-03", "title": "Free item", "cost": 0, "line_items": [ { "description": "Nail trim", "amount": 0 } ] } ],
        "pet_updates": [ { "pet_id": "rex", "field": "name", "value": "Rex" } ]
    }))
    .unwrap();
    let plan = build_plan(x, &Household::default(), "abc".into(), vec![]);
    assert!(plan.clinic.is_none(), "a blank clinic name with no match is dropped");
    assert!(plan.records[0].pet_id.is_none());
    assert_eq!(plan.records[0].cost_cents, Some(1235), "rounded to the cent");
    assert_eq!(plan.records[1].cost_cents, None, "$0 with no priced line means no prices shown");
    assert_eq!(plan.records[2].cost_cents, None, "$0 lines are no prices too (seen live)");
    assert_eq!(plan.records[2].line_items[0].amount_cents, None);
    assert!(plan.records[0].selected, "unassigned stays ticked; commit refuses it until a pet is chosen");
    assert!(plan.care_team.is_empty());
    assert!(plan.pet_updates.is_empty());
}

// ---------- the supporting changes ----------

#[test]
fn a_model_regimen_id_only_counts_for_the_same_product() {
    let apoquel = furfiles::models::Regimen {
        id: "apo".into(), pet_id: "lulu".into(), kind: "medication".into(), product: "Apoquel".into(),
        amount: Some("16 mg".into()), frequency: Some("daily".into()), protects: None,
        started_on: None, ended_on: None, notes: None, created_by: None,
        created_at: String::new(), updated_at: String::new(),
    };
    let h = Household::default(); // no pets: every pet id is unknown
    let x: ImportExtraction = serde_json::from_value(json!({
        "regimens": [ { "pet_id": "lulu", "kind": "medication", "product": "Cytopoint", "amount": "1 shot", "duplicate_of_regimen_id": "apo" } ]
    })).unwrap();
    // Without the pet on file nothing is planned; that is the id check, not this one.
    assert!(build_plan(x.clone(), &h, "s".into(), vec![]).regimens.is_empty());
    let lulu = furfiles::models::Pet {
        id: "lulu".into(), name: "Lulu".into(), species: "Dog".into(), breed: None, sex: None,
        birth_date: None, gotcha_date: None, weight: None, weight_unit: None, microchip: None,
        color: None, allergies: None, insurer: None, insurer_reimburse: None, gradient_a: None,
        gradient_b: None, photo_path: None, archived_at: None, archive_kind: None, memorial_note: None,
        created_by: None, created_at: String::new(), updated_at: String::new(),
    };
    let h = Household { pets: vec![lulu], current_regimens: vec![apoquel], ..Default::default() };
    let plan = build_plan(x, &h, "s".into(), vec![]);
    assert!(plan.regimens[0].duplicate_of.is_none(), "not hidden as Apoquel");
    assert!(plan.regimens[0].replaces.is_none(), "and never ends Apoquel");
}

fn a_pet(id: &str, name: &str, sex: Option<&str>) -> furfiles::models::Pet {
    furfiles::models::Pet {
        id: id.into(), name: name.into(), species: "Dog".into(), breed: None, sex: sex.map(Into::into),
        birth_date: None, gotcha_date: None, weight: None, weight_unit: None, microchip: None,
        color: None, allergies: None, insurer: None, insurer_reimburse: None, gradient_a: None,
        gradient_b: None, photo_path: None, archived_at: None, archive_kind: None, memorial_note: None,
        created_by: None, created_at: String::new(), updated_at: String::new(),
    }
}

/// #72: the document's wording lands on the closed set, and a spay or neuter
/// the document shows being PERFORMED is ticked even though it overwrites.
#[test]
fn a_performed_sterilisation_is_ticked_even_over_an_intact_sex() {
    let h = Household {
        pets: vec![
            a_pet("lulu", "Lulu", Some("female")),
            a_pet("pedro", "Pedro", Some("male")),
            a_pet("kipper", "Kipper", None),
            a_pet("rocco", "Rocco", Some("unknown")),
        ],
        ..Default::default()
    };
    let sex_update = |plan: &ImportPlan, pet: &str| {
        plan.pet_updates.iter().find(|u| u.pet_id == pet && u.field == "sex").cloned()
    };

    // A spay invoice: the record proves it even without the model's flag.
    let x: ImportExtraction = serde_json::from_value(json!({
        "records": [ { "pet_id": "lulu", "occurred_on": "2026-09-21", "title": "Spay", "category": "Appointments" } ],
        "pet_updates": [ { "pet_id": "lulu", "field": "sex", "value": "Spayed Female" } ]
    })).unwrap();
    let u = sex_update(&build_plan(x, &h, "s".into(), vec![]), "lulu").unwrap();
    assert_eq!((u.value.as_str(), u.current.as_deref(), u.selected), ("female_spayed", Some("female"), true));

    // The model's flag alone counts, but only when it points the right way.
    let x: ImportExtraction = serde_json::from_value(json!({
        "pet_updates": [
            { "pet_id": "pedro", "field": "sex", "value": "MN", "sterilisation_performed": true },
            { "pet_id": "lulu", "field": "sex", "value": "male_neutered", "sterilisation_performed": true }
        ]
    })).unwrap();
    let plan = build_plan(x, &h, "s".into(), vec![]);
    let pedro = sex_update(&plan, "pedro").unwrap();
    assert_eq!((pedro.value.as_str(), pedro.selected), ("male_neutered", true));
    assert!(!sex_update(&plan, "lulu").unwrap().selected, "a female can't be neutered male; offered, not ticked");

    // A status printed in a header is not a procedure: the overwrite is offered
    // unticked, as every other overwrite is. Filling a blank or `unknown` is on.
    // `unknown` from the document is dropped, and wording maps onto the set.
    let x: ImportExtraction = serde_json::from_value(json!({
        "records": [ { "pet_id": "lulu", "occurred_on": "2026-10-01", "title": "Annual exam", "category": "Appointments" } ],
        "pet_updates": [
            { "pet_id": "lulu", "field": "sex", "value": "FS" },
            { "pet_id": "kipper", "field": "sex", "value": "Neutered Male" },
            { "pet_id": "rocco", "field": "sex", "value": "M" },
            { "pet_id": "pedro", "field": "sex", "value": "not stated" }
        ]
    })).unwrap();
    let plan = build_plan(x, &h, "s".into(), vec![]);
    let lulu = sex_update(&plan, "lulu").unwrap();
    assert_eq!((lulu.value.as_str(), lulu.selected), ("female_spayed", false));
    let kipper = sex_update(&plan, "kipper").unwrap();
    assert_eq!((kipper.value.as_str(), kipper.selected), ("male_neutered", true));
    let rocco = sex_update(&plan, "rocco").unwrap();
    assert_eq!((rocco.value.as_str(), rocco.selected), ("male", true), "unknown is as good as blank");
    assert!(sex_update(&plan, "pedro").is_none(), "an unreadable sex is dropped");

    // The model saw the surgery but proposed nothing: the server proposes it,
    // from a billed line, ticked. Already-sterilised pets get nothing.
    let h2 = Household {
        pets: vec![a_pet("lulu", "Lulu", Some("female")), a_pet("pedro", "Pedro", Some("male_neutered"))],
        ..Default::default()
    };
    let x: ImportExtraction = serde_json::from_value(json!({
        "records": [
            { "pet_id": "lulu", "occurred_on": "2026-09-21", "title": "Surgery", "category": "Appointments",
              "line_items": [ { "description": "Ovariohysterectomy 20-40 lb", "amount": 350.0 } ] },
            { "pet_id": "pedro", "occurred_on": "2026-09-21", "title": "Neuter recheck", "category": "Appointments" }
        ]
    })).unwrap();
    let plan = build_plan(x.clone(), &h2, "s".into(), vec![]);
    let lulu = sex_update(&plan, "lulu").unwrap();
    assert_eq!((lulu.value.as_str(), lulu.current.as_deref(), lulu.selected), ("female_spayed", Some("female"), true));
    assert!(lulu.reason.unwrap().contains("spay"));
    assert!(sex_update(&plan, "pedro").is_none());

    // A model proposal that contradicts the procedure record is replaced by
    // what the record shows, ticked, rather than blocking it.
    let mut x2 = x;
    x2.pet_updates = serde_json::from_value(json!([ { "pet_id": "lulu", "field": "sex", "value": "male_neutered", "reason": "header" } ])).unwrap();
    let plan = build_plan(x2, &h2, "s".into(), vec![]);
    let lulu = sex_update(&plan, "lulu").unwrap();
    assert_eq!((lulu.value.as_str(), lulu.selected), ("female_spayed", true));
    assert_eq!(lulu.reason.as_deref(), Some("This document shows Lulu's spay."));
    assert_eq!(plan.pet_updates.iter().filter(|u| u.field == "sex").count(), 1, "one sex row per pet");

    // A "Spay estimate" — title says not performed — is no evidence at all.
    let x3: ImportExtraction = serde_json::from_value(json!({
        "records": [ { "pet_id": "lulu", "occurred_on": "2026-08-01", "title": "Spay estimate", "category": "Appointments",
                       "line_items": [ { "description": "OHE 20-40 lb", "amount": 350.0 } ] } ]
    })).unwrap();
    assert!(sex_update(&build_plan(x3, &h2, "s".into(), vec![]), "lulu").is_none());
}

#[test]
fn the_import_schema_offers_the_sex_values() {
    let schema = import_tool_schema(&["lulu".into()], &[]);
    let item = &schema["properties"]["pet_updates"]["items"]["properties"];
    assert!(item["value"]["description"].as_str().unwrap().contains("female_spayed"));
    assert_eq!(item["sterilisation_performed"]["type"], "boolean");
}

#[tokio::test]
async fn weights_can_be_corrected_and_deleted() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(&app, "POST", "/pets", Some(json!({ "name": "Pedro", "species": "Dog" }))).await;
    send(&app, "POST", "/pets", Some(json!({ "name": "Lulu", "species": "Dog" }))).await;
    let (s, w) = send(&app, "POST", "/pets/pedro/weights", Some(json!({ "recorded_on": "2026-09-04", "value": 44.8, "unit": "lb" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    let wid = w["id"].as_i64().unwrap();

    // Creation validates now: real dates, positive values, lb/kg.
    for bad in [
        json!({ "recorded_on": "2026-02-30", "value": 1.0, "unit": "lb" }),
        json!({ "recorded_on": "2026-02-01", "value": 0.0, "unit": "lb" }),
        json!({ "recorded_on": "2026-02-01", "value": 1.0, "unit": "stone" }),
    ] {
        let (s, _) = send(&app, "POST", "/pets/pedro/weights", Some(bad)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    let (s, w) = send(&app, "PATCH", &format!("/pets/pedro/weights/{wid}"), Some(json!({ "value": 48.0 }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(w["value"], 48.0);
    assert_eq!(w["recorded_on"], "2026-09-04", "omitted keys kept");
    let (s, w) = send(&app, "PATCH", &format!("/pets/pedro/weights/{wid}"), Some(json!({ "recorded_on": "2026-09-05", "unit": "kg" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!((w["recorded_on"].as_str(), w["unit"].as_str()), (Some("2026-09-05"), Some("kg")));
    let (s, _) = send(&app, "PATCH", &format!("/pets/pedro/weights/{wid}"), Some(json!({ "recorded_on": "" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&app, "PATCH", &format!("/pets/pedro/weights/{wid}"), Some(json!({ "value": -1.0 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Another pet's URL can't touch it.
    let (s, _) = send(&app, "PATCH", &format!("/pets/lulu/weights/{wid}"), Some(json!({ "value": 1.0 }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(&app, "DELETE", &format!("/pets/lulu/weights/{wid}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(&app, "DELETE", &format!("/pets/pedro/weights/{wid}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = send(&app, "GET", "/pets/pedro/weights", None).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn record_dates_are_validated() {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(&app, "POST", "/pets", Some(json!({ "name": "Lulu", "species": "Dog" }))).await;
    let (s, _) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-02-31", "title": "x" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-02-01", "ends_on": "soon", "title": "x" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, r) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-02-01", "ends_on": "", "title": "x" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(r["ends_on"].is_null(), "blank end date stored as NULL");
    let rid = r["id"].as_i64().unwrap();
    let (s, _) = send(&app, "PATCH", &format!("/pets/lulu/records/{rid}"), Some(json!({ "occurred_on": "2026-04-31" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&app, "PATCH", &format!("/pets/lulu/records/{rid}"), Some(json!({ "ends_on": "2026-99-01" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, r) = send(&app, "PATCH", &format!("/pets/lulu/records/{rid}"), Some(json!({ "occurred_on": "2026-03-01", "ends_on": "2026-03-03" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["ends_on"], "2026-03-03");
}

#[tokio::test]
async fn attachments_are_fingerprinted_and_legacy_rows_backfilled() {
    let pool = memory_pool().await;
    let st = AppState::for_test(pool.clone());
    let app = api_router(st.clone());
    send(&app, "POST", "/pets", Some(json!({ "name": "Lulu", "species": "Dog" }))).await;
    let (_, r) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-02-01", "title": "x" }))).await;
    let rid = r["id"].as_i64().unwrap();
    let (_, a) = send(&app, "POST", &format!("/records/{rid}/attachments"), Some(json!({ "filename": "a.pdf", "media_type": "application/pdf", "data_base64": b64("hello") }))).await;
    // sha256("hello")
    let want = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    assert_eq!(a["sha256"], want);
    let (_, b) = send(&app, "POST", &format!("/records/{rid}/attachments"), Some(json!({ "filename": "gone.pdf", "media_type": "application/pdf", "data_base64": b64("bye") }))).await;

    // Simulate pre-0015 rows: one whose file exists, one whose file is missing.
    sqlx::query("UPDATE attachments SET sha256 = NULL").execute(&pool).await.unwrap();
    let gone: String = sqlx::query_scalar("SELECT stored_path FROM attachments WHERE id = ?")
        .bind(b["id"].as_str().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    std::fs::remove_file(st.attachments_dir.join(gone)).unwrap();
    let filled = furfiles::handlers::backfill_attachment_hashes(&st).await.unwrap();
    assert_eq!(filled, 1, "the missing file is skipped, not fatal");
    let (_, list) = send(&app, "GET", &format!("/records/{rid}/attachments"), None).await;
    let shas: Vec<Value> = list.as_array().unwrap().iter().map(|a| a["sha256"].clone()).collect();
    assert!(shas.contains(&json!(want)) && shas.contains(&Value::Null));
}

// ---------- insurance documents ----------

#[tokio::test]
async fn claims_land_on_their_visit_and_policies_replace_the_old_one() {
    let (app, _) = app_with_mock(tool_reply(json!({
        "summary": "Lemonade claims and a renewed policy.",
        // An insurer is never the clinic; with no visit records it is dropped.
        "clinic": { "name": "Lemonade Insurance Company" },
        "claims": [
            // Onto Lulu's saved 9/4 visit.
            { "pet_id": "lulu", "claim_number": "DEMO-CLM-0952", "policy_number": "DEMO-POL-0401",
              "insurer": "Lemonade", "treatment_date": "2026-09-04", "total_paid": 177.12,
              "items": [ { "description": "Heartworm test", "cost": 74.39, "covered": 74.39 },
                         { "description": "Wellness exam", "cost": 73.60, "covered": 0, "note": "limit met" } ] },
            // No saved visit on 3/5: the claim becomes the visit. Its date is NOT bent to 9/4.
            { "pet_id": "pedro", "claim_number": "DEMO-CLM-0260", "treatment_date": "2026-03-05", "total_paid": 172.0,
              "items": [ { "description": "Bordetella vaccine", "cost": 35.0, "covered": 35.0 },
                         { "description": "Heartworm test", "cost": 88.0, "covered": 88.0 } ] },
            { "pet_id": "pedro", "claim_number": "DEMO-CLM-0260", "treatment_date": "2026-03-05", "total_paid": 172.0 },
            { "pet_id": "pedro", "claim_number": "bad", "treatment_date": "2026-13-01", "total_paid": 1.0 },
            { "pet_id": "pedro", "claim_number": "nopay", "treatment_date": "2026-03-01" }
        ],
        // A model that also wrote a record for the claimed visit would double it.
        "records": [ { "pet_id": "lulu", "occurred_on": "2026-09-04", "title": "Claim breakdown" } ],
        "policies": [
            { "pet_id": "lulu", "kind": "insurance", "provider": "Lemonade Insurance Company", "policy_number": "DEMO-POL-0778",
              "start_date": "2026-09-14", "end_date": "2027-09-14", "premium": 750.33, "premium_interval": "year",
              "reimburse_percent": 90, "deductible": 500, "annual_limit": 50000,
              "coverages": [ { "label": "Wellness exam", "detail": "Up to $70/yr" } ] },
            { "pet_id": "lulu", "kind": "insurance", "provider": "Lemonade Insurance Company", "policy_number": "demo-pol-0401" },
            { "pet_id": "lulu", "provider": "  " }
        ]
    })))
    .await;
    household(&app).await;
    let (_, old) = send(&app, "POST", "/pets/lulu/insurance", Some(json!({
        "kind": "insurance", "provider": "Lemonade Insurance Company", "policy_number": "DEMO-POL-0401",
        "start_date": "2026-02-10", "end_date": "2027-02-10" }))).await;
    let old_id = old["id"].as_str().unwrap().to_string();

    let pdf = b64("CLAIMS");
    let (s, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "filename": "claims.pdf", "media_type": "application/pdf", "data_base64": pdf }))).await;
    assert_eq!(s, StatusCode::OK, "{plan}");
    assert!(plan["clinic"].is_null() && plan["care_team"].as_array().unwrap().is_empty(), "the insurer is not a vet");
    assert!(plan["records"].as_array().unwrap().is_empty(), "the record for a claimed visit is dropped");
    let claims = plan["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 2, "duplicate, undated and unpaid claims dropped: {claims:?}");
    assert_eq!(claims[0]["record_id"], 1, "matched to Lulu's 9/4 visit");
    assert_eq!(claims[0]["paid_cents"], 17712);
    assert_eq!(claims[0]["items"][1]["covered_cents"], 0);
    assert!(claims[1]["record_id"].is_null(), "no 3/5 visit saved");
    assert_eq!(claims[1]["treatment_date"], "2026-03-05");
    let pols = plan["policies"].as_array().unwrap();
    assert_eq!(pols.len(), 2);
    assert_eq!(pols[0]["selected"], true);
    assert_eq!(pols[0]["premium_cents"], 75033);
    assert_eq!(pols[1]["existing_id"], old_id.as_str(), "same number, any case → already saved");
    assert_eq!(pols[1]["selected"], false);
    let ends = plan["policy_ends"].as_array().unwrap();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0]["policy_id"], old_id.as_str());
    assert_eq!(ends[0]["end_date"], "2026-09-14");

    let commit = json!({ "filename": "claims.pdf", "media_type": "application/pdf", "data_base64": pdf, "plan": plan });
    let (s, res) = send(&app, "POST", "/import/commit", Some(commit.clone())).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!((res["claims_recorded"].as_u64(), res["policies_added"].as_u64(), res["policies_ended"].as_u64()), (Some(2), Some(1), Some(1)));

    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    let visit = lulu["records"].as_array().unwrap().iter().find(|r| r["id"] == 1).unwrap().clone();
    assert_eq!(visit["covered_cents"], 17712);
    assert_eq!(visit["extra"]["Insurance claim DEMO-CLM-0952"], "$177.12 paid · policy DEMO-POL-0401");
    assert_eq!(visit["attachments"].as_array().unwrap().len(), 2, "the claim is filed with the visit");
    let new_pol = lulu["insurance_policies"].as_array().unwrap().iter().find(|p| p["policy_number"] == "DEMO-POL-0778").unwrap().clone();
    assert_eq!(new_pol["document_name"], "claims.pdf");
    assert_eq!(new_pol["reimburse_percent"], 90);
    let old_pol = lulu["insurance_policies"].as_array().unwrap().iter().find(|p| p["id"] == old_id.as_str()).unwrap().clone();
    assert_eq!((old_pol["status"].as_str(), old_pol["end_date"].as_str()), (Some("expired"), Some("2026-09-14")));
    let (_, pedro) = send(&app, "GET", "/pets/pedro", None).await;
    let march = &pedro["records"][0];
    assert_eq!((march["occurred_on"].as_str(), march["title"].as_str()), (Some("2026-03-05"), Some("Bordetella vaccine, Heartworm test")));
    assert_eq!((march["cost_cents"].as_i64(), march["covered_cents"].as_i64()), (Some(12300), Some(17200)));
    assert!(march["notes"].as_str().unwrap().contains("Heartworm test: $88.00 billed, $88.00 covered"));

    // Re-analysing marks the claims recorded; re-committing the old plan changes nothing.
    let (_, again) = send(&app, "POST", "/import/analyze", Some(json!({ "filename": "c.pdf", "media_type": "application/pdf", "data_base64": b64("CLAIMS-RESCAN") }))).await;
    assert!(again["claims"].as_array().unwrap().iter().all(|c| c["already_recorded"] == true && c["selected"] == false));
    let (s, res) = send(&app, "POST", "/import/commit", Some(commit)).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    assert_eq!((res["claims_recorded"].as_u64(), res["policies_added"].as_u64()), (Some(0), Some(0)));
    assert_eq!(res["skipped_existing"], 3, "two claims + the policy");
    assert!(res["records"].as_array().unwrap().is_empty());
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    let visit = lulu["records"].as_array().unwrap().iter().find(|r| r["id"] == 1).unwrap().clone();
    assert_eq!(visit["covered_cents"], 17712, "a claim is never counted twice");
}

// ---------- the outside review's findings, each pinned ----------

#[tokio::test]
async fn review_findings_stay_fixed() {
    let (app, _) = app_with_mock(tool_reply(json!({
        "clinic": { "name": "AMC of Springfield" },
        "records": [
            // Pedro's visit on the invoice Lulu's record already carries: still offered.
            { "pet_id": "pedro", "occurred_on": "2026-09-04", "title": "Exam + vaccines", "cost": 189.48 },
            // Model points at Lulu's 9/4 record from a DIFFERENT date: not a duplicate.
            { "pet_id": "lulu", "occurred_on": "2026-09-12", "title": "Panacur", "duplicate_of_record_id": 1 },
            // Same pet, date and title (no cost): a rescan of the saved visit.
            { "pet_id": "lulu", "occurred_on": "2026-09-04", "title": "comprehensive EXAM" }
        ],
        "reminders": [ { "pet_id": "lulu", "due_on": "2027-03-05", "title": "DHPP - 1 year" } ],
        // Pedro's document can't complete Lulu's reminder… and Lulu has a record here,
        // so this one is allowed (SNAP due 11/5, 4Dx on 9/12 within 90 days).
        "completed_reminders": [ { "reminder_id": 2 } ],
        "regimens": [
            { "pet_id": "kipper", "kind": "food", "product": "Hill's Science Diet 7+", "amount": "1/2 cup", "frequency": "2x/day" },
            { "pet_id": "kipper", "kind": "food", "product": "hills science diet 7+", "amount": "1 cup" }
        ]
    })))
    .await;
    let (_, lulu_rec) = household(&app).await;
    let file = b64("INVOICE-260743"); // already attached to Lulu's record
    let (_, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "filename": "inv.pdf", "media_type": "application/pdf", "data_base64": file }))).await;
    let recs = plan["records"].as_array().unwrap();
    assert_eq!(recs[0]["selected"], true, "the file being Lulu's doesn't hide Pedro's visit");
    assert!(recs[1]["duplicate_of"].is_null(), "a model dup on another date is ignored");
    assert_eq!(recs[2]["duplicate_of"]["record_id"], lulu_rec);
    assert!(recs[2]["duplicate_of"]["reason"].as_str().unwrap().contains("title"));
    assert_eq!(plan["reminders"][0]["same_day"], "Lepto - Annual", "the likely twin is shown");
    // Lulu's saved 9/4 visit (no clinic) is a duplicate here: offer to link it.
    assert_eq!(plan["clinic_links"].as_array().unwrap().len(), 1);
    assert_eq!(plan["clinic_links"][0]["record_id"], lulu_rec);
    assert_eq!(plan["completed_reminders"].as_array().unwrap().len(), 1);
    let regs = plan["regimens"].as_array().unwrap();
    assert_eq!(regs.len(), 1, "same product twice in one document → once");
    assert!(regs[0]["duplicate_of"].is_null() && regs[0]["replaces"].is_string(), "a dose change replaces");

    // A Pedro-only document can't complete Lulu's reminder (an injected or
    // confused id), even though the date is in range.
    let (pedro_app, _) = app_with_mock(tool_reply(json!({
        "records": [ { "pet_id": "pedro", "occurred_on": "2026-09-04", "title": "Exam" } ],
        "completed_reminders": [ { "reminder_id": 2 } ]
    })))
    .await;
    household(&pedro_app).await;
    let (_, p2) = send(&pedro_app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": b64("P") }))).await;
    assert!(p2["completed_reminders"].as_array().unwrap().is_empty(), "{p2}");

    let commit = |plan: &Value, data: &str| json!({ "filename": "inv.pdf", "media_type": "application/pdf", "data_base64": data, "plan": plan });

    // A plan with a different file is refused.
    let (s, e) = send(&app, "POST", "/import/commit", Some(commit(&plan, &b64("OTHER")))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(e["error"].as_str().unwrap().contains("different file"));

    // A bad weight unit is refused BEFORE anything is written.
    let mut bad = plan.clone();
    bad["weights"] = json!([{ "selected": true, "pet_id": "pedro", "recorded_on": "2026-09-04", "value": 48.0, "unit": "stone" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(&bad, &file))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, pedro) = send(&app, "GET", "/pets/pedro", None).await;
    assert!(pedro["records"].as_array().unwrap().is_empty(), "nothing half-written");

    // Stale plans: a weight or profile field changed since analysis.
    let mut stale = plan.clone();
    stale["weights"] = json!([{ "selected": true, "pet_id": "lulu", "recorded_on": "2026-09-11", "value": 55.4, "unit": "lb", "existing_id": 1, "existing_value": 50.0 }]);
    let (s, e) = send(&app, "POST", "/import/commit", Some(commit(&stale, &file))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(e["error"].as_str().unwrap().contains("changed since"));
    send(&app, "PATCH", "/pets/kipper", Some(json!({ "microchip": "111" }))).await;
    let mut stale = plan.clone();
    stale["pet_updates"] = json!([{ "selected": true, "pet_id": "kipper", "field": "microchip", "current": null, "value": "982" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(&stale, &file))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a microchip entered after analysis isn't overwritten");

    // Retrying a commit adds no second weight or care entry; the dose change
    // ends the old food entry exactly once.
    let mut ok = plan.clone();
    ok["weights"] = json!([{ "selected": true, "pet_id": "pedro", "recorded_on": "2026-09-04", "value": 48.0, "unit": "lb" }]);
    let (s, r1) = send(&app, "POST", "/import/commit", Some(commit(&ok, &file))).await;
    assert_eq!(s, StatusCode::CREATED, "{r1}");
    assert_eq!(r1["clinic_links"], 1);
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    let visit = lulu["records"].as_array().unwrap().iter().find(|r| r["id"] == lulu_rec).unwrap().clone();
    assert_eq!(visit["contact"]["name"], "AMC of Springfield", "the old visit now knows its clinic");
    assert_eq!((r1["regimens_added"].as_u64(), r1["regimens_ended"].as_u64()), (Some(1), Some(1)));
    let (s, r2) = send(&app, "POST", "/import/commit", Some(commit(&ok, &file))).await;
    assert_eq!(s, StatusCode::CREATED, "{r2}");
    assert_eq!(r2["weights_added"], 0);
    assert_eq!(r2["clinic_links"], 0, "already linked; never re-pointed");
    assert_eq!(r2["regimens_added"], 0);
    assert!(r2["skipped_existing"].as_u64().unwrap() >= 2);
    let (_, kipper) = send(&app, "GET", "/pets/kipper", None).await;
    let current: Vec<&Value> = kipper["regimens"].as_array().unwrap().iter().filter(|r| r["ended_on"].is_null()).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["amount"], "1/2 cup");
    let (_, pedro) = send(&app, "GET", "/pets/pedro", None).await;
    assert_eq!(pedro["weights"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_renewal_that_keeps_its_policy_number_is_a_new_term() {
    // Insurance value (#74) prices each policy row as one term, so a renewal
    // under the same number must not be mistaken for the saved term.
    let (app, _) = app_with_mock(tool_reply(json!({
        "policies": [ { "pet_id": "lulu", "kind": "insurance", "provider": "Lemonade", "policy_number": "SAME-1",
                        "start_date": "2026-09-14", "end_date": "2027-09-14" } ]
    })))
    .await;
    household(&app).await;
    send(&app, "POST", "/pets/lulu/insurance", Some(json!({
        "kind": "insurance", "provider": "Lemonade", "policy_number": "SAME-1", "status": "expired",
        "start_date": "2025-09-14", "end_date": "2026-09-14" }))).await;
    let data = b64("RENEWAL");
    let (_, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": data }))).await;
    assert_eq!(plan["policies"][0]["selected"], true, "a new term, not the saved one: {plan}");
    let commit = json!({ "filename": "r.pdf", "media_type": "application/pdf", "data_base64": data, "plan": plan });
    let (s, r) = send(&app, "POST", "/import/commit", Some(commit.clone())).await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
    assert_eq!(r["policies_added"], 1);
    // The same renewal again IS the saved term now: nothing added twice.
    let (_, again) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": data }))).await;
    assert_eq!(again["policies"][0]["selected"], false);
    let (_, r2) = send(&app, "POST", "/import/commit", Some(commit)).await;
    assert_eq!(r2["policies_added"], 0);
}

#[tokio::test]
async fn an_old_policy_is_only_ended_alongside_its_same_pet_replacement() {
    let (app, _) = app_with_mock(tool_reply(json!({
        "policies": [ { "pet_id": "lulu", "kind": "insurance", "provider": "Lemonade", "policy_number": "NEW-1", "start_date": "2026-09-14" } ]
    })))
    .await;
    household(&app).await;
    let (_, old) = send(&app, "POST", "/pets/lulu/insurance", Some(json!({
        "kind": "insurance", "provider": "Lemonade", "policy_number": "OLD-1", "start_date": "2026-02-10", "end_date": "2027-02-10" }))).await;
    let data = b64("POLICY");
    let (_, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": data }))).await;
    assert_eq!(plan["policy_ends"][0]["policy_id"], old["id"]);
    let commit = |plan: &Value| json!({ "filename": "p.pdf", "media_type": "application/pdf", "data_base64": data, "plan": plan });
    let old_status = |pets: Value| {
        pets["insurance_policies"].as_array().unwrap().iter().find(|p| p["policy_number"] == "OLD-1").unwrap()["status"].clone()
    };

    // The replacement unticked: the end is left alone, the save still succeeds.
    let mut untick = plan.clone();
    untick["policies"][0]["selected"] = json!(false);
    let (s, r) = send(&app, "POST", "/import/commit", Some(commit(&untick))).await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
    assert_eq!(r["policies_ended"], 0);
    // Re-assigned to Pedro: Lulu's policy is NOT ended.
    let mut moved = plan.clone();
    moved["policies"][0]["pet_id"] = json!("pedro");
    let (s, r) = send(&app, "POST", "/import/commit", Some(commit(&moved))).await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
    assert_eq!((r["policies_added"].as_u64(), r["policies_ended"].as_u64()), (Some(1), Some(0)));
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert_eq!(old_status(lulu), "active");

    // Unassigned policy: an end is offered for the matching pet, unticked.
    let (app2, _) = app_with_mock(tool_reply(json!({
        "policies": [ { "kind": "insurance", "provider": "Lemonade", "policy_number": "NEW-2", "start_date": "2026-09-14" } ]
    })))
    .await;
    household(&app2).await;
    send(&app2, "POST", "/pets/pedro/insurance", Some(json!({ "kind": "insurance", "provider": "Lemonade", "policy_number": "OLD-2", "start_date": "2026-02-10" }))).await;
    let (_, p2) = send(&app2, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": b64("P2") }))).await;
    assert_eq!(p2["policy_ends"][0]["pet_id"], "pedro");
    assert_eq!(p2["policy_ends"][0]["selected"], false, "ticked once the policy is assigned to Pedro");
}

#[tokio::test]
async fn a_record_whose_file_cannot_be_stored_is_undone() {
    let (base, _) = mock_anthropic(tool_reply(json!({}))).await;
    let mut st = AppState::for_test(memory_pool().await);
    st.anthropic_base = base;
    // Make the attachments dir unusable: a FILE where the directory should be.
    std::fs::create_dir_all(st.attachments_dir.parent().unwrap()).unwrap();
    std::fs::write(&st.attachments_dir, b"not a dir").unwrap();
    let app = api_router(st);
    send(&app, "POST", "/pets", Some(json!({ "name": "Lulu", "species": "Dog" }))).await;
    let data = b64("DOC");
    let plan = json!({ "sha256": "c9ff9d7d29e892ae070c1d1c994458bff733dfad5f006865c3c7065193f789db",
        "records": [ { "selected": true, "pet_id": "lulu", "occurred_on": "2026-01-02", "title": "Visit" } ] });
    let (s, _) = send(&app, "POST", "/import/commit", Some(json!({ "filename": "d.pdf", "media_type": "application/pdf", "data_base64": data, "plan": plan }))).await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert!(lulu["records"].as_array().unwrap().is_empty(), "no orphan visit left for a retry to duplicate");
}


// ---------- the second outside review's findings, each pinned ----------

#[tokio::test]
async fn second_review_findings_stay_fixed() {
    let (app, _) = app_with_mock(tool_reply(json!({
        "summary": "mixed",
        "clinic": { "name": "AMC of Springfield" },
        "records": [
            // A distinct same-day visit next to a claim for that day: kept.
            { "pet_id": "lulu", "occurred_on": "2026-09-20", "title": "Spay consult", "cost": 95.0 },
            // Same day + title as a saved visit but a DIFFERENT total: a second visit.
            { "pet_id": "lulu", "occurred_on": "2026-09-04", "title": "Comprehensive exam", "cost": 12.0 }
        ],
        "reminders": [ { "pet_id": "lulu", "due_on": "2027-03-05", "title": "DA2PP booster" } ],
        "claims": [
            { "pet_id": "lulu", "claim_number": "lpc-1", "treatment_date": "2026-09-20", "total_paid": 50.0,
              "items": [ { "description": "Exam", "cost": 60.0, "covered": 50.0 } ] },
            // Same number, other pet: both kept.
            { "pet_id": "pedro", "claim_number": "LPC-1", "treatment_date": "2026-09-20", "total_paid": 10.0 },
            // Same number again for Lulu, different case: one claim.
            { "pet_id": "lulu", "claim_number": "LPC-1", "treatment_date": "2026-09-20", "total_paid": 50.0 }
        ],
        "regimens": [
            // Model points "Cytopoint" at the saved Hill's id: ignored, it's a different product.
            { "pet_id": "kipper", "kind": "medication", "product": "Cytopoint", "duplicate_of_regimen_id": "WILL-BE-REPLACED" },
            { "pet_id": "kipper", "kind": "medication", "product": "Clavamox", "amount": "1 tab", "ended_on": "2025-02-01" }
        ],
        "policies": [ { "pet_id": "lulu", "kind": "wellness", "provider": "Oak Hollow", "start_date": "2026-01-10" } ]
    })))
    .await;
    household(&app).await;
    // Two saved same-day visits for the claim to choose between.
    let (_, a) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-09-20", "title": "Nail trim", "cost_cents": 2000 }))).await;
    let (_, b) = send(&app, "POST", "/pets/lulu/records", Some(json!({ "occurred_on": "2026-09-20", "title": "Exam", "cost_cents": 6200 }))).await;
    let (_, food) = send(&app, "GET", "/pets/kipper", None).await;
    let food_id = food["regimens"][0]["id"].as_str().unwrap().to_string();

    let data = b64("MIXED");
    let (s, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": data }))).await;
    assert_eq!(s, StatusCode::OK, "{plan}");
    let recs = plan["records"].as_array().unwrap();
    assert!(recs.iter().any(|r| r["title"] == "Spay consult"), "a distinct same-day visit survives a claim");
    let second = recs.iter().find(|r| r["occurred_on"] == "2026-09-04").unwrap();
    assert!(second["duplicate_of"].is_null(), "same title, different total: not a duplicate");
    assert_eq!(plan["reminders"][0]["selected"], false, "a same-day twin starts unticked");
    let claims = plan["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 2, "per pet; case-insensitive within a pet: {claims:?}");
    assert_eq!(claims[0]["record_id"], b["id"], "the same-day visit whose total is closest to the bill");
    assert_eq!(claims[0]["visit_options"].as_array().unwrap().len(), 2, "both visits offered to correct the guess");
    assert!(claims[1]["record_id"].is_null(), "Pedro has no visit that day");
    let _ = a;
    assert_eq!(plan["regimens"][0]["duplicate_of"], Value::Null);
    assert_ne!(plan["regimens"][0]["replaces"], json!(food_id), "never ends a different product");

    let commit = |plan: &Value, mt: &str| json!({ "filename": "m.pdf", "media_type": mt, "data_base64": data, "plan": plan });

    // Wrong type, empty fingerprint, ending a policy without its replacement:
    // all refused BEFORE anything is written.
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(&plan, "text/plain"))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut no_sha = plan.clone();
    no_sha["sha256"] = json!("");
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(&no_sha, "application/pdf"))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A reminder deleted since analysis is stale, not a mid-commit 404.
    let mut gone = plan.clone();
    gone["completed_reminders"] = json!([{ "selected": true, "reminder_id": 999, "pet_id": "lulu" }]);
    let (s, e) = send(&app, "POST", "/import/commit", Some(commit(&gone, "application/pdf"))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    // A weight whose unit changed since analysis is stale.
    let mut unit = plan.clone();
    unit["weights"] = json!([{ "selected": true, "pet_id": "lulu", "recorded_on": "2026-09-11", "value": 55.4, "unit": "lb", "existing_id": 1, "existing_value": 55.8, "existing_unit": "kg" }]);
    let (s, _) = send(&app, "POST", "/import/commit", Some(commit(&unit, "application/pdf"))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert_eq!(lulu["records"].as_array().unwrap().len(), 3, "nothing half-written by the refusals");

    // The real commit, then a retry: no second unnumbered policy, no second
    // ended course, the claim counted once.
    let (s, r1) = send(&app, "POST", "/import/commit", Some(commit(&plan, "application/pdf"))).await;
    assert_eq!(s, StatusCode::CREATED, "{r1}");
    let (s, r2) = send(&app, "POST", "/import/commit", Some(commit(&plan, "application/pdf"))).await;
    assert_eq!(s, StatusCode::CREATED, "{r2}");
    assert_eq!((r2["policies_added"].as_u64(), r2["regimens_added"].as_u64(), r2["claims_recorded"].as_u64()), (Some(0), Some(0), Some(0)));
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    assert_eq!(lulu["insurance_policies"].as_array().unwrap().len(), 1);
    let exam = lulu["records"].as_array().unwrap().iter().find(|r| r["id"] == b["id"]).unwrap().clone();
    assert_eq!(exam["covered_cents"], 5000);
    assert!(exam["extra"].as_object().unwrap().contains_key("Insurance claim LPC-1"), "the key is case-normalised");
    let (_, kipper) = send(&app, "GET", "/pets/kipper", None).await;
    assert_eq!(kipper["regimens"].as_array().unwrap().iter().filter(|r| r["product"] == "Clavamox").count(), 1);
    assert!(kipper["regimens"].as_array().unwrap().iter().any(|r| r["id"] == food_id.as_str() && r["ended_on"].is_null()), "Hill's untouched");
}

#[tokio::test]
async fn mixed_case_claim_keys_and_edited_reminders_are_caught() {
    let (app, _) = app_with_mock(tool_reply(json!({
        "records": [ { "pet_id": "lulu", "occurred_on": "2026-09-12", "title": "Panacur" } ],
        "claims": [ { "pet_id": "lulu", "claim_number": "LPC-9", "treatment_date": "2026-09-04", "total_paid": 5.0 } ],
        "completed_reminders": [ { "reminder_id": 2 } ]
    })))
    .await;
    let (_, rid) = household(&app).await;
    // A claim recorded earlier under a lower-case key is the same claim.
    send(&app, "PATCH", &format!("/pets/lulu/records/{rid}"), Some(json!({ "extra": { "Insurance claim lpc-9": "$5.00 paid" } }))).await;
    let data = b64("EOB9");
    let (_, plan) = send(&app, "POST", "/import/analyze", Some(json!({ "media_type": "application/pdf", "data_base64": data }))).await;
    assert_eq!(plan["claims"][0]["already_recorded"], true, "{plan}");
    let mut forced = plan.clone();
    forced["claims"][0]["selected"] = json!(true);
    // Reminder 2 is edited into another task after analysis: stale, not "done".
    send(&app, "PATCH", "/reminders/2", Some(json!({ "title": "Dental cleaning" }))).await;
    let commit = |plan: &Value| json!({ "filename": "e.pdf", "media_type": "application/pdf", "data_base64": data, "plan": plan });
    let (s, e) = send(&app, "POST", "/import/commit", Some(commit(&forced))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    assert!(e["error"].as_str().unwrap().contains("reminder"), "{e}");
    // Without the stale reminder, a forced re-record of the claim is skipped, not double-paid.
    forced["completed_reminders"] = json!([]);
    let (s, r) = send(&app, "POST", "/import/commit", Some(commit(&forced))).await;
    assert_eq!(s, StatusCode::CREATED, "{r}");
    assert_eq!(r["claims_recorded"], 0);
    let (_, lulu) = send(&app, "GET", "/pets/lulu", None).await;
    let visit = lulu["records"].as_array().unwrap().iter().find(|r| r["id"] == rid).unwrap().clone();
    assert!(visit["covered_cents"].is_null(), "never paid twice");
}
