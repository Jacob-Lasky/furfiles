//! Regression for #87: every AI read failed with
//! `400 — tool_choice: type "tool" and "any" are not supported for this model.`
//! once the household picked Claude Sonnet 5.5, because each request FORCED its
//! tool. The mock below rejects a forced `tool_choice` exactly the way Sonnet 5.5
//! does, so these tests fail on the old request shape and pass only when the
//! request asks for the tool with `auto`. They also pin the retry `auto` needs:
//! a model may answer in prose instead of calling the tool.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

mod common;
use common::send;

use furfiles::import::{build_import_request, Household};
use furfiles::ingestion::{
    build_facility_request, build_insurance_request, build_request, missed_tool_call,
};
use furfiles::{api_router, memory_pool, AppState};

/// The provider's own words for a forced tool choice on Sonnet 5.5 / Opus 5.5 /
/// Fable 5.1.
const FORCED_400: &str = "tool_choice: type \"tool\" and \"any\" are not supported for this model.";

/// A Messages API that behaves like Claude Sonnet 5.5 on `tool_choice`: a forced
/// choice is a 400; anything else gets the next scripted reply (the last one
/// repeats). Records every request body.
async fn sonnet_5_5_mock(replies: Vec<Value>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen2 = seen.clone();
    let app = Router::new().route(
        "/v1/messages",
        post(move |Json(body): Json<Value>| {
            let seen = seen2.clone();
            let replies = replies.clone();
            async move {
                let mut log = seen.lock().unwrap();
                log.push(body.clone());
                let kind = body["tool_choice"]["type"].as_str().unwrap_or("auto");
                if kind == "tool" || kind == "any" {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "type": "error",
                            "error": { "type": "invalid_request_error", "message": FORCED_400 }
                        })),
                    );
                }
                let i = (log.len() - 1).min(replies.len() - 1);
                (StatusCode::OK, Json(replies[i].clone()))
            }
        }),
    );
    (common::serve(app).await, seen)
}

/// What Sonnet 5.5 sends back: a (here empty) thinking block, then the call.
fn tool_reply(tool: &str, input: Value) -> Value {
    json!({
        "stop_reason": "tool_use",
        "content": [
            { "type": "thinking", "thinking": "", "signature": "sig" },
            { "type": "tool_use", "id": "toolu_1", "name": tool, "input": input }
        ]
    })
}

/// `auto` lets the model answer in prose instead.
fn prose_reply(stop: &str) -> Value {
    json!({ "stop_reason": stop, "content": [{ "type": "text", "text": "Here is what I found..." }] })
}

async fn app_with(replies: Vec<Value>) -> (Router, Arc<Mutex<Vec<Value>>>) {
    let (base, seen) = sonnet_5_5_mock(replies).await;
    let mut st = AppState::for_test(memory_pool().await);
    st.anthropic_base = base;
    let app = api_router(st);
    let (s, _) = send(
        &app,
        "PUT",
        "/settings/ingestion",
        Some(json!({ "api_key": "sk-ant-test", "enabled": true, "model": "claude-sonnet-5-5" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    (app, seen)
}

fn pdf() -> Value {
    // "%PDF-1.4" base64: check_document only bounds the size and decodes it.
    json!({ "filename": "invoice.pdf", "media_type": "application/pdf", "data_base64": "JVBERi0xLjQ=" })
}

/// The request contract: the tool is offered, the choice is `auto` (at most one
/// call), and the last thing the model reads names the tool it must call.
fn assert_asks_for(req: &Value, tool: &str) {
    assert_eq!(req["model"], "claude-sonnet-5-5");
    assert_eq!(req["tools"][0]["name"], tool);
    assert_eq!(
        req["tool_choice"],
        json!({ "type": "auto", "disable_parallel_tool_use": true })
    );
    let content = req["messages"][0]["content"].as_array().unwrap();
    let last = content.last().unwrap()["text"].as_str().unwrap();
    assert!(
        last.contains(&format!("`{tool}`")),
        "closing instruction names the tool: {last}"
    );
}

#[tokio::test]
async fn record_scan_works_on_a_model_that_rejects_forced_tool_choice() {
    let (app, seen) = app_with(vec![tool_reply(
        "record_fields",
        json!({ "title": "Annual exam", "occurred_on": "2026-09-04", "cost": 120.5 }),
    )])
    .await;
    let (s, body) = send(&app, "POST", "/extract", Some(pdf())).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["fields"]["title"], "Annual exam");
    assert_eq!(body["model"], "claude-sonnet-5-5");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_asks_for(&seen[0], "record_fields");
}

#[tokio::test]
async fn scan_preview_sends_exactly_what_extract_sends() {
    let (app, seen) = app_with(vec![tool_reply(
        "record_fields",
        json!({ "title": "Exam" }),
    )])
    .await;
    let (s, _) = send(&app, "POST", "/extract", Some(pdf())).await;
    assert_eq!(s, StatusCode::OK);
    let (s, body) = send(&app, "POST", "/extract/preview", Some(pdf())).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(
        body["content"][1]["name"], "record_fields",
        "preview returns the raw response"
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0], seen[1],
        "preview must debug the request /extract really sends"
    );
    assert_asks_for(&seen[1], "record_fields");
}

/// A refusal or a `max_tokens` cut-off can still carry a tool call whose input is
/// empty or truncated. Read as fields, it pre-fills a blank form with a 200; it
/// must be an error naming what happened, and it is not retried.
#[tokio::test]
async fn a_refused_or_cut_off_tool_call_is_an_error_not_a_blank_form() {
    for (stop, says) in [("refusal", "declined"), ("max_tokens", "too long")] {
        for (uri, tool) in [
            ("/extract", "record_fields"),
            ("/insurance/extract", "insurance_fields"),
        ] {
            let mut reply = tool_reply(tool, json!({}));
            reply["stop_reason"] = json!(stop);
            let (app, seen) = app_with(vec![reply]).await;
            let (s, body) = send(&app, "POST", uri, Some(pdf())).await;
            assert_eq!(s, StatusCode::BAD_GATEWAY, "{uri} {stop}: {body}");
            assert!(
                body["error"].as_str().unwrap().contains(says),
                "{uri} {stop}: {body}"
            );
            assert_eq!(seen.lock().unwrap().len(), 1, "{uri} {stop}: not retried");
        }
    }
    // The website parse shares the parser; no fetchable URL in a test, so call it.
    let mut refused = tool_reply("facility_fields", json!({}));
    refused["stop_reason"] = json!("refusal");
    let e = furfiles::ingestion::parse_facility_response(&refused).unwrap_err();
    assert!(e.contains("declined"), "{e}");
}

#[tokio::test]
async fn insurance_scan_works_on_a_model_that_rejects_forced_tool_choice() {
    let (app, seen) = app_with(vec![tool_reply(
        "insurance_fields",
        json!({ "provider": "Lemonade", "kind": "insurance" }),
    )])
    .await;
    let (s, body) = send(&app, "POST", "/insurance/extract", Some(pdf())).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["fields"]["provider"], "Lemonade");
    assert_asks_for(&seen.lock().unwrap()[0], "insurance_fields");
}

#[tokio::test]
async fn import_works_on_a_model_that_rejects_forced_tool_choice() {
    let (app, seen) = app_with(vec![tool_reply(
        "import_plan",
        json!({
            "summary": "One visit for Lulu.",
            "records": [{ "pet_id": "lulu", "occurred_on": "2026-09-04", "title": "Exam", "category": "Checkup" }]
        }),
    )])
    .await;
    let (s, _) = send(
        &app,
        "POST",
        "/pets",
        Some(json!({ "name": "Lulu", "species": "Dog" })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, plan) = send(&app, "POST", "/import/analyze", Some(pdf())).await;
    assert_eq!(s, StatusCode::OK, "{plan}");
    assert_eq!(plan["records"][0]["title"], "Exam");
    assert_asks_for(&seen.lock().unwrap()[0], "import_plan");
}

#[tokio::test]
async fn a_missed_tool_call_is_retried_once() {
    let (app, seen) = app_with(vec![
        prose_reply("end_turn"),
        tool_reply("record_fields", json!({ "title": "Dental cleaning" })),
    ])
    .await;
    let (s, body) = send(&app, "POST", "/extract", Some(pdf())).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["fields"]["title"], "Dental cleaning");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "one retry");
    assert_eq!(seen[0], seen[1], "the retry resends the same request");
}

#[tokio::test]
async fn two_missed_calls_are_a_clean_error_not_a_loop() {
    let (app, seen) = app_with(vec![prose_reply("end_turn")]).await;
    let (s, body) = send(&app, "POST", "/extract", Some(pdf())).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("did not return the record_fields tool call"),
        "{body}"
    );
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_cut_off_or_refused_read_is_not_retried() {
    for stop in ["max_tokens", "refusal"] {
        let (app, seen) = app_with(vec![prose_reply(stop)]).await;
        let (s, _) = send(&app, "POST", "/import/analyze", Some(pdf())).await;
        assert_eq!(s, StatusCode::BAD_GATEWAY, "{stop}");
        assert_eq!(
            seen.lock().unwrap().len(),
            1,
            "{stop}: a retry would only repeat it"
        );
    }
}

#[test]
fn no_request_builder_forces_a_tool_or_sends_a_model_gated_parameter() {
    let h = Household::default();
    let bodies = [
        build_request("m", "application/pdf", "QQ==", &[]),
        build_facility_request("m", "page"),
        build_insurance_request("m", "image/png", "QQ=="),
        build_import_request("m", "application/pdf", "QQ==", &h, "2026-10-17"),
    ];
    for b in bodies {
        let tool = b["tools"][0]["name"].clone();
        assert_eq!(
            b["tool_choice"],
            json!({ "type": "auto", "disable_parallel_tool_use": true }),
            "forced tool_choice 400s on Sonnet 5.5: {tool}"
        );
        // The CLASS of #87: the model is whatever the household picked, so a
        // parameter only some models accept (`thinking`, `output_config`,
        // `temperature`, a forced `tool_choice`) breaks the others. Only these
        // keys are valid on every model the picker offers.
        let mut keys: Vec<&str> = b.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["max_tokens", "messages", "model", "tool_choice", "tools"],
            "{tool}"
        );
        // Sonnet 5.5 thinks before the call and the thinking counts against
        // max_tokens: a 2-page invoice used 2,375 output tokens live (#87), so
        // the old 1,024 cap would cut the read off. Literal, not the constant,
        // so lowering the constant turns this red.
        assert!(
            b["max_tokens"].as_u64().unwrap() >= 8_000,
            "{tool}: {}",
            b["max_tokens"]
        );
    }
}

#[test]
fn missed_tool_call_reads_the_stop_reason() {
    let called = tool_reply("record_fields", json!({}));
    assert!(!missed_tool_call(&called, "record_fields"));
    assert!(
        missed_tool_call(&called, "facility_fields"),
        "a different tool is still a miss"
    );
    assert!(missed_tool_call(&prose_reply("end_turn"), "record_fields"));
    assert!(!missed_tool_call(
        &prose_reply("max_tokens"),
        "record_fields"
    ));
    assert!(!missed_tool_call(&prose_reply("refusal"), "record_fields"));
    assert!(
        missed_tool_call(&json!({}), "record_fields"),
        "no content at all"
    );
}
