use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use sqlx::types::Json as SqlxJson;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::{
    crypto, default_gradient, diag, ingestion, settings, slugify, truthy, web, AppState, VERSION,
};

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": VERSION }))
}

pub async fn list_users(State(pool): State<SqlitePool>) -> AppResult<Json<Vec<User>>> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY name")
        .fetch_all(&pool)
        .await?;
    Ok(Json(users))
}

pub(crate) async fn load_pet(pool: &SqlitePool, id: &str) -> AppResult<Pet> {
    sqlx::query_as::<_, Pet>("SELECT * FROM pets WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no pet '{id}'")))
}

pub async fn list_pets(
    State(pool): State<SqlitePool>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<Vec<PetSummary>>> {
    let include_archived = truthy(params.get("include_archived").map(String::as_str));
    let pets = sqlx::query_as::<_, Pet>(
        "SELECT * FROM pets WHERE (?1 = 1 OR archived_at IS NULL) \
         ORDER BY (archived_at IS NOT NULL), name COLLATE NOCASE",
    )
    .bind(include_archived as i64)
    .fetch_all(&pool)
    .await?;

    // The soonest not-done reminder per pet ("what's next"), in one query
    // instead of one-per-pet (avoids N+1). Ordered soonest-first so the first
    // row seen for each pet is its next reminder; overdue (past due_on) sorts
    // ahead of upcoming, which is what we want to surface.
    let next = next_reminder_map(&pool).await?;

    let out = pets
        .into_iter()
        .map(|pet| {
            let next_reminder = next.get(&pet.id).cloned();
            PetSummary { pet, next_reminder }
        })
        .collect();
    Ok(Json(out))
}

/// pet_id -> its soonest not-done reminder, in one grouped pass. Reminders are
/// small for a household, so we scan the not-done set ordered soonest-first and
/// keep the first row per pet.
async fn next_reminder_map(pool: &SqlitePool) -> AppResult<HashMap<String, Reminder>> {
    let rows = sqlx::query_as::<_, Reminder>(
        "SELECT r.* FROM reminders r JOIN pets p ON p.id = r.pet_id \
         WHERE r.done = 0 AND p.archived_at IS NULL ORDER BY r.due_on ASC, r.id ASC",
    )
    .fetch_all(pool)
    .await?;
    let mut map: HashMap<String, Reminder> = HashMap::new();
    for r in rows {
        map.entry(r.pet_id.clone()).or_insert(r);
    }
    Ok(map)
}

pub async fn get_pet(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<PetDetail>> {
    let pet = load_pet(&pool, &id).await?;
    let records = sqlx::query_as::<_, MedicalRecord>(
        "SELECT * FROM medical_records WHERE pet_id = ? ORDER BY occurred_on DESC, id DESC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    let weights = sqlx::query_as::<_, WeightEntry>(
        "SELECT * FROM weight_entries WHERE pet_id = ? ORDER BY recorded_on ASC, id ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;

    // Attachments for all of this pet's records in one query, grouped in Rust.
    let atts = sqlx::query_as::<_, Attachment>(
        "SELECT a.* FROM attachments a JOIN medical_records r ON a.record_id = r.id \
         WHERE r.pet_id = ? ORDER BY a.created_at ASC, a.id ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    let mut by_record: HashMap<i64, Vec<Attachment>> = HashMap::new();
    for a in atts {
        by_record.entry(a.record_id).or_default().push(a);
    }

    let reminders = sqlx::query_as::<_, Reminder>(
        "SELECT * FROM reminders WHERE pet_id = ? ORDER BY done ASC, due_on ASC, id ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;

    // The pet's care team: contacts linked to it (#20), newest link first.
    let contacts = sqlx::query_as::<_, Contact>(
        "SELECT c.* FROM contacts c JOIN pet_contacts pc ON pc.contact_id = c.id \
         WHERE pc.pet_id = ? ORDER BY c.name COLLATE NOCASE",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;

    // Insurance policies + wellness plans on the pet (#8/#29), active first.
    let insurance_policies = sqlx::query_as::<_, InsurancePolicy>(
        "SELECT * FROM insurance_policies WHERE pet_id = ? \
         ORDER BY (status != 'active'), provider COLLATE NOCASE, created_at DESC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;

    // Standing care (#63): what the pet is on. Current entries (no end date)
    // first, then by kind in the order the profile shows them, then product.
    let regimens = sqlx::query_as::<_, Regimen>(
        "SELECT * FROM pet_regimens WHERE pet_id = ? ORDER BY \
           (ended_on IS NOT NULL), \
           CASE kind WHEN 'food' THEN 0 WHEN 'preventative' THEN 1 \
                     WHEN 'medication' THEN 2 ELSE 3 END, \
           product COLLATE NOCASE",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;

    // Contacts referenced by this pet's records, to resolve each record's clinic
    // in one query (a record may point at a contact not on the care team).
    let rec_contacts = sqlx::query_as::<_, Contact>(
        "SELECT DISTINCT c.* FROM contacts c JOIN medical_records r ON r.contact_id = c.id \
         WHERE r.pet_id = ?",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    let contact_by_id: HashMap<String, Contact> = rec_contacts
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();

    // "What's next": the soonest not-done reminder. `reminders` is already
    // ordered done-first then soonest due_on, so the first not-done row is it.
    // An archived pet has nothing "next" (#3): its old reminders stay on file as
    // history but must not keep prompting for a pet that is gone.
    let next_reminder = if pet.archived_at.is_some() {
        None
    } else {
        reminders.iter().find(|r| !r.done).cloned()
    };
    // Same for the spay/neuter suggestion (#72): nothing to prompt for a pet
    // that is gone.
    let sex_suggestion = if pet.archived_at.is_some() {
        None
    } else {
        crate::sex::suggestion(pet.sex.as_deref(), &records)
    };
    let records = records
        .into_iter()
        .map(|r| {
            let attachments = by_record.remove(&r.id).unwrap_or_default();
            let contact = r
                .contact_id
                .as_deref()
                .and_then(|cid| contact_by_id.get(cid).cloned());
            RecordDetail {
                record: r,
                attachments,
                contact,
            }
        })
        .collect();
    Ok(Json(PetDetail {
        pet,
        next_reminder,
        records,
        weights,
        reminders,
        contacts,
        insurance_policies,
        regimens,
        sex_suggestion,
    }))
}

async fn unique_slug(pool: &SqlitePool, name: &str) -> AppResult<String> {
    let base = slugify(name);
    let mut candidate = base.clone();
    let mut n = 2;
    loop {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM pets WHERE id = ?")
            .bind(&candidate)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Ok(candidate);
        }
        candidate = format!("{base}-{n}");
        n += 1;
    }
}

pub async fn create_pet(
    State(pool): State<SqlitePool>,
    Json(np): Json<NewPet>,
) -> AppResult<(StatusCode, Json<Pet>)> {
    if np.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if np.species.trim().is_empty() {
        return Err(AppError::BadRequest("species is required".into()));
    }
    let color = bounded_text("color", np.color)?;
    let breed = bounded_text("breed", np.breed)?;
    let sex = crate::sex::validate_sex(np.sex)?;
    let microchip = bounded_text("microchip", np.microchip)?;
    let birth_date = iso_date("birth_date", np.birth_date)?;
    let gotcha_date = iso_date("gotcha_date", np.gotcha_date)?;
    let allergies = clean_labeled("allergies", np.allergies)?.map(SqlxJson);
    let id = unique_slug(&pool, &np.name).await?;
    let (dga, dgb) = default_gradient(&np.species);
    let ga = np.gradient_a.unwrap_or_else(|| dga.to_string());
    let gb = np.gradient_b.unwrap_or_else(|| dgb.to_string());

    sqlx::query(
        "INSERT INTO pets \
         (id, name, species, breed, sex, birth_date, gotcha_date, weight, weight_unit, microchip, \
          color, allergies, insurer, insurer_reimburse, gradient_a, gradient_b, created_by, \
          created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?, datetime('now'), datetime('now'))",
    )
    .bind(&id)
    .bind(np.name.trim())
    .bind(np.species.trim())
    .bind(&breed)
    .bind(&sex)
    .bind(&birth_date)
    .bind(&gotcha_date)
    .bind(np.weight)
    .bind(&np.weight_unit)
    .bind(&microchip)
    .bind(&color)
    .bind(allergies)
    .bind(&np.insurer)
    .bind(&np.insurer_reimburse)
    .bind(&ga)
    .bind(&gb)
    .bind(&np.created_by)
    .execute(&pool)
    .await?;

    let pet = load_pet(&pool, &id).await?;
    Ok((StatusCode::CREATED, Json(pet)))
}

/// Edit a pet's profile. `name`/`species` are set-or-keep and may never be
/// blanked; every other field is null-vs-absent (`double_option`) so the editor
/// can CLEAR a value — the old blanket `COALESCE` made a typo'd microchip or a
/// wrong birth date permanent.
///
/// The merge happens IN SQL (`CASE WHEN <present> THEN <value> ELSE col END`),
/// NOT by loading the row into Rust and writing every column back. Read-then-
/// write loses data when two edits overlap: both read the old row, and the
/// second write restores whatever the first one changed from its stale copy.
/// One statement has no such window. DO NOT "simplify" this back into a load,
/// a merge in Rust, and an UPDATE.
pub async fn update_pet(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(up): Json<UpdatePet>,
) -> AppResult<Json<Pet>> {
    load_pet(&pool, &id).await?; // 404 if missing
    // name/species: set-or-keep, and never blank.
    let name = match up.name {
        Some(n) if n.trim().is_empty() => {
            return Err(AppError::BadRequest("name cannot be empty".into()))
        }
        other => bounded_text("name", other)?,
    };
    let species = match up.species {
        Some(sp) if sp.trim().is_empty() => {
            return Err(AppError::BadRequest("species cannot be empty".into()))
        }
        other => bounded_text("species", other)?,
    };
    let breed = up.breed.map(|v| bounded_text("breed", v)).transpose()?;
    let sex = up.sex.map(crate::sex::validate_sex).transpose()?;
    let birth_date = up.birth_date.map(|v| iso_date("birth_date", v)).transpose()?;
    let gotcha_date = up.gotcha_date.map(|v| iso_date("gotcha_date", v)).transpose()?;
    let weight_unit = up
        .weight_unit
        .map(|v| bounded_text("weight_unit", v))
        .transpose()?;
    let microchip = up.microchip.map(|v| bounded_text("microchip", v)).transpose()?;
    let color = up.color.map(|v| bounded_text("color", v)).transpose()?;
    let insurer = up.insurer.map(|v| bounded_text("insurer", v)).transpose()?;
    let insurer_reimburse = up
        .insurer_reimburse
        .map(|v| bounded_text("insurer_reimburse", v))
        .transpose()?;
    // allergies: absent → keep; present → overwrite (empty list → NULL).
    let allergies = up
        .allergies
        .map(|v| clean_labeled("allergies", v))
        .transpose()?
        .map(|v| v.map(SqlxJson));

    sqlx::query(
        "UPDATE pets SET \
           name = COALESCE(?, name), \
           species = COALESCE(?, species), \
           breed = CASE WHEN ? THEN ? ELSE breed END, \
           sex = CASE WHEN ? THEN ? ELSE sex END, \
           birth_date = CASE WHEN ? THEN ? ELSE birth_date END, \
           gotcha_date = CASE WHEN ? THEN ? ELSE gotcha_date END, \
           weight = CASE WHEN ? THEN ? ELSE weight END, \
           weight_unit = CASE WHEN ? THEN ? ELSE weight_unit END, \
           microchip = CASE WHEN ? THEN ? ELSE microchip END, \
           color = CASE WHEN ? THEN ? ELSE color END, \
           allergies = CASE WHEN ? THEN ? ELSE allergies END, \
           insurer = CASE WHEN ? THEN ? ELSE insurer END, \
           insurer_reimburse = CASE WHEN ? THEN ? ELSE insurer_reimburse END, \
           updated_at = datetime('now') \
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&species)
    .bind(breed.is_some())
    .bind(breed.flatten())
    .bind(sex.is_some())
    .bind(sex.flatten())
    .bind(birth_date.is_some())
    .bind(birth_date.flatten())
    .bind(gotcha_date.is_some())
    .bind(gotcha_date.flatten())
    .bind(up.weight.is_some())
    .bind(up.weight.flatten())
    .bind(weight_unit.is_some())
    .bind(weight_unit.flatten())
    .bind(microchip.is_some())
    .bind(microchip.flatten())
    .bind(color.is_some())
    .bind(color.flatten())
    .bind(allergies.is_some())
    .bind(allergies.flatten())
    .bind(insurer.is_some())
    .bind(insurer.flatten())
    .bind(insurer_reimburse.is_some())
    .bind(insurer_reimburse.flatten())
    .bind(&id)
    .execute(&pool)
    .await?;
    Ok(Json(load_pet(&pool, &id).await?))
}

/// Archive a pet — or edit an existing archive (#3, #37). Partial like every
/// other edit here: an omitted field keeps its value (see [`ArchivePet`]), so a
/// note-only correction can't flip a rehomed pet to `passed` or move its date.
/// `kind` is `passed` (the only meaning archiving had before 0014) or `rehomed`
/// for a pet that is alive and simply no longer in the household.
pub async fn archive_pet(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(body): Json<ArchivePet>,
) -> AppResult<Json<Pet>> {
    let pet = load_pet(&pool, &id).await?;
    let kind = match body.kind {
        Some(k) => k.trim().to_string(),
        None => pet
            .archived_at
            .as_ref()
            .and(pet.archive_kind.clone())
            .unwrap_or_else(|| "passed".to_string()),
    };
    if !ARCHIVE_KINDS.contains(&kind.as_str()) {
        return Err(AppError::BadRequest(format!(
            "unknown archive kind '{kind}' (expected one of: {})",
            ARCHIVE_KINDS.join(", ")
        )));
    }
    let archived_at = body.archived_at.map(|v| iso_date("archived_at", v)).transpose()?;
    let note = body
        .memorial_note
        .map(|v| bounded_text("memorial_note", v))
        .transpose()?;
    // A date, not a timestamp: it is the day it happened, shown as a date and
    // edited with a date input. Present-but-null → today; absent → keep (today
    // on a first archive). Same present/absent split for the note.
    sqlx::query(
        "UPDATE pets SET \
           archive_kind = ?, \
           archived_at = CASE WHEN ? THEN COALESCE(?, date('now')) \
                              ELSE COALESCE(archived_at, date('now')) END, \
           memorial_note = CASE WHEN ? THEN ? ELSE memorial_note END, \
           updated_at = datetime('now') \
         WHERE id = ?",
    )
    .bind(&kind)
    .bind(archived_at.is_some())
    .bind(archived_at.flatten())
    .bind(note.is_some())
    .bind(note.flatten())
    .bind(&id)
    .execute(&pool)
    .await?;
    Ok(Json(load_pet(&pool, &id).await?))
}

/// Bring an archived pet back to the active household (#3: an accidental
/// archive must be undoable). Clears the date, kind and note together; every
/// record, reminder and attachment was untouched by the archive, so nothing
/// else needs restoring.
pub async fn unarchive_pet(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Pet>> {
    load_pet(&pool, &id).await?;
    sqlx::query(
        "UPDATE pets SET archived_at = NULL, archive_kind = NULL, memorial_note = NULL, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&id)
    .execute(&pool)
    .await?;
    Ok(Json(load_pet(&pool, &id).await?))
}

pub async fn delete_pet(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    // Collect the pet's attachment files first so we can unlink them after the
    // cascade deletes the rows — otherwise the bytes orphan on the volume.
    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT a.stored_path FROM attachments a JOIN medical_records r ON a.record_id = r.id \
         WHERE r.pet_id = ?",
    )
    .bind(&id)
    .fetch_all(&st.pool)
    .await?;

    let res = sqlx::query("DELETE FROM pets WHERE id = ?")
        .bind(&id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("no pet '{id}'")));
    }
    for rel in &paths {
        let _ = std::fs::remove_file(st.attachments_dir.join(rel));
    }
    // Prune the now-empty per-record directories so they don't accumulate on the
    // volume forever (remove_dir is a no-op if a dir still holds files).
    let dirs: std::collections::HashSet<_> = paths
        .iter()
        .filter_map(|r| std::path::Path::new(r).parent().map(|p| p.to_path_buf()))
        .collect();
    for d in dirs {
        let _ = std::fs::remove_dir(st.attachments_dir.join(d));
    }
    // Drop the pet's portrait tree too (#21). `id` is a validated slug (it just
    // matched a real row), so it can't traverse out of the photos dir.
    let _ = std::fs::remove_dir_all(st.attachments_dir.join(format!("{PHOTO_SUBDIR}/{id}")));
    // And its insurance-document tree (#8/#29); the policy rows already cascaded.
    let _ = std::fs::remove_dir_all(st.attachments_dir.join(format!("{INSURANCE_SUBDIR}/{id}")));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_records(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<MedicalRecord>>> {
    load_pet(&pool, &id).await?;
    let records = sqlx::query_as::<_, MedicalRecord>(
        "SELECT * FROM medical_records WHERE pet_id = ? ORDER BY occurred_on DESC, id DESC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(records))
}

pub async fn create_record(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(nr): Json<NewRecord>,
) -> AppResult<(StatusCode, Json<MedicalRecord>)> {
    load_pet(&pool, &id).await?;
    if nr.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    // Real calendar dates only: a "2026-02-31" stored here sorts and charts as
    // garbage, and the browser silently rolls it into March.
    let occurred_on = iso_date("occurred_on", Some(nr.occurred_on))?
        .ok_or_else(|| AppError::BadRequest("occurred_on is required".into()))?;
    let ends_on = iso_date("ends_on", nr.ends_on)?;
    // A blank contact_id means "no clinic"; a non-blank one must resolve to a real
    // contact so the record never carries a dangling clinic link (#20).
    let contact_id = nr.contact_id.filter(|s| !s.is_empty());
    if let Some(cid) = contact_id.as_deref() {
        contact_exists(&pool, cid).await?;
    }
    // Drop empty details/breakdown so the columns stay NULL rather than "{}"/"[]".
    let extra = nr.extra.filter(|m| !m.is_empty()).map(SqlxJson);
    let line_items = nr.line_items.filter(|v| !v.is_empty()).map(SqlxJson);
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO medical_records \
         (pet_id, occurred_on, ends_on, title, notes, category, cost_cents, covered_cents, source, contact_id, extra, line_items, created_by, created_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?, datetime('now')) RETURNING id",
    )
    .bind(&id)
    .bind(&occurred_on)
    .bind(&ends_on)
    .bind(nr.title.trim())
    .bind(&nr.notes)
    .bind(&nr.category)
    .bind(nr.cost_cents)
    .bind(nr.covered_cents)
    .bind(&nr.source)
    .bind(&contact_id)
    .bind(extra)
    .bind(line_items)
    .bind(&nr.created_by)
    .fetch_one(&pool)
    .await?;

    let rec = sqlx::query_as::<_, MedicalRecord>("SELECT * FROM medical_records WHERE id = ?")
        .bind(new_id)
        .fetch_one(&pool)
        .await?;
    Ok((StatusCode::CREATED, Json(rec)))
}

/// Load a record, 404-ing unless it exists AND belongs to `pet_id`. The pet
/// check means a mismatched URL (`/pets/other/records/:rid`) can't touch a
/// record it doesn't own.
async fn load_record_of_pet(pool: &SqlitePool, pet_id: &str, rid: i64) -> AppResult<MedicalRecord> {
    sqlx::query_as::<_, MedicalRecord>("SELECT * FROM medical_records WHERE id = ? AND pet_id = ?")
        .bind(rid)
        .bind(pet_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no record {rid} for pet '{pet_id}'")))
}

/// Edit a record (#15). Partial: an omitted field is left as-is; a `null` on a
/// nullable field clears it (see [`UpdateRecord`]). Empty `extra`/`line_items`
/// collapse to NULL, mirroring create.
pub async fn update_record(
    State(pool): State<SqlitePool>,
    Path((id, rid)): Path<(String, i64)>,
    Json(up): Json<UpdateRecord>,
) -> AppResult<Json<MedicalRecord>> {
    let cur = load_record_of_pet(&pool, &id, rid).await?;

    let title = match up.title {
        Some(t) => {
            let t = t.trim().to_string();
            if t.is_empty() {
                return Err(AppError::BadRequest("title cannot be empty".into()));
            }
            t
        }
        None => cur.title,
    };
    let occurred_on = match up.occurred_on {
        Some(d) => iso_date("occurred_on", Some(d))?
            .ok_or_else(|| AppError::BadRequest("occurred_on cannot be blank".into()))?,
        None => cur.occurred_on,
    };
    let category = up.category.unwrap_or(cur.category);
    let ends_on = match up.ends_on {
        Some(v) => iso_date("ends_on", v)?,
        None => cur.ends_on,
    };
    let notes = up.notes.unwrap_or(cur.notes);
    let cost_cents = up.cost_cents.unwrap_or(cur.cost_cents);
    // contact_id: omitted → keep; null/"" → unlink; an id → validate then set (#20).
    let contact_id = match up.contact_id {
        Some(v) => {
            let v = v.filter(|s| !s.is_empty());
            if let Some(cid) = v.as_deref() {
                contact_exists(&pool, cid).await?;
            }
            v
        }
        None => cur.contact_id,
    };
    let covered_cents = up.covered_cents.unwrap_or(cur.covered_cents);
    // For the JSON columns, a present value overwrites (empty → NULL); an absent
    // key keeps whatever is stored.
    let extra = match up.extra {
        Some(v) => v.filter(|m| !m.is_empty()).map(SqlxJson),
        None => cur.extra,
    };
    let line_items = match up.line_items {
        Some(v) => v.filter(|v| !v.is_empty()).map(SqlxJson),
        None => cur.line_items,
    };

    sqlx::query(
        "UPDATE medical_records SET \
           occurred_on = ?, ends_on = ?, title = ?, notes = ?, category = ?, \
           cost_cents = ?, covered_cents = ?, contact_id = ?, extra = ?, line_items = ? \
         WHERE id = ?",
    )
    .bind(&occurred_on)
    .bind(&ends_on)
    .bind(&title)
    .bind(&notes)
    .bind(&category)
    .bind(cost_cents)
    .bind(covered_cents)
    .bind(&contact_id)
    .bind(extra)
    .bind(line_items)
    .bind(rid)
    .execute(&pool)
    .await?;

    Ok(Json(
        sqlx::query_as::<_, MedicalRecord>("SELECT * FROM medical_records WHERE id = ?")
            .bind(rid)
            .fetch_one(&pool)
            .await?,
    ))
}

/// Delete a record (#15). Verifies pet ownership, unlinks its attachment files
/// (the rows go via ON DELETE CASCADE), and prunes the now-empty per-record dir
/// — the same file-hygiene the pet delete does, scoped to one record.
pub async fn delete_record(
    State(st): State<AppState>,
    Path((id, rid)): Path<(String, i64)>,
) -> AppResult<StatusCode> {
    load_record_of_pet(&st.pool, &id, rid).await?;
    let paths: Vec<String> =
        sqlx::query_scalar("SELECT stored_path FROM attachments WHERE record_id = ?")
            .bind(rid)
            .fetch_all(&st.pool)
            .await?;
    sqlx::query("DELETE FROM medical_records WHERE id = ?")
        .bind(rid)
        .execute(&st.pool)
        .await?;
    for rel in &paths {
        let _ = std::fs::remove_file(st.attachments_dir.join(rel));
    }
    let _ = std::fs::remove_dir(st.attachments_dir.join(rid.to_string()));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_weights(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<WeightEntry>>> {
    load_pet(&pool, &id).await?;
    let weights = sqlx::query_as::<_, WeightEntry>(
        "SELECT * FROM weight_entries WHERE pet_id = ? ORDER BY recorded_on ASC, id ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(weights))
}

/// Validate a weight's unit + value. The unit is a closed pair because the chart
/// converts between exactly these two; a zero/negative weight is a typo.
pub(crate) fn check_weight(value: f64, unit: &str) -> AppResult<()> {
    if !(value.is_finite() && value > 0.0) {
        return Err(AppError::BadRequest("weight must be a positive number".into()));
    }
    if unit != "lb" && unit != "kg" {
        return Err(AppError::BadRequest(format!("weight unit must be 'lb' or 'kg', got '{unit}'")));
    }
    Ok(())
}

pub async fn create_weight(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(nw): Json<NewWeight>,
) -> AppResult<(StatusCode, Json<WeightEntry>)> {
    load_pet(&pool, &id).await?;
    let recorded_on = iso_date("recorded_on", Some(nw.recorded_on))?
        .ok_or_else(|| AppError::BadRequest("recorded_on is required".into()))?;
    check_weight(nw.value, &nw.unit)?;
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO weight_entries (pet_id, recorded_on, value, unit) VALUES (?,?,?,?) RETURNING id",
    )
    .bind(&id)
    .bind(&recorded_on)
    .bind(nw.value)
    .bind(&nw.unit)
    .fetch_one(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(load_weight_of_pet(&pool, &id, new_id).await?)))
}

async fn load_weight_of_pet(pool: &SqlitePool, pet_id: &str, wid: i64) -> AppResult<WeightEntry> {
    sqlx::query_as::<_, WeightEntry>("SELECT * FROM weight_entries WHERE id = ? AND pet_id = ?")
        .bind(wid)
        .bind(pet_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no weight {wid} for pet '{pet_id}'")))
}

/// Correct a logged weight (#23). Before this a mistyped weight could only be
/// lived with: there was no edit or delete path, so a 44.8 that should have been
/// 48.0 sat in the chart forever. Partial: omitted keys keep their value.
pub async fn update_weight(
    State(pool): State<SqlitePool>,
    Path((id, wid)): Path<(String, i64)>,
    Json(up): Json<UpdateWeight>,
) -> AppResult<Json<WeightEntry>> {
    let cur = load_weight_of_pet(&pool, &id, wid).await?;
    let recorded_on = match up.recorded_on {
        Some(d) => iso_date("recorded_on", Some(d))?
            .ok_or_else(|| AppError::BadRequest("recorded_on cannot be blank".into()))?,
        None => cur.recorded_on,
    };
    let value = up.value.unwrap_or(cur.value);
    let unit = up.unit.unwrap_or(cur.unit);
    check_weight(value, &unit)?;
    sqlx::query("UPDATE weight_entries SET recorded_on = ?, value = ?, unit = ? WHERE id = ?")
        .bind(&recorded_on)
        .bind(value)
        .bind(&unit)
        .bind(wid)
        .execute(&pool)
        .await?;
    Ok(Json(load_weight_of_pet(&pool, &id, wid).await?))
}

pub async fn delete_weight(
    State(pool): State<SqlitePool>,
    Path((id, wid)): Path<(String, i64)>,
) -> AppResult<StatusCode> {
    load_weight_of_pet(&pool, &id, wid).await?;
    sqlx::query("DELETE FROM weight_entries WHERE id = ?")
        .bind(wid)
        .execute(&pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Reminders / calendar events (#26) ----------

pub async fn list_reminders(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Reminder>>> {
    load_pet(&pool, &id).await?;
    let rows = sqlx::query_as::<_, Reminder>(
        "SELECT * FROM reminders WHERE pet_id = ? ORDER BY done ASC, due_on ASC, id ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}

/// Create a reminder. Idempotent on (pet, due_on, case-insensitive title): if an
/// exact match already exists it is returned with 200 instead of duplicating —
/// the dupe guard for extraction proposing an already-scheduled item.
pub async fn create_reminder(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(nr): Json<NewReminder>,
) -> AppResult<(StatusCode, Json<Reminder>)> {
    load_pet(&pool, &id).await?;
    if nr.title.trim().is_empty() {
        return Err(AppError::BadRequest("reminder title is required".into()));
    }
    if nr.due_on.trim().is_empty() {
        return Err(AppError::BadRequest("reminder due date is required".into()));
    }
    if let Some(existing) = sqlx::query_as::<_, Reminder>(
        "SELECT * FROM reminders WHERE pet_id = ? AND due_on = ? AND lower(title) = lower(?) LIMIT 1",
    )
    .bind(&id)
    .bind(nr.due_on.trim())
    .bind(nr.title.trim())
    .fetch_optional(&pool)
    .await?
    {
        return Ok((StatusCode::OK, Json(existing)));
    }
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO reminders (pet_id, due_on, title, kind, source, created_at) \
         VALUES (?,?,?,?,?, datetime('now')) RETURNING id",
    )
    .bind(&id)
    .bind(nr.due_on.trim())
    .bind(nr.title.trim())
    .bind(&nr.kind)
    .bind(&nr.source)
    .fetch_one(&pool)
    .await?;
    let rem = sqlx::query_as::<_, Reminder>("SELECT * FROM reminders WHERE id = ?")
        .bind(new_id)
        .fetch_one(&pool)
        .await?;
    Ok((StatusCode::CREATED, Json(rem)))
}

pub async fn update_reminder(
    State(pool): State<SqlitePool>,
    Path(rid): Path<i64>,
    Json(up): Json<UpdateReminder>,
) -> AppResult<Json<Reminder>> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM reminders WHERE id = ?")
        .bind(rid)
        .fetch_optional(&pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("no reminder {rid}")));
    }
    sqlx::query(
        "UPDATE reminders SET \
           done = COALESCE(?, done), \
           due_on = COALESCE(?, due_on), \
           title = COALESCE(?, title) \
         WHERE id = ?",
    )
    .bind(up.done)
    .bind(&up.due_on)
    .bind(&up.title)
    .bind(rid)
    .execute(&pool)
    .await?;
    Ok(Json(
        sqlx::query_as::<_, Reminder>("SELECT * FROM reminders WHERE id = ?")
            .bind(rid)
            .fetch_one(&pool)
            .await?,
    ))
}

pub async fn delete_reminder(
    State(pool): State<SqlitePool>,
    Path(rid): Path<i64>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM reminders WHERE id = ?")
        .bind(rid)
        .execute(&pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("no reminder {rid}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// All reminders across pets, for the calendar. Joins the pet name. Archived
/// pets' reminders are left out by default (#3) so a pet that passed or was
/// rehomed doesn't keep putting vaccines on the calendar; `?include_archived=1`
/// returns them too, for scripts that want the full history.
pub async fn list_all_reminders(
    State(pool): State<SqlitePool>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<Vec<ReminderWithPet>>> {
    let include_archived = truthy(params.get("include_archived").map(String::as_str));
    let rows = sqlx::query_as::<_, ReminderWithPet>(
        "SELECT r.id, r.pet_id, p.name AS pet_name, r.due_on, r.title, r.kind, r.done \
         FROM reminders r JOIN pets p ON p.id = r.pet_id \
         WHERE (?1 = 1 OR p.archived_at IS NULL) \
         ORDER BY r.due_on ASC, r.id ASC",
    )
    .bind(include_archived as i64)
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}

// ---------- Shared contacts (#20) ----------

/// Contact roles the app understands. Kept closed so the data stays clean and the
/// UI can label/group by a known set; extend here (and in the frontend) to add a
/// role. `boarding` is the #42 facility role (labeled "Daycare & boarding") — one
/// role covers both, because in the real world a facility does both under one roof.
pub(crate) const CONTACT_ROLES: &[&str] = &[
    "vet",
    "emergency",
    "specialist",
    "groomer",
    "boarding",
    "other",
];

pub(crate) fn validate_role(role: &str) -> AppResult<()> {
    if CONTACT_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "unknown contact role '{role}' (expected one of: {})",
            CONTACT_ROLES.join(", ")
        )))
    }
}

async fn load_contact(pool: &SqlitePool, cid: &str) -> AppResult<Contact> {
    sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = ?")
        .bind(cid)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no contact '{cid}'")))
}

/// 404 unless the contact exists — used to reject a record/link pointing at a
/// contact that isn't there, with a clean error instead of an FK violation.
pub(crate) async fn contact_exists(pool: &SqlitePool, cid: &str) -> AppResult<()> {
    let found: Option<String> = sqlx::query_scalar("SELECT id FROM contacts WHERE id = ?")
        .bind(cid)
        .fetch_optional(pool)
        .await?;
    found
        .map(|_| ())
        .ok_or_else(|| AppError::NotFound(format!("no contact '{cid}'")))
}

/// List contacts, optionally filtered by `?role=`. Ordered by name.
pub async fn list_contacts(
    State(pool): State<SqlitePool>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<Vec<Contact>>> {
    let role = params.get("role").cloned();
    let contacts = sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE (?1 IS NULL OR role = ?1) ORDER BY name COLLATE NOCASE",
    )
    .bind(role)
    .fetch_all(&pool)
    .await?;
    Ok(Json(contacts))
}

pub async fn create_contact(
    State(pool): State<SqlitePool>,
    Json(nc): Json<NewContact>,
) -> AppResult<(StatusCode, Json<Contact>)> {
    if nc.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    validate_role(&nc.role)?;
    let id = uuid::Uuid::new_v4().to_string();
    // Drop empty lists so the columns stay NULL, not "[]" (#42).
    let requirements = nc.requirements.filter(|v| !v.is_empty()).map(SqlxJson);
    let hours = nc.hours.filter(|v| !v.is_empty()).map(SqlxJson);
    sqlx::query(
        "INSERT INTO contacts \
         (id, name, role, doctor, phone, email, address, website, notes, requirements, hours, created_by, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?, datetime('now'), datetime('now'))",
    )
    .bind(&id)
    .bind(nc.name.trim())
    .bind(&nc.role)
    .bind(&nc.doctor)
    .bind(&nc.phone)
    .bind(&nc.email)
    .bind(&nc.address)
    .bind(&nc.website)
    .bind(&nc.notes)
    .bind(requirements)
    .bind(hours)
    .bind(&nc.created_by)
    .execute(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(load_contact(&pool, &id).await?)))
}

/// Parse a facility's website (#42): fetch the URL server-side (SSRF-guarded),
/// reduce it to text, and have the LLM propose contact fields + a requirements
/// checklist. NEVER writes — the frontend pre-fills the contact sheet from the
/// response for the user to confirm (human-in-the-loop, same gate as `/extract`).
pub async fn parse_contact_url(
    State(st): State<AppState>,
    Json(req): Json<ParseUrlRequest>,
) -> AppResult<Json<ParseUrlResponse>> {
    let (cfg, api_key) = ai_key(
        &st,
        "Website parsing needs AI. Add your API key in Settings and enable it.",
    )
    .await?;
    // Fetch + reduce to text first (this is where the SSRF guards live). A bad
    // URL / blocked host / dead site is a clean 400, before we spend any tokens.
    let page_text = web::fetch_page_text(&req.url)
        .await
        .map_err(AppError::BadRequest)?;

    let fields = ingestion::extract_facility(
        &st.http,
        &st.anthropic_base,
        &api_key,
        &cfg.model,
        &page_text,
    )
    .await
    .map_err(AppError::Upstream)?;

    Ok(Json(ParseUrlResponse {
        fields,
        provider: cfg.provider,
        model: cfg.model,
    }))
}

pub async fn get_contact(
    State(pool): State<SqlitePool>,
    Path(cid): Path<String>,
) -> AppResult<Json<Contact>> {
    Ok(Json(load_contact(&pool, &cid).await?))
}

/// Edit a contact (#20). Partial: `name`/`role` are set-or-keep; the rest use the
/// null-vs-absent editor semantics (`double_option`). Editing a clinic's phone
/// here updates it everywhere the contact is shown — the whole point of #20.
pub async fn update_contact(
    State(pool): State<SqlitePool>,
    Path(cid): Path<String>,
    Json(up): Json<UpdateContact>,
) -> AppResult<Json<Contact>> {
    let cur = load_contact(&pool, &cid).await?;
    let name = match up.name {
        Some(n) => {
            let n = n.trim().to_string();
            if n.is_empty() {
                return Err(AppError::BadRequest("name cannot be empty".into()));
            }
            n
        }
        None => cur.name,
    };
    let role = match up.role {
        Some(r) => {
            validate_role(&r)?;
            r
        }
        None => cur.role,
    };
    let doctor = up.doctor.unwrap_or(cur.doctor);
    let phone = up.phone.unwrap_or(cur.phone);
    let email = up.email.unwrap_or(cur.email);
    let address = up.address.unwrap_or(cur.address);
    let website = up.website.unwrap_or(cur.website);
    let notes = up.notes.unwrap_or(cur.notes);
    // requirements/hours: absent → keep; present → overwrite (empty list → NULL) (#42).
    let requirements = match up.requirements {
        Some(v) => v.filter(|v| !v.is_empty()).map(SqlxJson),
        None => cur.requirements,
    };
    let hours = match up.hours {
        Some(v) => v.filter(|v| !v.is_empty()).map(SqlxJson),
        None => cur.hours,
    };
    sqlx::query(
        "UPDATE contacts SET \
           name = ?, role = ?, doctor = ?, phone = ?, email = ?, address = ?, \
           website = ?, notes = ?, requirements = ?, hours = ?, updated_at = datetime('now') \
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&role)
    .bind(&doctor)
    .bind(&phone)
    .bind(&email)
    .bind(&address)
    .bind(&website)
    .bind(&notes)
    .bind(requirements)
    .bind(hours)
    .bind(&cid)
    .execute(&pool)
    .await?;
    Ok(Json(load_contact(&pool, &cid).await?))
}

/// Delete a contact. `pet_contacts` links cascade; records that named it have
/// their `contact_id` set NULL (history is never deleted, just unlinked).
pub async fn delete_contact(
    State(pool): State<SqlitePool>,
    Path(cid): Path<String>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&cid)
        .execute(&pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("no contact '{cid}'")));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The contacts linked to one pet (its care team).
pub async fn list_pet_contacts(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Contact>>> {
    load_pet(&pool, &id).await?;
    let contacts = sqlx::query_as::<_, Contact>(
        "SELECT c.* FROM contacts c JOIN pet_contacts pc ON pc.contact_id = c.id \
         WHERE pc.pet_id = ? ORDER BY c.name COLLATE NOCASE",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(contacts))
}

/// Link an existing contact to a pet. Idempotent — linking an already-linked
/// contact is a no-op that still returns the care team, so the UI never sees a
/// spurious error on a double-tap.
pub async fn link_pet_contact(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(body): Json<LinkContact>,
) -> AppResult<Json<Vec<Contact>>> {
    load_pet(&pool, &id).await?;
    contact_exists(&pool, &body.contact_id).await?;
    sqlx::query("INSERT OR IGNORE INTO pet_contacts (pet_id, contact_id) VALUES (?, ?)")
        .bind(&id)
        .bind(&body.contact_id)
        .execute(&pool)
        .await?;
    list_pet_contacts(State(pool), Path(id)).await
}

/// Unlink a contact from a pet (does not delete the contact).
pub async fn unlink_pet_contact(
    State(pool): State<SqlitePool>,
    Path((id, cid)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    load_pet(&pool, &id).await?;
    let res = sqlx::query("DELETE FROM pet_contacts WHERE pet_id = ? AND contact_id = ?")
        .bind(&id)
        .bind(&cid)
        .execute(&pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "contact '{cid}' is not linked to pet '{id}'"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Insurance & wellness plans (#8 layer 1 + #29 insurance slice) ----------

/// Subdir on the attachments volume for policy documents. Grouped by pet so a pet
/// delete can drop the whole tree (like `PHOTO_SUBDIR`).
const INSURANCE_SUBDIR: &str = "insurance";

fn validate_insurance_kind(kind: &str) -> AppResult<()> {
    if INSURANCE_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "unknown plan kind '{kind}' (expected one of: {})",
            INSURANCE_KINDS.join(", ")
        )))
    }
}

fn validate_insurance_status(status: &str) -> AppResult<()> {
    if INSURANCE_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "unknown status '{status}' (expected one of: {})",
            INSURANCE_STATUSES.join(", ")
        )))
    }
}

/// Money on a policy is never negative and a reimbursement is a percentage.
/// Insurance value (#74) does arithmetic on these; a typo'd "-911" premium or
/// "900%" would otherwise flow straight into paid-in and break-even figures.
fn validate_policy_amounts(
    premium: Option<i64>,
    deductible: Option<i64>,
    annual_limit: Option<i64>,
    reimburse_percent: Option<i64>,
) -> AppResult<()> {
    for (name, v) in [("premium_cents", premium), ("deductible_cents", deductible), ("annual_limit_cents", annual_limit)] {
        if v.is_some_and(|v| v < 0) {
            return Err(AppError::BadRequest(format!("{name} cannot be negative")));
        }
    }
    if reimburse_percent.is_some_and(|p| !(0..=100).contains(&p)) {
        return Err(AppError::BadRequest("reimburse_percent must be between 0 and 100".into()));
    }
    Ok(())
}

/// Extension for a stored policy document, from its MIME. `None` for a type we
/// don't accept (guarded before this by [`ingestion::media_supported`]).
fn doc_ext(media_type: &str) -> Option<&'static str> {
    match media_type {
        "application/pdf" => Some("pdf"),
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

async fn load_insurance(pool: &SqlitePool, pid: &str) -> AppResult<InsurancePolicy> {
    sqlx::query_as::<_, InsurancePolicy>("SELECT * FROM insurance_policies WHERE id = ?")
        .bind(pid)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no insurance policy '{pid}'")))
}

/// A pet's insurance policies and wellness plans, active first, then by provider.
pub async fn list_pet_insurance(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<InsurancePolicy>>> {
    load_pet(&pool, &id).await?;
    let policies = sqlx::query_as::<_, InsurancePolicy>(
        "SELECT * FROM insurance_policies WHERE pet_id = ? \
         ORDER BY (status != 'active'), provider COLLATE NOCASE, created_at DESC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(policies))
}

pub async fn create_insurance(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(np): Json<NewInsurancePolicy>,
) -> AppResult<(StatusCode, Json<InsurancePolicy>)> {
    load_pet(&pool, &id).await?;
    if np.provider.trim().is_empty() {
        return Err(AppError::BadRequest("provider is required".into()));
    }
    validate_insurance_kind(&np.kind)?;
    validate_insurance_status(&np.status)?;
    validate_policy_amounts(np.premium_cents, np.deductible_cents, np.annual_limit_cents, np.reimburse_percent)?;
    let pid = uuid::Uuid::new_v4().to_string();
    // Drop empty lists so the columns stay NULL, not "[]" (#20/#42 pattern).
    let coverages = np.coverages.filter(|v| !v.is_empty()).map(SqlxJson);
    let exclusions = np.exclusions.filter(|v| !v.is_empty()).map(SqlxJson);
    sqlx::query(
        "INSERT INTO insurance_policies \
         (id, pet_id, kind, provider, plan_name, policy_number, status, start_date, end_date, \
          premium_cents, premium_interval, reimburse_percent, deductible_cents, annual_limit_cents, \
          notes, coverages, exclusions, created_by, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?, datetime('now'), datetime('now'))",
    )
    .bind(&pid)
    .bind(&id)
    .bind(&np.kind)
    .bind(np.provider.trim())
    .bind(&np.plan_name)
    .bind(&np.policy_number)
    .bind(&np.status)
    .bind(&np.start_date)
    .bind(&np.end_date)
    .bind(np.premium_cents)
    .bind(&np.premium_interval)
    .bind(np.reimburse_percent)
    .bind(np.deductible_cents)
    .bind(np.annual_limit_cents)
    .bind(&np.notes)
    .bind(coverages)
    .bind(exclusions)
    .bind(&np.created_by)
    .execute(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(load_insurance(&pool, &pid).await?)))
}

pub async fn get_insurance(
    State(pool): State<SqlitePool>,
    Path(pid): Path<String>,
) -> AppResult<Json<InsurancePolicy>> {
    Ok(Json(load_insurance(&pool, &pid).await?))
}

/// Edit a policy. `kind`/`provider`/`status` are set-or-keep (never null); the
/// rest use the null-vs-absent editor semantics (`double_option`). The document
/// is managed by the document endpoints, not here.
pub async fn update_insurance(
    State(pool): State<SqlitePool>,
    Path(pid): Path<String>,
    Json(up): Json<UpdateInsurancePolicy>,
) -> AppResult<Json<InsurancePolicy>> {
    let cur = load_insurance(&pool, &pid).await?;
    let kind = match up.kind {
        Some(k) => {
            validate_insurance_kind(&k)?;
            k
        }
        None => cur.kind,
    };
    let provider = match up.provider {
        Some(p) => {
            let p = p.trim().to_string();
            if p.is_empty() {
                return Err(AppError::BadRequest("provider cannot be empty".into()));
            }
            p
        }
        None => cur.provider,
    };
    let status = match up.status {
        Some(s) => {
            validate_insurance_status(&s)?;
            s
        }
        None => cur.status,
    };
    let plan_name = up.plan_name.unwrap_or(cur.plan_name);
    let policy_number = up.policy_number.unwrap_or(cur.policy_number);
    let start_date = up.start_date.unwrap_or(cur.start_date);
    let end_date = up.end_date.unwrap_or(cur.end_date);
    let premium_cents = up.premium_cents.unwrap_or(cur.premium_cents);
    let premium_interval = up.premium_interval.unwrap_or(cur.premium_interval);
    let reimburse_percent = up.reimburse_percent.unwrap_or(cur.reimburse_percent);
    let deductible_cents = up.deductible_cents.unwrap_or(cur.deductible_cents);
    let annual_limit_cents = up.annual_limit_cents.unwrap_or(cur.annual_limit_cents);
    validate_policy_amounts(premium_cents, deductible_cents, annual_limit_cents, reimburse_percent)?;
    let notes = up.notes.unwrap_or(cur.notes);
    // coverages/exclusions: absent → keep; present → overwrite (empty list → NULL).
    let coverages = match up.coverages {
        Some(v) => v.filter(|v| !v.is_empty()).map(SqlxJson),
        None => cur.coverages,
    };
    let exclusions = match up.exclusions {
        Some(v) => v.filter(|v| !v.is_empty()).map(SqlxJson),
        None => cur.exclusions,
    };
    sqlx::query(
        "UPDATE insurance_policies SET \
           kind = ?, provider = ?, plan_name = ?, policy_number = ?, status = ?, \
           start_date = ?, end_date = ?, premium_cents = ?, premium_interval = ?, \
           reimburse_percent = ?, deductible_cents = ?, annual_limit_cents = ?, \
           notes = ?, coverages = ?, exclusions = ?, updated_at = datetime('now') \
         WHERE id = ?",
    )
    .bind(&kind)
    .bind(&provider)
    .bind(&plan_name)
    .bind(&policy_number)
    .bind(&status)
    .bind(&start_date)
    .bind(&end_date)
    .bind(premium_cents)
    .bind(&premium_interval)
    .bind(reimburse_percent)
    .bind(deductible_cents)
    .bind(annual_limit_cents)
    .bind(&notes)
    .bind(coverages)
    .bind(exclusions)
    .bind(&pid)
    .execute(&pool)
    .await?;
    Ok(Json(load_insurance(&pool, &pid).await?))
}

/// Delete a policy and unlink its stored document file.
pub async fn delete_insurance(
    State(st): State<AppState>,
    Path(pid): Path<String>,
) -> AppResult<StatusCode> {
    let policy = load_insurance(&st.pool, &pid).await?;
    sqlx::query("DELETE FROM insurance_policies WHERE id = ?")
        .bind(&pid)
        .execute(&st.pool)
        .await?;
    if let Some(rel) = policy.document_path.as_deref() {
        let _ = std::fs::remove_file(st.attachments_dir.join(rel));
        let _ = std::fs::remove_dir(st.attachments_dir.join(format!(
            "{INSURANCE_SUBDIR}/{}",
            policy.pet_id
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Attach or replace the policy's document (PDF/image). Same write-new-before-
/// delete-old order as pet photos (#21), so a failure never leaves the row
/// pointing at deleted bytes.
pub async fn set_insurance_document(
    State(st): State<AppState>,
    Path(pid): Path<String>,
    Json(nd): Json<NewInsuranceDocument>,
) -> AppResult<Json<InsurancePolicy>> {
    let policy = load_insurance(&st.pool, &pid).await?;
    if !ingestion::media_supported(&nd.media_type) {
        return Err(AppError::BadRequest(format!(
            "unsupported document type '{}' (PDF or image only)",
            nd.media_type
        )));
    }
    let bytes = decode_bounded(&nd.data_base64, MAX_ATTACHMENT_BYTES)?;
    Ok(Json(store_insurance_document(&st, &policy, &nd.filename, &nd.media_type, &bytes).await?))
}

/// Write a policy's document and point the policy at it. Shared by the upload
/// endpoint and bulk import (#23), which already holds the decoded bytes.
pub(crate) async fn store_insurance_document(
    st: &AppState,
    policy: &InsurancePolicy,
    filename: &str,
    media_type: &str,
    bytes: &[u8],
) -> AppResult<InsurancePolicy> {
    let ext = doc_ext(media_type)
        .ok_or_else(|| AppError::BadRequest("unsupported document type".into()))?;
    let pid = &policy.id;
    let rel = format!("{INSURANCE_SUBDIR}/{}/{pid}.{ext}", policy.pet_id);
    let full = st.attachments_dir.join(&rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("could not store document: {e}")))?;
    }
    std::fs::write(&full, bytes)
        .map_err(|e| AppError::Internal(format!("could not store document: {e}")))?;
    sqlx::query(
        "UPDATE insurance_policies SET \
           document_path = ?, document_name = ?, document_media_type = ?, \
           document_size_bytes = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&rel)
    .bind(filename)
    .bind(media_type)
    .bind(bytes.len() as i64)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    // Remove the previous file ONLY after the new one is recorded (path can differ
    // when the media type changed).
    if let Some(old) = policy.document_path.as_deref() {
        if old != rel {
            let _ = std::fs::remove_file(st.attachments_dir.join(old));
        }
    }
    load_insurance(&st.pool, pid).await
}

/// Stream the policy document bytes inline (so a PDF renders in-app, image shows).
pub async fn get_insurance_document(
    State(st): State<AppState>,
    Path(pid): Path<String>,
) -> AppResult<Response> {
    let policy = load_insurance(&st.pool, &pid).await?;
    let rel = policy
        .document_path
        .ok_or_else(|| AppError::NotFound("this policy has no document".into()))?;
    let bytes = std::fs::read(st.attachments_dir.join(&rel))
        .map_err(|_| AppError::NotFound("document file is missing".into()))?;
    let media_type = policy
        .document_media_type
        .unwrap_or_else(|| "application/octet-stream".into());
    let name = policy.document_name.as_deref().unwrap_or("document");
    let disposition = format!("inline; filename=\"{}\"", safe_filename(name));
    Ok((
        [
            (header::CONTENT_TYPE, media_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// Remove the policy's document (keeps the policy).
pub async fn delete_insurance_document(
    State(st): State<AppState>,
    Path(pid): Path<String>,
) -> AppResult<Json<InsurancePolicy>> {
    let policy = load_insurance(&st.pool, &pid).await?;
    if let Some(rel) = policy.document_path.as_deref() {
        let _ = std::fs::remove_file(st.attachments_dir.join(rel));
        let _ = std::fs::remove_dir(st.attachments_dir.join(format!(
            "{INSURANCE_SUBDIR}/{}",
            policy.pet_id
        )));
    }
    sqlx::query(
        "UPDATE insurance_policies SET \
           document_path = NULL, document_name = NULL, document_media_type = NULL, \
           document_size_bytes = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&pid)
    .execute(&st.pool)
    .await?;
    Ok(Json(load_insurance(&st.pool, &pid).await?))
}

/// Extract structured fields from an insurance/plan document (#8/#29). Same gate
/// order as `/extract`: setup check → media check → size/base64 check → decrypt →
/// extract. NEVER writes — the frontend pre-fills the sheet for confirmation.
pub async fn extract_insurance_doc(
    State(st): State<AppState>,
    Json(req): Json<InsuranceExtractRequest>,
) -> AppResult<Json<InsuranceExtractResponse>> {
    let (cfg, api_key) = ai_key(&st, SCAN_NOT_SET_UP).await?;
    check_document(&req.media_type, &req.data_base64)?;
    let fields = ingestion::extract_insurance(
        &st.http,
        &st.anthropic_base,
        &api_key,
        &cfg.model,
        &req.media_type,
        req.data_base64.trim(),
    )
    .await
    .map_err(AppError::Upstream)?;
    Ok(Json(InsuranceExtractResponse {
        fields,
        provider: cfg.provider,
        model: cfg.model,
    }))
}

pub async fn costs_summary(State(pool): State<SqlitePool>) -> AppResult<Json<CostsSummary>> {
    let total_cents: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(cost_cents),0) FROM medical_records")
            .fetch_one(&pool)
            .await?;
    let covered_cents: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(covered_cents),0) FROM medical_records")
            .fetch_one(&pool)
            .await?;

    let by_category = sqlx::query_as::<_, (String, i64)>(
        "SELECT category, COALESCE(SUM(cost_cents),0) as c FROM medical_records \
         GROUP BY category HAVING c > 0 ORDER BY c DESC",
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|(category, cost_cents)| CategoryTotal {
        category,
        cost_cents,
    })
    .collect();

    let by_pet = sqlx::query_as::<_, PetCostRow>(
        "SELECT p.id as pet_id, p.name as name, p.insurer as insurer, \
                COALESCE(SUM(r.cost_cents),0) as cost_cents, \
                COALESCE(SUM(r.covered_cents),0) as covered_cents \
         FROM pets p JOIN medical_records r ON r.pet_id = p.id \
         GROUP BY p.id HAVING cost_cents > 0 ORDER BY cost_cents DESC",
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(CostsSummary {
        total_cents,
        covered_cents,
        by_category,
        by_pet,
    }))
}

// ---------- LLM ingestion settings (#4) ----------

fn ingestion_view(cfg: &settings::IngestionConfig) -> IngestionSettingsView {
    IngestionSettingsView {
        enabled: cfg.enabled,
        provider: cfg.provider.clone(),
        model: cfg.model.clone(),
        has_key: cfg.api_key_enc.is_some(),
    }
}

pub async fn get_ingestion(State(st): State<AppState>) -> AppResult<Json<IngestionSettingsView>> {
    let cfg = settings::load(&st.pool).await?;
    Ok(Json(ingestion_view(&cfg)))
}

pub async fn put_ingestion(
    State(st): State<AppState>,
    Json(up): Json<IngestionSettingsUpdate>,
) -> AppResult<Json<IngestionSettingsView>> {
    // Validate the WHOLE request before writing any of it, then write in one
    // transaction: a refused key must not leave the new model stored beside the
    // old key.
    // Only Anthropic is implemented today; reject others rather than silently
    // storing a provider we can't call.
    if let Some(p) = up.provider.as_deref() {
        if p != settings::DEFAULT_PROVIDER {
            return Err(AppError::BadRequest(format!(
                "unsupported provider '{p}' (only '{}' is available)",
                settings::DEFAULT_PROVIDER
            )));
        }
    }
    let model = up.model.as_deref().map(str::trim);
    if model == Some("") {
        return Err(AppError::BadRequest("model cannot be empty".into()));
    }
    // A present, non-empty key replaces the stored one; empty/absent leaves it be.
    let api_key = up
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    if let Some(k) = api_key {
        check_key_provider(k)?;
    }

    let mut tx = st.pool.begin().await?;
    if let Some(p) = up.provider.as_deref() {
        settings::set_provider(&mut tx, p).await?;
    }
    if let Some(m) = model {
        settings::set_model(&mut tx, m).await?;
    }
    if let Some(k) = api_key {
        settings::set_api_key_enc(&mut tx, &crypto::encrypt(&st.key, k)).await?;
    }
    if let Some(enabled) = up.enabled {
        settings::set_enabled(&mut tx, enabled).await?;
    }
    tx.commit().await?;
    let cfg = settings::load(&st.pool).await?;
    Ok(Json(ingestion_view(&cfg)))
}

pub async fn delete_ingestion(
    State(st): State<AppState>,
) -> AppResult<Json<IngestionSettingsView>> {
    let mut tx = st.pool.begin().await?;
    settings::clear_api_key(&mut tx).await?;
    settings::set_enabled(&mut tx, false).await?;
    tx.commit().await?;
    let cfg = settings::load(&st.pool).await?;
    Ok(Json(ingestion_view(&cfg)))
}

/// A key that clearly belongs to another provider is refused before it is saved
/// or sent anywhere (#67): only Anthropic is implemented, so an OpenAI or Gemini
/// key could never work, and passing it to Anthropic would hand one vendor's
/// secret to another.
fn check_key_provider(api_key: &str) -> AppResult<()> {
    match ingestion::foreign_key_provider(api_key) {
        Some(other) => Err(AppError::BadRequest(format!(
            "That looks like a key for {other}, not Anthropic. FurFiles reads documents with \
             Anthropic (Claude) today, so it needs an Anthropic key (they start with sk-ant-)."
        ))),
        None => Ok(()),
    }
}

/// Decrypt the stored key for a provider call. The provider check runs here too,
/// not only on save: versions before #67 accepted any key, so an OpenAI or
/// Gemini key may already be stored, and it must not be sent to Anthropic.
fn decrypt_key(st: &AppState, blob: &str) -> AppResult<String> {
    let api_key = crypto::decrypt(&st.key, blob).map_err(|_| {
        AppError::BadRequest("stored API key could not be read; re-enter it in Settings.".into())
    })?;
    check_key_provider(&api_key)?;
    Ok(api_key)
}

// ---------- Display preferences (#72) ----------

pub async fn get_display(State(pool): State<SqlitePool>) -> AppResult<Json<DisplaySettings>> {
    Ok(Json(DisplaySettings {
        breeder_terms: settings::breeder_terms(&pool).await?,
    }))
}

pub async fn put_display(
    State(pool): State<SqlitePool>,
    Json(up): Json<DisplaySettingsUpdate>,
) -> AppResult<Json<DisplaySettings>> {
    if let Some(on) = up.breeder_terms {
        settings::set_breeder_terms(&mut *pool.acquire().await?, on).await?;
    }
    get_display(State(pool)).await
}

/// The configured model settings plus the DECRYPTED API key, or a 400 telling the
/// user to set scanning up. Shared by every endpoint that calls the model
/// (extract, preview, insurance, facility parse, bulk import) — it was copy-pasted
/// four times before #23. `not_set_up` is the feature-specific hint.
pub(crate) async fn ai_key(
    st: &AppState,
    not_set_up: &str,
) -> AppResult<(settings::IngestionConfig, String)> {
    let cfg = settings::load(&st.pool).await?;
    let blob = match (cfg.enabled, &cfg.api_key_enc) {
        (true, Some(blob)) => blob.clone(),
        _ => return Err(AppError::BadRequest(not_set_up.to_string())),
    };
    let api_key = decrypt_key(st, &blob)?;
    Ok((cfg, api_key))
}

/// The provider's model list for `api_key`, from the short-lived cache when this
/// key was listed recently. A refused key is a 400 the person can act on; any
/// other provider failure is a 502 with a sanitised message.
async fn models_for_key(
    st: &AppState,
    api_key: &str,
) -> AppResult<Json<Vec<ingestion::ProviderModel>>> {
    if let Some(models) = st.models_cache.get(api_key) {
        return Ok(Json(models));
    }
    let models = ingestion::list_anthropic_models(&st.http, &st.anthropic_base, api_key)
        .await
        .map_err(|e| match e {
            ingestion::ModelListError::Rejected => AppError::BadRequest(
                "Anthropic rejected this API key. Check it was copied in full and is still active."
                    .into(),
            ),
            ingestion::ModelListError::Failed(m) => AppError::Upstream(m),
        })?;
    st.models_cache.put(api_key, models.clone());
    Ok(Json(models))
}

/// `GET /settings/ingestion/models` (#67): models the STORED key can call,
/// newest first. Works while scanning is switched off, since choosing a model is
/// part of setting it up. The key never leaves the server.
pub async fn list_ingestion_models(
    State(st): State<AppState>,
) -> AppResult<Json<Vec<ingestion::ProviderModel>>> {
    let cfg = settings::load(&st.pool).await?;
    let Some(blob) = cfg.api_key_enc else {
        return Err(AppError::BadRequest(
            "No API key is saved yet. Paste one to load the model list.".into(),
        ));
    };
    let api_key = decrypt_key(&st, &blob)?;
    models_for_key(&st, &api_key).await
}

/// `POST /settings/ingestion/models {api_key}` (#67): the same list for a key
/// that has been pasted but not saved, so the picker fills before Save. Nothing
/// is stored.
pub async fn list_ingestion_models_for_key(
    State(st): State<AppState>,
    Json(req): Json<ModelsForKeyRequest>,
) -> AppResult<Json<Vec<ingestion::ProviderModel>>> {
    let api_key = req.api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::BadRequest("api_key is required".into()));
    }
    check_key_provider(api_key)?;
    models_for_key(&st, api_key).await
}

const SCAN_NOT_SET_UP: &str =
    "Document scanning isn't set up. Add your API key in Settings and enable it.";

/// Reject a document the model can't read (type) or shouldn't be sent (size),
/// before spending any tokens. Returns the decoded bytes.
pub(crate) fn check_document(media_type: &str, data_b64: &str) -> AppResult<Vec<u8>> {
    if !ingestion::media_supported(media_type) {
        return Err(AppError::BadRequest(format!(
            "unsupported file type '{media_type}' (accepts PDF, JPEG, PNG, WebP, GIF)"
        )));
    }
    decode_bounded(data_b64, ingestion::MAX_DECODED_BYTES)
}

/// Distinct `extra` field labels already used across all records, so extraction
/// reuses existing names instead of coining near-duplicates (Jake, #4).
pub(crate) async fn known_field_labels(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let labels: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT je.key FROM medical_records, json_each(medical_records.extra) je \
         WHERE medical_records.extra IS NOT NULL ORDER BY je.key",
    )
    .fetch_all(pool)
    .await?;
    Ok(labels)
}

pub async fn extract(
    State(st): State<AppState>,
    Json(req): Json<ExtractRequest>,
) -> AppResult<Json<ExtractResponse>> {
    let (cfg, api_key) = ai_key(&st, SCAN_NOT_SET_UP).await?;
    check_document(&req.media_type, &req.data_base64)?;
    let known = known_field_labels(&st.pool).await?;

    let fields = ingestion::extract_anthropic(
        &st.http,
        &st.anthropic_base,
        &api_key,
        &cfg.model,
        &req.media_type,
        req.data_base64.trim(),
        &known,
    )
    .await
    .map_err(AppError::Upstream)?;

    Ok(Json(ExtractResponse {
        fields,
        provider: cfg.provider,
        model: cfg.model,
    }))
}

/// Preview/debug (#4 troubleshooting, #5 scriptability): run the model on a
/// document and return the RAW provider response. With `prompt`, runs free-form;
/// otherwise runs the real extraction tool. Same setup/validation gate as
/// `extract`; never saves anything.
pub async fn extract_preview(
    State(st): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> AppResult<Json<Value>> {
    let (cfg, api_key) = ai_key(&st, SCAN_NOT_SET_UP).await?;
    check_document(&req.media_type, &req.data_base64)?;
    let data = req.data_base64.trim();
    let raw = match req.prompt.as_deref() {
        Some(p) if !p.trim().is_empty() => {
            let body = ingestion::build_freeform_request(&cfg.model, &req.media_type, data, p.trim());
            ingestion::call_anthropic(&st.http, &st.anthropic_base, &api_key, &body).await
        }
        // The tool path goes through `call_tool` (retry on a missed call) so the
        // preview shows what `/extract` would actually have parsed.
        _ => {
            let known = known_field_labels(&st.pool).await?;
            let body = ingestion::build_request(&cfg.model, &req.media_type, data, &known);
            ingestion::call_tool(&st.http, &st.anthropic_base, &api_key, &body, ingestion::RECORD_TOOL)
                .await
        }
    }
    .map_err(AppError::Upstream)?;
    Ok(Json(raw))
}

// ---------- Ongoing care: food, preventatives, medications, supplements (#63) ----------

/// Bound and normalise a labelled list (allergies today; the same shape backs a
/// contact's requirements/hours). Blank labels are dropped and an empty result
/// becomes `None`, so the column stores NULL rather than "[]".
fn clean_labeled(field: &str, items: Option<Vec<LabeledDetail>>) -> AppResult<Option<Vec<LabeledDetail>>> {
    let Some(items) = items else { return Ok(None) };
    let mut out = Vec::new();
    for it in items {
        let Some(label) = bounded_text(field, Some(it.label))? else { continue };
        let detail = bounded_text(field, it.detail)?;
        out.push(LabeledDetail { label, detail });
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

/// Trim a client string, mapping blank to `None`. A blank string is the SAME
/// thing as an omitted value everywhere in this API — without this, `""` reaches
/// the database as a non-NULL value and every `IS NOT NULL` test disagrees with
/// the frontend's falsy test. That exact split made a care entry sort as
/// "stopped" in SQL while the profile rendered it as current.
pub(crate) fn blank_to_none(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Longest accepted free-text field. The global body cap is 40 MiB, which a
/// single `product` or `notes` string can fill on its own and then bloat every
/// pet-detail response forever; nothing a household types is near this.
const MAX_TEXT_LEN: usize = 4_000;

/// Reject an absurd free-text value (see [`MAX_TEXT_LEN`]) and normalise blanks.
pub(crate) fn bounded_text(field: &str, v: Option<String>) -> AppResult<Option<String>> {
    if let Some(s) = v.as_deref() {
        if s.len() > MAX_TEXT_LEN {
            return Err(AppError::BadRequest(format!(
                "{field} is too long ({} bytes, max {MAX_TEXT_LEN})",
                s.len()
            )));
        }
    }
    Ok(blank_to_none(v))
}

/// Validate an ISO `YYYY-MM-DD` date, normalising blank to `None`. Accepts only
/// real calendar dates: `"2026-02-31"` is rejected here rather than being stored
/// and silently rolled forward to March by the browser's `Date`.
pub(crate) fn iso_date(field: &str, v: Option<String>) -> AppResult<Option<String>> {
    let Some(s) = blank_to_none(v) else {
        return Ok(None);
    };
    let bad = || AppError::BadRequest(format!("{field} must be an ISO date (YYYY-MM-DD), got '{s}'"));
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().map_err(|_| bad());
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&m) || d == 0 {
        return Err(bad());
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    if d > days {
        return Err(bad());
    }
    Ok(Some(s))
}

pub(crate) fn validate_regimen_kind(kind: &str) -> AppResult<()> {
    if REGIMEN_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "unknown care kind '{kind}' (expected one of: {})",
            REGIMEN_KINDS.join(", ")
        )))
    }
}

/// Normalise and validate what a preventative protects against. Values are
/// lower-cased and de-duplicated so `["Flea","flea"]` can't store the same
/// protection twice and defeat the "is the pet covered for X" query. An empty
/// list becomes `None`, which stores NULL rather than "[]".
///
/// Protections are dropped for any kind but `preventative`. A bag of kibble
/// that claims to protect against heartworm would be reported as the pet's
/// current heartworm cover by `currentProtection()` on the client, which is the
/// exact question a vet intake form asks — so the SERVER enforces it, not just
/// the form that happens to hide the checkboxes.
pub(crate) fn clean_protects(kind: &str, protects: Option<Vec<String>>) -> AppResult<Option<Vec<String>>> {
    if kind != "preventative" {
        return Ok(None);
    }
    let Some(list) = protects else {
        return Ok(None);
    };
    let mut out: Vec<String> = Vec::new();
    for p in list {
        let p = p.trim().to_lowercase();
        if p.is_empty() {
            continue;
        }
        if !PROTECTIONS.contains(&p.as_str()) {
            return Err(AppError::BadRequest(format!(
                "unknown protection '{p}' (expected one of: {})",
                PROTECTIONS.join(", ")
            )));
        }
        if !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

async fn load_regimen(pool: &SqlitePool, rid: &str) -> AppResult<Regimen> {
    sqlx::query_as::<_, Regimen>("SELECT * FROM pet_regimens WHERE id = ?")
        .bind(rid)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no care entry '{rid}'")))
}

/// A pet's ongoing care, current entries first (same order as `PetDetail`).
pub async fn list_pet_regimens(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Regimen>>> {
    load_pet(&pool, &id).await?;
    let regimens = sqlx::query_as::<_, Regimen>(
        "SELECT * FROM pet_regimens WHERE pet_id = ? ORDER BY \
           (ended_on IS NOT NULL), \
           CASE kind WHEN 'food' THEN 0 WHEN 'preventative' THEN 1 \
                     WHEN 'medication' THEN 2 ELSE 3 END, \
           product COLLATE NOCASE",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(regimens))
}

pub async fn create_regimen(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(nr): Json<NewRegimen>,
) -> AppResult<(StatusCode, Json<Regimen>)> {
    load_pet(&pool, &id).await?;
    if nr.product.trim().is_empty() {
        return Err(AppError::BadRequest("product is required".into()));
    }
    validate_regimen_kind(&nr.kind)?;
    let protects = clean_protects(&nr.kind, nr.protects)?.map(SqlxJson);
    let product = bounded_text("product", Some(nr.product))?
        .ok_or_else(|| AppError::BadRequest("product is required".into()))?;
    let amount = bounded_text("amount", nr.amount)?;
    let frequency = bounded_text("frequency", nr.frequency)?;
    let notes = bounded_text("notes", nr.notes)?;
    let started_on = iso_date("started_on", nr.started_on)?;
    let ended_on = iso_date("ended_on", nr.ended_on)?;
    let rid = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pet_regimens \
         (id, pet_id, kind, product, amount, frequency, protects, started_on, ended_on, notes, \
          created_by, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?, datetime('now'), datetime('now'))",
    )
    .bind(&rid)
    .bind(&id)
    .bind(&nr.kind)
    .bind(&product)
    .bind(&amount)
    .bind(&frequency)
    .bind(protects)
    .bind(&started_on)
    .bind(&ended_on)
    .bind(&notes)
    .bind(&nr.created_by)
    .execute(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(load_regimen(&pool, &rid).await?)))
}

pub async fn get_regimen(
    State(pool): State<SqlitePool>,
    Path(rid): Path<String>,
) -> AppResult<Json<Regimen>> {
    Ok(Json(load_regimen(&pool, &rid).await?))
}

/// Edit a care entry. `kind`/`product` are set-or-keep; the rest are
/// null-vs-absent. Clearing `ended_on` puts the pet back ON something.
///
/// Protections are re-derived from the RESULTING kind: switching a preventative
/// to a food drops them, so a bag of kibble can never be reported as the pet's
/// current heartworm cover. That needs the current row, so unlike `update_pet`
/// this one reads first — but it re-reads inside the same statement for every
/// column it does not own, and only `protects` depends on the snapshot.
pub async fn update_regimen(
    State(pool): State<SqlitePool>,
    Path(rid): Path<String>,
    Json(up): Json<UpdateRegimen>,
) -> AppResult<Json<Regimen>> {
    let cur = load_regimen(&pool, &rid).await?;
    let kind = match up.kind {
        Some(k) => {
            validate_regimen_kind(&k)?;
            k
        }
        None => cur.kind,
    };
    let product = match up.product {
        Some(pr) if pr.trim().is_empty() => {
            return Err(AppError::BadRequest("product cannot be empty".into()))
        }
        other => bounded_text("product", other)?,
    };
    let amount = up.amount.map(|v| bounded_text("amount", v)).transpose()?;
    let frequency = up
        .frequency
        .map(|v| bounded_text("frequency", v))
        .transpose()?;
    let notes = up.notes.map(|v| bounded_text("notes", v)).transpose()?;
    let started_on = up
        .started_on
        .map(|v| iso_date("started_on", v))
        .transpose()?;
    let ended_on = up.ended_on.map(|v| iso_date("ended_on", v)).transpose()?;
    // Absent → keep what the row has, then re-filter by the RESULTING kind.
    let protects = match up.protects {
        Some(v) => v,
        None => cur.protects.map(|j| j.0),
    };
    let protects = clean_protects(&kind, protects)?.map(SqlxJson);

    sqlx::query(
        "UPDATE pet_regimens SET \
           kind = ?, \
           product = COALESCE(?, product), \
           amount = CASE WHEN ? THEN ? ELSE amount END, \
           frequency = CASE WHEN ? THEN ? ELSE frequency END, \
           protects = ?, \
           started_on = CASE WHEN ? THEN ? ELSE started_on END, \
           ended_on = CASE WHEN ? THEN ? ELSE ended_on END, \
           notes = CASE WHEN ? THEN ? ELSE notes END, \
           updated_at = datetime('now') \
         WHERE id = ?",
    )
    .bind(&kind)
    .bind(&product)
    .bind(amount.is_some())
    .bind(amount.flatten())
    .bind(frequency.is_some())
    .bind(frequency.flatten())
    .bind(protects)
    .bind(started_on.is_some())
    .bind(started_on.flatten())
    .bind(ended_on.is_some())
    .bind(ended_on.flatten())
    .bind(notes.is_some())
    .bind(notes.flatten())
    .bind(&rid)
    .execute(&pool)
    .await?;
    Ok(Json(load_regimen(&pool, &rid).await?))
}

pub async fn delete_regimen(
    State(pool): State<SqlitePool>,
    Path(rid): Path<String>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM pet_regimens WHERE id = ?")
        .bind(&rid)
        .execute(&pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("no care entry '{rid}'")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Pet portrait photos (#21) ----------

/// Where portraits live under the shared attachments volume: `photos/<pet_id>/`.
/// `photos` is never a valid record-id directory, so it can't collide with the
/// attachment tree that sits beside it.
const PHOTO_SUBDIR: &str = "photos";

/// A cropped square portrait is small; cap well under the attachment limit so a
/// stray full-resolution upload can't bloat the volume.
const MAX_PHOTO_BYTES: usize = 10 * 1024 * 1024;

/// Accepted portrait image types → the file extension we store them under.
/// Images only (no PDF): a portrait is always a picture. Returns None for
/// anything unsupported.
fn photo_ext(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

/// Does `b` start with the file signature of the `ext` format? The media type
/// is client-supplied, so without this a non-image (or a mislabelled one) is
/// stored and later served as an image the browser can't draw.
fn has_image_signature(ext: &str, b: &[u8]) -> bool {
    match ext {
        "jpg" => b.starts_with(&[0xFF, 0xD8, 0xFF]),
        "png" => b.starts_with(b"\x89PNG\r\n\x1a\n"),
        "gif" => b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a"),
        "webp" => b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP",
        _ => false,
    }
}

/// Validate and decode an uploaded portrait image: a supported type, valid
/// base64 under [`MAX_PHOTO_BYTES`], and bytes that really are that type.
/// Returns the stored extension and the bytes. Shared by pet portraits (#21)
/// and the household portrait (#75) so both accept exactly the same files.
pub(crate) fn decode_image(media_type: &str, data_b64: &str) -> AppResult<(&'static str, Vec<u8>)> {
    let ext = photo_ext(media_type).ok_or_else(|| {
        AppError::BadRequest(format!(
            "unsupported image type '{media_type}' (accepts JPEG, PNG, WebP, GIF)"
        ))
    })?;
    let bytes = decode_bounded(data_b64, MAX_PHOTO_BYTES)?;
    if !has_image_signature(ext, &bytes) {
        return Err(AppError::BadRequest(format!(
            "the file is not a valid {media_type} image"
        )));
    }
    Ok((ext, bytes))
}

/// Recover the content type for a stored portrait from its file extension, so a
/// GET serves the bytes with the right MIME. Defaults to JPEG (the client's
/// canvas export format).
fn photo_media_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

/// Set (or replace) a pet's portrait. Stores the cropped bytes on the volume,
/// unlinks any prior photo, and records the path on the reserved `photo_path`
/// column. One portrait per pet — PUT replaces in place.
pub async fn set_pet_photo(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(np): Json<NewPetPhoto>,
) -> AppResult<Json<Pet>> {
    let pet = load_pet(&st.pool, &id).await?;
    let (ext, bytes) = decode_image(&np.media_type, &np.data_base64)?;

    let file_id = uuid::Uuid::new_v4();
    let rel = format!("{PHOTO_SUBDIR}/{id}/{file_id}.{ext}");
    write_image(&st, &rel, &bytes)?;

    sqlx::query("UPDATE pets SET photo_path = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&rel)
        .bind(&id)
        .execute(&st.pool)
        .await?;

    // Remove the previous file only after the new one is safely recorded, so a
    // failure never leaves the pet pointing at deleted bytes.
    if let Some(old) = pet.photo_path.as_deref() {
        if old != rel {
            let _ = std::fs::remove_file(st.attachments_dir.join(old));
        }
    }
    Ok(Json(load_pet(&st.pool, &id).await?))
}

/// Stream a pet's portrait bytes. 404 if the pet has no photo set. The stable
/// per-pet URL means the frontend derives it from the pet alone; a `?v=` cache
/// buster on the client side handles replacement.
pub async fn get_pet_photo(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let pet = load_pet(&st.pool, &id).await?;
    let rel = pet
        .photo_path
        .ok_or_else(|| AppError::NotFound(format!("pet '{id}' has no photo")))?;
    serve_image(&st, &rel)
}

/// Write a stored image under the attachments volume, creating its directory.
/// Shared by pet portraits and the household portrait (#75); callers record the
/// new path BEFORE unlinking the old file, so nothing points at deleted bytes.
pub(crate) fn write_image(st: &AppState, rel: &str, bytes: &[u8]) -> AppResult<()> {
    let full = st.attachments_dir.join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("could not store photo: {e}")))?;
    }
    std::fs::write(&full, bytes)
        .map_err(|e| AppError::Internal(format!("could not store photo: {e}")))
}

/// Stream a stored image with the content type its extension implies. 404 when
/// the row names a file that is no longer on the volume.
pub(crate) fn serve_image(st: &AppState, rel: &str) -> AppResult<Response> {
    let bytes = std::fs::read(st.attachments_dir.join(rel))
        .map_err(|_| AppError::NotFound("photo file is missing".into()))?;
    let ext = std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    Ok((
        [(header::CONTENT_TYPE, photo_media_type(ext))],
        Body::from(bytes),
    )
        .into_response())
}

/// Clear a pet's portrait: unlink the file, prune the now-empty dir, and null
/// out `photo_path` so the UI falls back to the gradient placeholder.
pub async fn delete_pet_photo(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Pet>> {
    let pet = load_pet(&st.pool, &id).await?;
    if let Some(old) = pet.photo_path.as_deref() {
        let full = st.attachments_dir.join(old);
        let _ = std::fs::remove_file(&full);
        if let Some(parent) = full.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    sqlx::query("UPDATE pets SET photo_path = NULL, updated_at = datetime('now') WHERE id = ?")
        .bind(&id)
        .execute(&st.pool)
        .await?;
    Ok(Json(load_pet(&st.pool, &id).await?))
}

// ---------- Attachments (#4) ----------

pub(crate) const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

/// Decode a base64 upload, rejecting invalid/empty data (400) and enforcing a
/// size cap (413). Shared by the extract and attachment-upload paths.
pub(crate) fn decode_bounded(data_b64: &str, max: usize) -> AppResult<Vec<u8>> {
    let bytes = B64
        .decode(data_b64.trim())
        .map_err(|_| AppError::BadRequest("file data is not valid base64".into()))?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("file is empty".into()));
    }
    if bytes.len() > max {
        return Err(AppError::TooLarge(format!(
            "file is too large ({} MB); max is {} MB",
            bytes.len() / (1024 * 1024),
            max / (1024 * 1024)
        )));
    }
    Ok(bytes)
}

async fn record_exists(pool: &SqlitePool, rid: i64) -> AppResult<()> {
    let found: Option<i64> = sqlx::query_scalar("SELECT id FROM medical_records WHERE id = ?")
        .bind(rid)
        .fetch_optional(pool)
        .await?;
    found
        .map(|_| ())
        .ok_or_else(|| AppError::NotFound(format!("no record {rid}")))
}

/// Keep only path-safe characters from the original filename, for the on-disk
/// name and the download disposition. The stored bytes are addressed by a uuid,
/// so this is cosmetic, but it prevents path traversal via the filename.
pub(crate) fn safe_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "file".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Hex SHA-256 of a file's bytes: the content fingerprint bulk import (#23) uses
/// to recognise a document that is already in the library, whatever it is named.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Write `bytes` under the record's attachment dir and insert the metadata row,
/// fingerprint included. Shared by the single-file upload and bulk import (which
/// attaches one document to every record it produced without re-decoding it).
pub(crate) async fn store_attachment(
    st: &AppState,
    rid: i64,
    filename: &str,
    media_type: &str,
    bytes: &[u8],
    source: &str,
) -> AppResult<Attachment> {
    let id = uuid::Uuid::new_v4().to_string();
    let display = safe_filename(filename);
    // Grouped by record and made unique by the uuid prefix.
    let rel = format!("{rid}/{id}-{display}");
    let full = st.attachments_dir.join(&rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("could not store file: {e}")))?;
    }
    std::fs::write(&full, bytes)
        .map_err(|e| AppError::Internal(format!("could not store file: {e}")))?;

    let source = if source == "scan" { "scan" } else { "upload" };
    sqlx::query(
        "INSERT INTO attachments (id, record_id, filename, media_type, size_bytes, stored_path, source, sha256, created_at) \
         VALUES (?,?,?,?,?,?,?,?, datetime('now'))",
    )
    .bind(&id)
    .bind(rid)
    .bind(filename)
    .bind(media_type)
    .bind(bytes.len() as i64)
    .bind(&rel)
    .bind(source)
    .bind(sha256_hex(bytes))
    .execute(&st.pool)
    .await?;

    Ok(sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE id = ?")
        .bind(&id)
        .fetch_one(&st.pool)
        .await?)
}

/// Fill in the fingerprint for attachments stored before migration 0015, by
/// hashing the bytes already on the volume. Runs at startup; a row whose file is
/// missing stays NULL (it can't be matched, and nothing else depends on it).
pub async fn backfill_attachment_hashes(st: &AppState) -> AppResult<usize> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, stored_path FROM attachments WHERE sha256 IS NULL")
            .fetch_all(&st.pool)
            .await?;
    let mut filled = 0;
    for (id, rel) in rows {
        let Ok(bytes) = std::fs::read(st.attachments_dir.join(&rel)) else { continue };
        sqlx::query("UPDATE attachments SET sha256 = ? WHERE id = ?")
            .bind(sha256_hex(&bytes))
            .bind(&id)
            .execute(&st.pool)
            .await?;
        filled += 1;
    }
    Ok(filled)
}

pub async fn create_attachment(
    State(st): State<AppState>,
    Path(rid): Path<i64>,
    Json(na): Json<NewAttachment>,
) -> AppResult<(StatusCode, Json<Attachment>)> {
    record_exists(&st.pool, rid).await?;
    let bytes = decode_bounded(&na.data_base64, MAX_ATTACHMENT_BYTES)?;
    let att = store_attachment(&st, rid, &na.filename, &na.media_type, &bytes, &na.source).await?;
    Ok((StatusCode::CREATED, Json(att)))
}

pub async fn list_attachments(
    State(st): State<AppState>,
    Path(rid): Path<i64>,
) -> AppResult<Json<Vec<Attachment>>> {
    record_exists(&st.pool, rid).await?;
    let atts = sqlx::query_as::<_, Attachment>(
        "SELECT * FROM attachments WHERE record_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(rid)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(atts))
}

pub async fn get_attachment(
    State(st): State<AppState>,
    Path(aid): Path<String>,
) -> AppResult<Response> {
    let att = sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE id = ?")
        .bind(&aid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no attachment '{aid}'")))?;
    let bytes = std::fs::read(st.attachments_dir.join(&att.stored_path))
        .map_err(|_| AppError::NotFound("attachment file is missing".into()))?;
    let disposition = format!("inline; filename=\"{}\"", safe_filename(&att.filename));
    Ok((
        [
            (header::CONTENT_TYPE, att.media_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from(bytes),
    )
        .into_response())
}

// ---------- Diagnostics capture (#41) ----------

/// Cap on an attached diagnostic file, and how many reports to retain. Bounds
/// the ephemeral store so a burst can't fill the tmpfs.
const MAX_DIAG_FILE_BYTES: usize = 30 * 1024 * 1024;
const DIAG_KEEP: usize = 25;

/// Accept a client error report (and optionally the file that triggered it),
/// enrich it with the app version + request user-agent, and stash it in the
/// ephemeral store so a client-side failure (e.g. HEIC decode on a phone) can be
/// pulled and reproduced. Best-effort from the client's view; a bad file 413s.
pub async fn create_diag(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(nd): Json<NewDiag>,
) -> AppResult<(StatusCode, Json<DiagReport>)> {
    let received_at: String = sqlx::query_scalar("SELECT datetime('now')")
        .fetch_one(&st.pool)
        .await?;
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let (file_meta, bytes) = match &nd.file {
        Some(f) => {
            let b = decode_bounded(&f.data_base64, MAX_DIAG_FILE_BYTES)?;
            let meta = DiagFileMeta {
                filename: f.filename.clone(),
                media_type: f.media_type.clone(),
                size_bytes: b.len() as i64,
            };
            (Some(meta), Some(b))
        }
        None => (None, None),
    };

    let report = DiagReport {
        id: uuid::Uuid::new_v4().to_string(),
        received_at,
        app_version: VERSION.to_string(),
        user_agent,
        context: nd.context,
        message: nd.message,
        stack: nd.stack,
        url: nd.url,
        extra: nd.extra,
        file: file_meta,
    };
    diag::store(&st.diag_dir, &report, bytes.as_deref())
        .map_err(|e| AppError::Internal(format!("could not store diagnostic: {e}")))?;
    diag::prune(&st.diag_dir, DIAG_KEEP);
    Ok((StatusCode::CREATED, Json(report)))
}

pub async fn list_diag(State(st): State<AppState>) -> Json<Vec<DiagReport>> {
    Json(diag::list(&st.diag_dir))
}

pub async fn get_diag(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<DiagReport>> {
    diag::load(&st.diag_dir, &id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("no diagnostic '{id}'")))
}

pub async fn get_diag_file(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let (meta, bytes) = diag::load_file(&st.diag_dir, &id)
        .ok_or_else(|| AppError::NotFound(format!("no diagnostic file for '{id}'")))?;
    let disposition = format!("inline; filename=\"{}\"", safe_filename(&meta.filename));
    Ok((
        [
            (header::CONTENT_TYPE, meta.media_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from(bytes),
    )
        .into_response())
}

pub async fn clear_diag(State(st): State<AppState>) -> StatusCode {
    diag::clear(&st.diag_dir);
    StatusCode::NO_CONTENT
}

/// Fallback for unmatched `/api/*` paths. Without it, an unknown API route falls
/// through to the SPA fallback and returns `index.html` (200) — so a mistyped or
/// removed endpoint hands an API client HTML instead of an honest JSON 404 (and,
/// being a 200, it stays invisible to the error-logging middleware).
pub async fn api_not_found(uri: axum::http::Uri) -> AppError {
    AppError::NotFound(format!("no such API route: {}", uri.path()))
}

pub async fn delete_attachment(
    State(st): State<AppState>,
    Path(aid): Path<String>,
) -> AppResult<StatusCode> {
    let att = sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE id = ?")
        .bind(&aid)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no attachment '{aid}'")))?;
    sqlx::query("DELETE FROM attachments WHERE id = ?")
        .bind(&aid)
        .execute(&st.pool)
        .await?;
    let full = st.attachments_dir.join(&att.stored_path);
    let _ = std::fs::remove_file(&full);
    // Prune the per-record dir if this was its last file (no-op otherwise).
    if let Some(parent) = full.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(StatusCode::NO_CONTENT)
}
