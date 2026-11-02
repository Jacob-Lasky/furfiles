//! Bulk import (#23): drop any number of documents in ONE place — no pet picked
//! first — and let the model work out which RECORDS each file holds and which
//! PET each record belongs to. A file is a container, not a record: one invoice
//! covers two dogs, one clinic history report holds a dozen visits, and a
//! vaccination summary is a profile update plus reminders. So analysis returns a
//! PLAN (records, clinic, care-team links, weights, reminders, reminders it
//! shows were done, profile updates, ongoing care), reviewed by a human, then
//! committed in one call.
//!
//! Two endpoints, both API-first so a script can do exactly what the screen does:
//! - `POST /import/analyze` — reads one document, NEVER writes. The model sees a
//!   snapshot of the household (pets, contacts, existing records, open reminders,
//!   weights, current care) so it can assign pets and flag duplicates; the server
//!   then re-checks every id it returned and layers on deterministic duplicate
//!   checks (file fingerprint, same pet + date + total, same pet + date weight,
//!   exact reminder) so a model slip can't create a duplicate on its own.
//!   Each proposed item carries `selected`: the server's recommendation.
//! - `POST /import/commit` — the same plan back (edited or not) plus the file;
//!   writes every `selected` item through the existing handlers, so validation
//!   lives in one place per entity.
//!
//! The model call is the same tool pattern as `ingestion.rs` (`tool_request` +
//! `call_tool`); the pure
//! pieces (request, parse, plan normalisation) are unit-tested, and the handler
//! path is exercised end to end against a mock provider in `tests/import.rs`.
//!
//! Layout: [`types`] (what the model returns, the plan, the household
//! snapshot), [`prompt`] (prompt, tool schema, request, response parsing),
//! [`plan`] (model output → a plan the server vouches for: ids, money and the
//! deterministic duplicate checks), [`commit`] (validation and the writes).
//! This file holds the shared rules and the analyze handler.

mod commit;
mod plan;
mod prompt;
mod types;

pub use commit::*;
pub use plan::*;
pub use prompt::*;
pub use types::*;

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::handlers;
use crate::ingestion::{self, ExtractedLineItem};
use crate::models::*;
use crate::models::RECORD_CATEGORIES as CATEGORIES;
use crate::{sex, AppState};

/// Profile fields an import may propose changing. Identity facts a vet document
/// states authoritatively; never insurance, photo or gradient.
pub const PET_UPDATE_FIELDS: &[&str] = &["name", "breed", "sex", "color", "microchip", "birth_date"];

/// What every claim's custom-field label starts with. Insurance value (#74)
/// reads claims back by this prefix, so it lives here with [`claim_key`].
pub const CLAIM_KEY_PREFIX: &str = "Insurance claim ";

/// The custom-field label a claim is recorded under on its visit. One place, so
/// "is this claim already recorded" and "record this claim" can't drift.
pub fn claim_key(claim_number: &str) -> String {
    // Upper-cased: "DEMO-CLM-0952" and "demo-clm-0952" are one claim, and an
    // exact-case key would count its payout twice.
    format!("{CLAIM_KEY_PREFIX}{}", claim_number.trim().to_uppercase())
}

pub async fn load_household(pool: &SqlitePool) -> AppResult<Household> {
    Ok(Household {
        pets: sqlx::query_as("SELECT * FROM pets ORDER BY name COLLATE NOCASE")
            .fetch_all(pool)
            .await?,
        contacts: sqlx::query_as("SELECT * FROM contacts ORDER BY name COLLATE NOCASE")
            .fetch_all(pool)
            .await?,
        care_team: sqlx::query_as("SELECT pet_id, contact_id FROM pet_contacts")
            .fetch_all(pool)
            .await?,
        records: sqlx::query_as(
            // EVERY record, for the deterministic duplicate checks; only the
            // newest MAX_CONTEXT_RECORDS go into the prompt (household_context).
            "SELECT r.id, r.pet_id, r.occurred_on, r.title, r.cost_cents, c.name AS clinic, r.extra \
             FROM medical_records r LEFT JOIN contacts c ON c.id = r.contact_id \
             ORDER BY r.occurred_on DESC, r.id DESC",
        )
        .fetch_all(pool)
        .await?,
        open_reminders: sqlx::query_as(
            "SELECT * FROM reminders WHERE done = 0 ORDER BY due_on ASC, id ASC",
        )
        .fetch_all(pool)
        .await?,
        weights: sqlx::query_as("SELECT * FROM weight_entries ORDER BY recorded_on ASC, id ASC")
            .fetch_all(pool)
            .await?,
        current_regimens: sqlx::query_as(
            "SELECT * FROM pet_regimens WHERE ended_on IS NULL ORDER BY product COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await?,
        policies: sqlx::query_as("SELECT * FROM insurance_policies ORDER BY start_date DESC")
            .fetch_all(pool)
            .await?,
        known_fields: handlers::known_field_labels(pool).await?,
    })
}

/// Records that already carry a file with these exact bytes.
pub(crate) async fn file_duplicates(pool: &SqlitePool, sha256: &str) -> AppResult<Vec<FileDuplicate>> {
    Ok(sqlx::query_as(
        "SELECT a.id AS attachment_id, r.id AS record_id, r.pet_id, p.name AS pet_name, \
                r.title, r.occurred_on, a.filename \
         FROM attachments a JOIN medical_records r ON r.id = a.record_id \
         JOIN pets p ON p.id = r.pet_id \
         WHERE a.sha256 = ? ORDER BY r.occurred_on DESC, r.id",
    )
    .bind(sha256)
    .fetch_all(pool)
    .await?)
}

pub(crate) fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub async fn analyze(
    State(st): State<AppState>,
    Json(req): Json<ImportAnalyzeRequest>,
) -> AppResult<Json<ImportPlan>> {
    let (cfg, api_key) = handlers::ai_key(
        &st,
        "Importing reads documents with AI. Add your API key in Settings and enable it.",
    )
    .await?;
    let bytes = handlers::check_document(&req.media_type, &req.data_base64)?;
    let sha = handlers::sha256_hex(&bytes);
    // Retry any legacy attachment whose fingerprint couldn't be taken at
    // startup (a transient read error); a no-op once every row has one.
    handlers::backfill_attachment_hashes(&st).await?;
    let dups = file_duplicates(&st.pool, &sha).await?;
    let household = load_household(&st.pool).await?;
    let body = build_import_request(
        &cfg.model,
        &req.media_type,
        req.data_base64.trim(),
        &household,
        &today(),
    );
    let raw = ingestion::call_tool(&st.http, &st.anthropic_base, &api_key, &body, IMPORT_TOOL)
        .await
        .map_err(AppError::Upstream)?;
    // Stop reason + token usage only (no content): enough to tell a truncated or
    // refused read from a genuinely short document in `docker logs`.
    tracing::info!(
        stop_reason = raw.get("stop_reason").and_then(|v| v.as_str()).unwrap_or("-"),
        usage = %raw.get("usage").cloned().unwrap_or(serde_json::Value::Null),
        "import analyze"
    );
    let extraction = parse_import_response(&raw).map_err(AppError::Upstream)?;
    let mut plan = build_plan(extraction, &household, sha, dups);
    plan.provider = cfg.provider;
    plan.model = cfg.model;
    Ok(Json(plan))
}
