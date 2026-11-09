//! Commit: validate EVERYTHING first, then write each selected item through
//! the existing handlers, idempotently (see `commit`).

use super::*;

/// An old policy is ended only alongside its replacement: the policy it names
/// exists in this plan, is for the SAME pet (a replacement re-assigned to
/// another pet must not end this pet's policy), and is being saved or already
/// was. Anything else is left alone rather than failing the whole import.
fn replacement_kept(plan: &ImportPlan, e: &PlannedPolicyEnd) -> bool {
    plan.policies.get(e.replaced_by).is_some_and(|p| {
        p.pet_id.as_deref() == Some(e.pet_id.as_str()) && (p.selected || p.existing_id.is_some())
    })
}

/// Attach the document to a record this commit just created; if the bytes can't
/// be stored, delete the record again rather than leave a visit without its
/// source that a retry can't recognise (and would create a second time).
async fn attach_or_undo(
    st: &AppState,
    pool: &SqlitePool,
    rec: &MedicalRecord,
    filename: &str,
    media_type: &str,
    bytes: &[u8],
) -> AppResult<()> {
    if let Err(e) = handlers::store_attachment(st, rec.id, filename, media_type, bytes, "scan").await {
        sqlx::query("DELETE FROM medical_records WHERE id = ?")
            .bind(rec.id)
            .execute(pool)
            .await?;
        return Err(e);
    }
    Ok(())
}

async fn weight_exists(pool: &SqlitePool, pet_id: &str, date: &str) -> AppResult<bool> {
    let hit: Option<i64> =
        sqlx::query_scalar("SELECT id FROM weight_entries WHERE pet_id = ? AND recorded_on = ? LIMIT 1")
            .bind(pet_id)
            .bind(date)
            .fetch_optional(pool)
            .await?;
    Ok(hit.is_some())
}

/// An entry for this product at this dose, frequency and end already exists
/// (current or ended: a retried commit of an ended course must not duplicate it).
async fn regimen_exists(
    pool: &SqlitePool,
    pet_id: &str,
    product: &str,
    amount: Option<&str>,
    frequency: Option<&str>,
    ended_on: Option<&str>,
) -> AppResult<bool> {
    let theirs: Vec<Regimen> = sqlx::query_as("SELECT * FROM pet_regimens WHERE pet_id = ?")
        .bind(pet_id)
        .fetch_all(pool)
        .await?;
    let same = |a: Option<&str>, b: Option<&str>| a.map(name_key) == b.map(name_key);
    Ok(theirs.iter().any(|r| {
        name_key(&r.product) == name_key(product)
            && same(r.amount.as_deref(), amount)
            && same(r.frequency.as_deref(), frequency)
            && r.ended_on.as_deref() == ended_on
    }))
}

async fn claim_recorded(pool: &SqlitePool, pet_id: &str, key: &str) -> AppResult<bool> {
    let hit: Option<i64> = sqlx::query_scalar(
        "SELECT r.id FROM medical_records r, json_each(r.extra) je \
         WHERE r.pet_id = ? AND r.extra IS NOT NULL AND upper(je.key) = upper(?) LIMIT 1",
    )
    .bind(pet_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(hit.is_some())
}

async fn policy_exists(
    pool: &SqlitePool,
    pet_id: &str,
    provider: &str,
    kind: &str,
    number: Option<&str>,
    start: Option<&str>,
) -> AppResult<bool> {
    let theirs: Vec<InsurancePolicy> = sqlx::query_as("SELECT * FROM insurance_policies WHERE pet_id = ?")
        .bind(pet_id)
        .fetch_all(pool)
        .await?;
    Ok(theirs.iter().any(|e| same_policy(e, provider, kind, number, start)))
}

fn cents(c: i64) -> String {
    format!("${}.{:02}", c / 100, c % 100)
}

/// Between a claim's payout and its policy number in [`claim_value`]. Insurance
/// value (#74) splits on it to credit the payout to that policy.
pub const CLAIM_POLICY_SEP: &str = " · policy ";

/// What a claim adds to its visit's custom fields: "$177.12 paid · policy …".
pub fn claim_value(c: &PlannedClaim) -> String {
    let mut v = format!("{} paid", cents(c.paid_cents));
    if let Some(p) = &c.policy_number {
        v.push_str(&format!("{CLAIM_POLICY_SEP}{p}"));
    }
    v
}

/// A visit recorded from a claim alone is titled by what was treated.
pub fn claim_title(c: &PlannedClaim) -> String {
    let items: Vec<&str> = c.items.iter().map(|i| i.description.as_str()).collect();
    if items.is_empty() {
        format!("Insurance claim {}", c.claim_number)
    } else {
        items.join(", ")
    }
}

/// The claim's itemised decision as markdown, for a visit recorded from it.
pub fn claim_notes(c: &PlannedClaim) -> String {
    let mut n = format!(
        "Recorded from {} claim {}{}.\n",
        c.insurer.as_deref().unwrap_or("an insurance"),
        c.claim_number,
        c.policy_number.as_deref().map(|p| format!(" (policy {p})")).unwrap_or_default()
    );
    for i in &c.items {
        n.push_str(&format!(
            "\n- {}: {} billed, {} covered{}",
            i.description,
            i.cost_cents.map(cents).unwrap_or_else(|| "?".into()),
            i.covered_cents.map(cents).unwrap_or_else(|| "?".into()),
            i.note.as_deref().map(|x| format!(" ({x})")).unwrap_or_default()
        ));
    }
    n
}

/// The id of a contact with exactly this name (case-insensitive), if any.
async fn existing_contact_named(pool: &SqlitePool, name: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT id FROM contacts WHERE lower(name) = lower(?) LIMIT 1")
        .bind(name.trim())
        .fetch_optional(pool)
        .await?)
}

/// Whether this pet already has a record with this date and title that carries
/// a file with these bytes — i.e. this commit already wrote it.
async fn already_imported(pool: &SqlitePool, pet_id: &str, r: &PlannedRecord, sha256: &str) -> AppResult<bool> {
    // Cost and notes too (NULL-safe `IS`): two same-day "Office visit"s in one
    // file are two visits, priced or not, and the second must not be skipped as
    // the first.
    let hit: Option<i64> = sqlx::query_scalar(
        "SELECT r.id FROM medical_records r JOIN attachments a ON a.record_id = r.id \
         WHERE r.pet_id = ? AND r.occurred_on = ? AND lower(r.title) = lower(?) \
           AND r.cost_cents IS ? AND r.notes IS ? AND a.sha256 = ? LIMIT 1",
    )
    .bind(pet_id)
    .bind(&r.occurred_on)
    .bind(r.title.trim())
    .bind(r.cost_cents)
    .bind(&r.notes)
    .bind(sha256)
    .fetch_optional(pool)
    .await?;
    Ok(hit.is_some())
}


/// Check every selected item BEFORE writing anything, so a bad edit is a clean
/// 400 instead of a half-imported document.
async fn validate_commit(pool: &SqlitePool, plan: &ImportPlan, sha: &str) -> AppResult<()> {
    // The plan describes the file it was made from; committing it with other
    // bytes would file document A's facts under document B.
    // Required: a hand-built plan with an empty fingerprint would otherwise
    // pair any file with any facts.
    if plan.sha256 != sha {
        return Err(AppError::BadRequest(
            "this plan was made from a different file; read the document again".into(),
        ));
    }
    let stale = |what: &str| {
        AppError::BadRequest(format!("{what} changed since this document was read; read it again"))
    };
    let pets: HashSet<String> = sqlx::query_scalar("SELECT id FROM pets")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
    let pet = |id: &str, what: &str| -> AppResult<()> {
        if pets.contains(id) {
            Ok(())
        } else {
            Err(AppError::BadRequest(format!("{what}: no pet '{id}'")))
        }
    };
    for (i, r) in plan.records.iter().enumerate().filter(|(_, r)| r.selected) {
        let what = format!("record {} ('{}')", i + 1, r.title);
        let pid = r
            .pet_id
            .as_deref()
            .ok_or_else(|| AppError::BadRequest(format!("{what}: choose which pet it belongs to")))?;
        pet(pid, &what)?;
        if r.title.trim().is_empty() {
            return Err(AppError::BadRequest(format!("record {}: a title is required", i + 1)));
        }
        handlers::iso_date(&format!("{what} date"), Some(r.occurred_on.clone()))?
            .ok_or_else(|| AppError::BadRequest(format!("{what}: a date is required")))?;
        handlers::iso_date(&format!("{what} end date"), r.ends_on.clone())?;
    }
    if let Some(c) = plan.clinic.as_ref().filter(|c| c.selected) {
        match c.contact_id.as_deref() {
            Some(cid) => handlers::contact_exists(pool, cid).await?,
            None => {
                if c.name.trim().is_empty() {
                    return Err(AppError::BadRequest("the new clinic needs a name".into()));
                }
                handlers::validate_role(&c.role)?;
            }
        }
    }
    for l in plan.care_team.iter().filter(|l| l.selected) {
        pet(&l.pet_id, "care team")?;
    }
    for l in plan.clinic_links.iter().filter(|l| l.selected) {
        let owner: Option<String> = sqlx::query_scalar("SELECT pet_id FROM medical_records WHERE id = ?")
            .bind(l.record_id)
            .fetch_optional(pool)
            .await?;
        if owner.as_deref() != Some(l.pet_id.as_str()) {
            return Err(stale("a visit"));
        }
    }
    for w in plan.weights.iter().filter(|w| w.selected) {
        pet(&w.pet_id, "weight")?;
        handlers::iso_date("weight date", Some(w.recorded_on.clone()))?;
        handlers::check_weight(w.value, &w.unit)?;
        // A correction overwrites; only overwrite the value the person saw.
        if let Some(wid) = w.existing_id {
            let now: Option<(f64, String, String)> = sqlx::query_as(
                "SELECT value, unit, recorded_on FROM weight_entries WHERE id = ? AND pet_id = ?",
            )
            .bind(wid)
            .bind(&w.pet_id)
            .fetch_optional(pool)
            .await?;
            let same = |v: f64, u: &str, a: f64, au: Option<&str>| (v - a).abs() < 0.005 && au == Some(u);
            match (now, w.existing_value) {
                // As seen at analysis, or already corrected by an earlier try.
                (Some((v, u, d)), Some(seen))
                    if d == w.recorded_on
                        && (same(v, &u, seen, w.existing_unit.as_deref())
                            || same(v, &u, w.value, Some(w.unit.as_str()))) => {}
                _ => return Err(stale("a weight")),
            }
        }
    }
    for c in plan.completed_reminders.iter().filter(|c| c.selected) {
        // Same pet, title and due date as reviewed: an id edited into another
        // task since must not be marked done as this one.
        let now: Option<(String, String, String)> =
            sqlx::query_as("SELECT pet_id, title, due_on FROM reminders WHERE id = ?")
                .bind(c.reminder_id)
                .fetch_optional(pool)
                .await?;
        if now.as_ref().map(|(p, t, d)| (p.as_str(), t.as_str(), d.as_str()))
            != Some((c.pet_id.as_str(), c.title.as_str(), c.due_on.as_str()))
        {
            return Err(stale("a reminder"));
        }
    }
    for r in plan.reminders.iter().filter(|r| r.selected) {
        pet(&r.pet_id, "reminder")?;
        handlers::iso_date("reminder due date", Some(r.due_on.clone()))?;
        if r.title.trim().is_empty() {
            return Err(AppError::BadRequest("a reminder needs a title".into()));
        }
    }
    for u in plan.pet_updates.iter().filter(|u| u.selected) {
        pet(&u.pet_id, "profile update")?;
        if !PET_UPDATE_FIELDS.contains(&u.field.as_str()) {
            return Err(AppError::BadRequest(format!("can't update pet field '{}'", u.field)));
        }
        if u.field == "birth_date" {
            handlers::iso_date("birth_date", Some(u.value.clone()))?;
        }
        if u.field == "sex" {
            sex::validate_sex(Some(u.value.clone()))?;
        }
        if u.field == "name" && u.value.trim().is_empty() {
            return Err(AppError::BadRequest("name cannot be empty".into()));
        }
        // Only fill/overwrite the value the person saw when they ticked it.
        // One literal statement per allowed field (PET_UPDATE_FIELDS above), so
        // no SQL is ever assembled from input; sqlx 0.9 also refuses a built
        // string here without an explicit audit.
        let sql = match u.field.as_str() {
            "name" => "SELECT name FROM pets WHERE id = ?",
            "breed" => "SELECT breed FROM pets WHERE id = ?",
            "sex" => "SELECT sex FROM pets WHERE id = ?",
            "color" => "SELECT color FROM pets WHERE id = ?",
            "microchip" => "SELECT microchip FROM pets WHERE id = ?",
            _ => "SELECT birth_date FROM pets WHERE id = ?",
        };
        let now: Option<String> = sqlx::query_scalar(sql)
            .bind(&u.pet_id)
            .fetch_one(pool)
            .await?;
        let norm = |v: Option<&str>| v.map(str::trim).filter(|v| !v.is_empty()).map(str::to_lowercase);
        // Unchanged since analysis, or already set to this value by an earlier
        // try of this same commit (a retry must not fail on its own write).
        let now = norm(now.as_deref());
        if now != norm(u.current.as_deref()) && now != norm(Some(u.value.as_str())) {
            return Err(stale(&format!("this pet's {}", u.field.replace('_', " "))));
        }
    }
    for r in plan.regimens.iter().filter(|r| r.selected) {
        pet(&r.pet_id, "ongoing care")?;
        handlers::validate_regimen_kind(&r.kind)?;
        handlers::clean_protects(&r.kind, Some(r.protects.clone()))?;
        handlers::iso_date("started_on", r.started_on.clone())?;
        handlers::iso_date("ended_on", r.ended_on.clone())?;
        if r.product.trim().is_empty() {
            return Err(AppError::BadRequest("an ongoing-care entry needs a product".into()));
        }
    }
    for c in plan.claims.iter().filter(|c| c.selected) {
        let pid = c
            .pet_id
            .as_deref()
            .ok_or_else(|| AppError::BadRequest(format!("claim {}: choose which pet", c.claim_number)))?;
        pet(pid, "claim")?;
        handlers::iso_date("claim treatment date", Some(c.treatment_date.clone()))?;
        if c.claim_number.trim().is_empty() || c.paid_cents < 0 {
            return Err(AppError::BadRequest("a claim needs a number and a paid amount".into()));
        }
        if let Some(rid) = c.record_id {
            let now: Option<(String, String)> =
                sqlx::query_as("SELECT pet_id, occurred_on FROM medical_records WHERE id = ?")
                    .bind(rid)
                    .fetch_optional(pool)
                    .await?;
            if now.as_ref().map(|(p, d)| (p.as_str(), d.as_str())) != Some((pid, c.treatment_date.as_str())) {
                return Err(stale("the visit this claim belongs to"));
            }
        }
    }
    for p in plan.policies.iter().filter(|p| p.selected) {
        let pid = p
            .pet_id
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("a policy needs a pet".into()))?;
        pet(pid, "policy")?;
        if p.provider.trim().is_empty() || !INSURANCE_KINDS.contains(&p.kind.as_str()) {
            return Err(AppError::BadRequest("a policy needs a provider and a kind".into()));
        }
        handlers::iso_date("policy start", p.start_date.clone())?;
        handlers::iso_date("policy end", p.end_date.clone())?;
    }
    for e in plan.policy_ends.iter().filter(|e| e.selected) {
        handlers::iso_date("policy end", Some(e.end_date.clone()))?;
        // An end without its kept, same-pet replacement is simply not applied
        // (see `replacement_kept`); only one that WILL apply is checked here.
        if !replacement_kept(plan, e) {
            continue;
        }
        let now: Option<(String, String, Option<String>)> =
            sqlx::query_as("SELECT pet_id, status, end_date FROM insurance_policies WHERE id = ?")
                .bind(&e.policy_id)
                .fetch_optional(pool)
                .await?;
        let unchanged = now.as_ref().is_some_and(|(p, st, end)| {
            p == &e.pet_id && (st == "active" && end == &e.seen_end_date || st == "expired" && end.as_deref() == Some(e.end_date.as_str()))
        });
        if !unchanged {
            return Err(stale("a policy"));
        }
    }
    Ok(())
}

pub async fn commit(
    State(st): State<AppState>,
    Json(req): Json<ImportCommitRequest>,
) -> AppResult<(StatusCode, Json<ImportResult>)> {
    let pool = st.pool.clone();
    let bytes = handlers::decode_bounded(&req.data_base64, handlers::MAX_ATTACHMENT_BYTES)?;
    // Same type gate as analyze, BEFORE any write: a policy document store would
    // otherwise reject a bad type after records were already saved.
    if !ingestion::media_supported(&req.media_type) {
        return Err(AppError::BadRequest(format!("unsupported file type '{}'", req.media_type)));
    }
    // The fingerprint of the bytes actually sent, not the plan's `sha256`: a
    // client could pair a plan with a different file.
    let sha = handlers::sha256_hex(&bytes);
    let plan = req.plan;
    validate_commit(&pool, &plan, &sha).await?;
    let mut out = ImportResult::default();

    // 1. The clinic: an existing contact, or a new one.
    let contact_id = match plan.clinic.as_ref().filter(|c| c.selected) {
        Some(c) => match &c.contact_id {
            Some(cid) => Some(cid.clone()),
            // A contact of this exact name already exists (a retried commit
            // created it, or it was added since analysis): reuse it.
            None if existing_contact_named(&pool, &c.name).await?.is_some() => {
                existing_contact_named(&pool, &c.name).await?
            }
            None => {
                let (_, Json(created)) = handlers::create_contact(
                    State(pool.clone()),
                    Json(NewContact {
                        name: c.name.trim().to_string(),
                        role: c.role.clone(),
                        doctor: None,
                        phone: c.phone.clone(),
                        email: c.email.clone(),
                        address: c.address.clone(),
                        website: c.website.clone(),
                        notes: None,
                        requirements: None,
                        hours: None,
                        created_by: None,
                    }),
                )
                .await?;
                let id = created.id.clone();
                out.contact = Some(created);
                Some(id)
            }
        },
        None => None,
    };

    // 2. Records, each carrying the source document.
    for r in plan.records.iter().filter(|r| r.selected) {
        let pid = r.pet_id.clone().unwrap_or_default();
        // Commit isn't one transaction (it reuses each entity's handler, so the
        // validation stays in one place), so it is made IDEMPOTENT instead: a
        // record this same file already produced — same pet, date and title,
        // carrying these exact bytes — is skipped. Without this, a retry after a
        // mid-commit failure (a full disk on the third attachment) re-created
        // the first two records.
        if already_imported(&pool, &pid, r, &sha).await? {
            continue;
        }
        let (_, Json(rec)) = handlers::create_record(
            State(pool.clone()),
            Path(pid),
            Json(NewRecord {
                occurred_on: r.occurred_on.clone(),
                ends_on: r.ends_on.clone(),
                title: r.title.clone(),
                notes: r.notes.clone(),
                category: if CATEGORIES.contains(&r.category.as_str()) {
                    r.category.clone()
                } else {
                    default_category()
                },
                cost_cents: r.cost_cents,
                covered_cents: None,
                source: "import".into(),
                contact_id: contact_id.clone(),
                extra: Some(r.extra.clone()),
                line_items: Some(r.line_items.clone()),
                created_by: None,
            }),
        )
        .await?;
        attach_or_undo(&st, &pool, &rec, &req.filename, &req.media_type, &bytes).await?;
        out.records.push(rec);
    }

    // 3. Care team, and the clinic on duplicate visits that had none (only
    // where still empty, so a clinic someone set since is never replaced).
    if let Some(cid) = &contact_id {
        for l in plan.clinic_links.iter().filter(|l| l.selected) {
            let n = sqlx::query("UPDATE medical_records SET contact_id = ? WHERE id = ? AND pet_id = ? AND contact_id IS NULL")
                .bind(cid)
                .bind(l.record_id)
                .bind(&l.pet_id)
                .execute(&pool)
                .await?
                .rows_affected();
            out.clinic_links += n as usize;
        }
        for l in plan.care_team.iter().filter(|l| l.selected) {
            let _ = handlers::link_pet_contact(
                State(pool.clone()),
                Path(l.pet_id.clone()),
                Json(LinkContact { contact_id: cid.clone() }),
            )
            .await?;
            out.care_team_links += 1;
        }
    }

    // 4. Weights: add, or correct the existing point for that date.
    for w in plan.weights.iter().filter(|w| w.selected) {
        match w.existing_id {
            Some(wid) => {
                let _ = handlers::update_weight(
                    State(pool.clone()),
                    Path((w.pet_id.clone(), wid)),
                    Json(UpdateWeight {
                        recorded_on: None,
                        value: Some(w.value),
                        unit: Some(w.unit.clone()),
                    }),
                )
                .await?;
                out.weights_corrected += 1;
            }
            None if weight_exists(&pool, &w.pet_id, &w.recorded_on).await? => {
                out.skipped_existing += 1;
            }
            None => {
                let _ = handlers::create_weight(
                    State(pool.clone()),
                    Path(w.pet_id.clone()),
                    Json(NewWeight {
                        recorded_on: w.recorded_on.clone(),
                        value: w.value,
                        unit: w.unit.clone(),
                    }),
                )
                .await?;
                out.weights_added += 1;
            }
        }
    }

    // 5. Reminders (create is idempotent on pet + date + title).
    for r in plan.reminders.iter().filter(|r| r.selected) {
        let (status, _) = handlers::create_reminder(
            State(pool.clone()),
            Path(r.pet_id.clone()),
            Json(NewReminder {
                due_on: r.due_on.clone(),
                title: r.title.clone(),
                kind: None,
                source: "import".into(),
            }),
        )
        .await?;
        if status == StatusCode::CREATED {
            out.reminders_added += 1;
        }
    }
    for c in plan.completed_reminders.iter().filter(|c| c.selected) {
        let _ = handlers::update_reminder(
            State(pool.clone()),
            Path(c.reminder_id),
            Json(UpdateReminder { done: Some(true), due_on: None, title: None }),
        )
        .await?;
        out.reminders_completed += 1;
    }

    // 6. Profile updates, one PATCH per pet.
    let mut by_pet: BTreeMap<&str, UpdatePet> = BTreeMap::new();
    for u in plan.pet_updates.iter().filter(|u| u.selected) {
        let up = by_pet.entry(u.pet_id.as_str()).or_default();
        let v = Some(u.value.trim().to_string());
        match u.field.as_str() {
            "name" => up.name = v,
            "breed" => up.breed = Some(v),
            "sex" => up.sex = Some(v),
            "color" => up.color = Some(v),
            "microchip" => up.microchip = Some(v),
            _ => up.birth_date = Some(v),
        }
        out.pet_updates += 1;
    }
    for (pid, up) in by_pet {
        let _ = handlers::update_pet(State(pool.clone()), Path(pid.to_string()), Json(up)).await?;
    }

    // 7. Ongoing care. A dose change ends the old entry the day the new one
    // starts (today when the document doesn't say).
    for r in plan.regimens.iter().filter(|r| r.selected) {
        if regimen_exists(&pool, &r.pet_id, &r.product, r.amount.as_deref(), r.frequency.as_deref(), r.ended_on.as_deref()).await? {
            out.skipped_existing += 1;
            continue;
        }
        let _ = handlers::create_regimen(
            State(pool.clone()),
            Path(r.pet_id.clone()),
            Json(NewRegimen {
                kind: r.kind.clone(),
                product: r.product.clone(),
                amount: r.amount.clone(),
                frequency: r.frequency.clone(),
                protects: Some(r.protects.clone()),
                started_on: r.started_on.clone(),
                ended_on: r.ended_on.clone(),
                notes: r.notes.clone(),
                created_by: None,
            }),
        )
        .await?;
        out.regimens_added += 1;
        if let Some(old) = &r.replaces {
            let ended = r.started_on.clone().unwrap_or_else(today);
            let _ = handlers::update_regimen(
                State(pool.clone()),
                Path(old.clone()),
                Json(UpdateRegimen { ended_on: Some(Some(ended)), ..Default::default() }),
            )
            .await?;
            out.regimens_ended += 1;
        }
    }

    // 8. Insurance claims: the payout lands on the visit it reimbursed as its
    // covered amount (summed across claims), with the claim number recorded so
    // a re-import can't count it twice; no saved visit → the claim is the visit.
    for c in plan.claims.iter().filter(|c| c.selected) {
        let pid = c.pet_id.clone().unwrap_or_default();
        let key = claim_key(&c.claim_number);
        if claim_recorded(&pool, &pid, &key).await? {
            out.skipped_existing += 1;
            continue;
        }
        let value = claim_value(c);
        match c.record_id {
            Some(rid) => {
                // The document first: if it can't be stored, nothing about the
                // claim is recorded and a retry starts clean.
                handlers::store_attachment(&st, rid, &req.filename, &req.media_type, &bytes, "scan").await?;
                let cur = sqlx::query_as::<_, MedicalRecord>("SELECT * FROM medical_records WHERE id = ?")
                    .bind(rid)
                    .fetch_one(&pool)
                    .await?;
                let mut extra = cur.extra.map(|e| e.0).unwrap_or_default();
                extra.insert(key, value);
                let covered = cur.covered_cents.unwrap_or(0) + c.paid_cents;
                let _ = handlers::update_record(
                    State(pool.clone()),
                    Path((pid, rid)),
                    Json(UpdateRecord {
                        covered_cents: Some(Some(covered)),
                        extra: Some(Some(extra)),
                        ..Default::default()
                    }),
                )
                .await?;
            }
            None => {
                let (_, Json(rec)) = handlers::create_record(
                    State(pool.clone()),
                    Path(pid),
                    Json(NewRecord {
                        occurred_on: c.treatment_date.clone(),
                        ends_on: None,
                        title: claim_title(c),
                        notes: Some(claim_notes(c)),
                        category: default_category(),
                        cost_cents: c.items.iter().filter_map(|i| i.cost_cents).reduce(|a, b| a + b),
                        covered_cents: Some(c.paid_cents),
                        source: "import".into(),
                        contact_id: None,
                        extra: Some(BTreeMap::from([(key, value)])),
                        line_items: Some(
                            c.items
                                .iter()
                                .map(|i| LineItem { description: i.description.clone(), amount_cents: i.cost_cents })
                                .collect(),
                        ),
                        created_by: None,
                    }),
                )
                .await?;
                attach_or_undo(&st, &pool, &rec, &req.filename, &req.media_type, &bytes).await?;
                out.records.push(rec);
            }
        }
        out.claims_recorded += 1;
    }

    // 9. Insurance policies (with this document as the policy's document), then
    // end the ones they replace.
    for p in plan.policies.iter().filter(|p| p.selected) {
        let pid = p.pet_id.clone().unwrap_or_default();
        if policy_exists(&pool, &pid, &p.provider, &p.kind, p.policy_number.as_deref(), p.start_date.as_deref()).await? {
            out.skipped_existing += 1;
            continue;
        }
        let (_, Json(policy)) = handlers::create_insurance(
            State(pool.clone()),
            Path(pid),
            Json(NewInsurancePolicy {
                kind: p.kind.clone(),
                provider: p.provider.trim().to_string(),
                plan_name: p.plan_name.clone(),
                policy_number: p.policy_number.clone(),
                status: "active".into(),
                start_date: p.start_date.clone(),
                end_date: p.end_date.clone(),
                premium_cents: p.premium_cents,
                premium_interval: p.premium_interval.clone(),
                reimburse_percent: p.reimburse_percent,
                deductible_cents: p.deductible_cents,
                annual_limit_cents: p.annual_limit_cents,
                notes: p.notes.clone(),
                coverages: Some(p.coverages.clone()),
                exclusions: Some(p.exclusions.clone()),
                created_by: None,
            }),
        )
        .await?;
        if let Err(e) = handlers::store_insurance_document(&st, &policy, &req.filename, &req.media_type, &bytes).await {
            // No policy without its document: a retry would skip it as existing.
            sqlx::query("DELETE FROM insurance_policies WHERE id = ?")
                .bind(&policy.id)
                .execute(&pool)
                .await?;
            return Err(e);
        }
        out.policies_added += 1;
    }
    for e in plan.policy_ends.iter().filter(|e| e.selected && replacement_kept(&plan, e)) {
        let replacement = &plan.policies[e.replaced_by];
        let replacement_saved = replacement.existing_id.is_some()
            || policy_exists(
                &pool,
                replacement.pet_id.as_deref().unwrap_or_default(),
                &replacement.provider,
                &replacement.kind,
                replacement.policy_number.as_deref(),
                replacement.start_date.as_deref(),
            )
            .await?;
        if !replacement_saved {
            continue;
        }
        let _ = handlers::update_insurance(
            State(pool.clone()),
            Path(e.policy_id.clone()),
            Json(UpdateInsurancePolicy {
                status: Some("expired".into()),
                end_date: Some(Some(e.end_date.clone())),
                ..Default::default()
            }),
        )
        .await?;
        out.policies_ended += 1;
    }

    Ok((StatusCode::CREATED, Json(out)))
}
