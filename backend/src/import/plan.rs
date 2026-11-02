//! Model output → a plan the server vouches for. Pure (no I/O), so every rule
//! here is unit-testable without a model.

use super::*;

/// How far past a document's latest service date a reminder may fall due and
/// still be completed by it: a booster or test given a little EARLY satisfies
/// the reminder (Lulu's 4Dx ran Sep 4 against a Nov 5 reminder). Anything later
/// is the clinic's next due date.
pub(crate) const COMPLETION_WINDOW_DAYS: i64 = 90;

/// `iso` + `days`, as an ISO date. None for an unparseable date.
pub(crate) fn add_days(iso: &str, days: i64) -> Option<String> {
    chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .ok()
        .map(|d| (d + chrono::Duration::days(days)).format("%Y-%m-%d").to_string())
}

// ---------- normalisation: model output → a plan the server vouches for ----------

pub(crate) fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub(crate) fn valid_date(v: Option<String>) -> Option<String> {
    handlers::iso_date("date", v).ok().flatten()
}

pub(crate) fn dollars_to_cents(d: f64) -> Option<i64> {
    d.is_finite().then(|| (d * 100.0).round() as i64)
}

/// Digits only, last 10 — so "555-010-0142" and "+1 (555) 010 0142" compare equal.
pub(crate) fn phone_key(p: &str) -> Option<String> {
    let d: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
    (d.len() >= 10).then(|| d[d.len() - 10..].to_string())
}

pub(crate) fn name_key(n: &str) -> String {
    n.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Why a sex update is proposed from the document's own procedure record.
fn procedure_reason(pet: &Pet, value: &str) -> String {
    let what = if value == "female_spayed" { "spay" } else { "neuter" };
    format!("This document shows {}'s {what}.", pet.name)
}

/// Turn the model's extraction into a plan: drop ids that don't exist, convert
/// money to cents, and apply the deterministic duplicate checks. Pure, so every
/// rule here is unit-testable without a model.
pub fn build_plan(
    x: ImportExtraction,
    h: &Household,
    sha256: String,
    file_duplicates: Vec<FileDuplicate>,
) -> ImportPlan {
    let pet_by_id: HashMap<&str, &Pet> = h.pets.iter().map(|p| (p.id.as_str(), p)).collect();
    let pet_by_name: HashMap<String, &str> = h
        .pets
        .iter()
        .map(|p| (name_key(&p.name), p.id.as_str()))
        .collect();
    // Pets that already have a record carrying this exact file. Per PET, not per
    // file: an invoice for Lulu and Pedro previously attached only to Lulu's
    // visit must still offer Pedro's.
    // ...and per DATE: a multi-visit file partly imported before must still offer
    // that pet's other visits.
    // ...and per VISIT (pet + date + title): a multi-visit file partly imported
    // before must still offer that pet's other visits, same-day ones included.
    let file_dup_visits: HashSet<(&str, &str, String)> = file_duplicates
        .iter()
        .map(|d| (d.pet_id.as_str(), d.occurred_on.as_str(), name_key(&d.title)))
        .collect();

    // Clinic: trust the model's id only if it exists; else match phone or name.
    let clinic = x.clinic.and_then(|c| {
        let by_id = c
            .existing_contact_id
            .as_deref()
            .and_then(|id| h.contacts.iter().find(|k| k.id == id));
        let by_phone = c.phone.as_deref().and_then(phone_key).and_then(|pk| {
            h.contacts
                .iter()
                .find(|k| k.phone.as_deref().and_then(phone_key).as_deref() == Some(pk.as_str()))
        });
        let by_name = c.name.as_deref().map(name_key).and_then(|nk| {
            h.contacts.iter().find(|k| !nk.is_empty() && name_key(&k.name) == nk)
        });
        let matched = by_id.or(by_phone).or(by_name);
        let name = clean(c.name.clone()).or_else(|| matched.map(|m| m.name.clone()))?;
        let role = c
            .role
            .filter(|r| handlers::CONTACT_ROLES.contains(&r.as_str()))
            .unwrap_or_else(default_role);
        Some(PlannedClinic {
            selected: true,
            contact_id: matched.map(|m| m.id.clone()),
            matched_name: matched.map(|m| m.name.clone()),
            name,
            role,
            phone: clean(c.phone),
            email: clean(c.email),
            address: clean(c.address),
            website: clean(c.website),
        })
    });

    let mut records = Vec::new();
    for r in x.records {
        let pet_id = r
            .pet_id
            .filter(|id| pet_by_id.contains_key(id.as_str()))
            .or_else(|| {
                r.pet_name_on_document
                    .as_deref()
                    .and_then(|n| pet_by_name.get(&name_key(n)).map(|s| s.to_string()))
            });
        let occurred_on = valid_date(r.occurred_on).unwrap_or_default();
        let title = clean(r.title).unwrap_or_default();
        let cost_cents = r.cost.and_then(dollars_to_cents);
        let category = r
            .category
            .filter(|c| CATEGORIES.contains(&c.as_str()))
            .unwrap_or_else(default_category);
        let line_items: Vec<LineItem> = r
            .line_items
            .into_iter()
            .filter(|li| !li.description.trim().is_empty())
            .map(|li| LineItem {
                description: li.description.trim().to_string(),
                amount_cents: li.amount.and_then(dollars_to_cents),
            })
            .collect();

        // $0 means "no price shown", not "free": live, a clinic history with no
        // prices came back as cost 0 on every visit, and on another run as $0
        // line items, both rendering "$0.00" everywhere. A truly free visit loses
        // its "$0", which is the cheaper mistake.
        let cost_cents = cost_cents.filter(|c| *c != 0);
        let line_items: Vec<LineItem> = line_items
            .into_iter()
            .map(|l| LineItem { amount_cents: l.amount_cents.filter(|a| *a != 0), ..l })
            .collect();

        // Duplicate: the model's pick if it is a real record of the same pet ON
        // THE SAME DATE (a different date is a different visit), else the same
        // pet and date with the same total or the same title.
        let same_pet = |b: &&RecordBrief| Some(b.pet_id.as_str()) == pet_id.as_deref();
        let by_model = r.duplicate_of_record_id.and_then(|rid| {
            h.records.iter().filter(same_pet).find(|b| b.id == rid && b.occurred_on == occurred_on).map(|b| RecordRef {
                record_id: b.id,
                title: b.title.clone(),
                occurred_on: b.occurred_on.clone(),
                reason: "Looks like the same visit as a record you already have".into(),
            })
        });
        let by_total = || {
            h.records
                .iter()
                .filter(same_pet)
                .find(|b| {
                    b.occurred_on == occurred_on && cost_cents.is_some() && b.cost_cents == cost_cents
                })
                .map(|b| RecordRef {
                    record_id: b.id,
                    title: b.title.clone(),
                    occurred_on: b.occurred_on.clone(),
                    reason: "Same pet, date and total as a record you already have".into(),
                })
        };
        // A rescanned certificate has different bytes and often no cost.
        let by_title = || {
            let tk = name_key(&title);
            h.records
                .iter()
                .filter(same_pet)
                // Two real same-day "Office visit"s with different totals are two
                // visits; only a matching title with no conflicting total is a rescan.
                .find(|b| {
                    b.occurred_on == occurred_on
                        && !tk.is_empty()
                        && name_key(&b.title) == tk
                        && (cost_cents.is_none() || b.cost_cents.is_none() || b.cost_cents == cost_cents)
                })
                .map(|b| RecordRef {
                    record_id: b.id,
                    title: b.title.clone(),
                    occurred_on: b.occurred_on.clone(),
                    reason: "Same pet, date and title as a record you already have".into(),
                })
        };
        let duplicate_of = by_model.or_else(by_total).or_else(by_title);
        let file_dup = pet_id
            .as_deref()
            .is_some_and(|p| file_dup_visits.contains(&(p, occurred_on.as_str(), name_key(&title))));

        // Selected even with no pet: the screen then BLOCKS saving until one is
        // picked, instead of an unassigned visit sitting unticked and getting
        // silently left behind.
        let selected = !occurred_on.is_empty()
            && !title.is_empty()
            && duplicate_of.is_none()
            && !file_dup;
        records.push(PlannedRecord {
            selected,
            pet_id,
            pet_name_on_document: clean(r.pet_name_on_document),
            occurred_on,
            ends_on: valid_date(r.ends_on),
            title,
            category,
            cost_cents,
            line_items,
            notes: clean(r.notes),
            extra: r
                .extra_fields
                .into_iter()
                .filter(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
                .collect(),
            duplicate_of,
        });
    }

    // Weights: one per pet+date; an identical stored value is dropped, a
    // different one becomes a correction that's off by default (it overwrites).
    let mut weights: Vec<PlannedWeight> = Vec::new();
    for w in x.weights {
        let (Some(pid), Some(date), Some(value)) = (
            w.pet_id.filter(|id| pet_by_id.contains_key(id.as_str())),
            valid_date(w.recorded_on),
            w.value.filter(|v| v.is_finite() && *v > 0.0),
        ) else {
            continue;
        };
        if weights.iter().any(|p| p.pet_id == pid && p.recorded_on == date) {
            continue;
        }
        let unit = match w.unit.as_deref() {
            Some("kg") => "kg",
            _ => "lb",
        }
        .to_string();
        let existing = h
            .weights
            .iter()
            .find(|e| e.pet_id == pid && e.recorded_on == date);
        match existing {
            Some(e) if (e.value - value).abs() < 0.05 && e.unit == unit => continue,
            Some(e) => weights.push(PlannedWeight {
                selected: false,
                pet_id: pid,
                recorded_on: date,
                value,
                unit,
                existing_id: Some(e.id),
                existing_value: Some(e.value),
                existing_unit: Some(e.unit.clone()),
                date_is_inferred: w.date_is_inferred,
            }),
            None => weights.push(PlannedWeight {
                selected: !w.date_is_inferred,
                pet_id: pid,
                recorded_on: date,
                value,
                unit,
                existing_id: None,
                existing_value: None,
                existing_unit: None,
                date_is_inferred: w.date_is_inferred,
            }),
        }
    }

    let mut reminders: Vec<PlannedReminder> = Vec::new();
    for r in x.reminders {
        let (Some(pid), Some(due), Some(title)) = (
            r.pet_id.filter(|id| pet_by_id.contains_key(id.as_str())),
            valid_date(r.due_on),
            clean(r.title),
        ) else {
            continue;
        };
        let lt = title.to_lowercase();
        if reminders
            .iter()
            .any(|p| p.pet_id == pid && p.due_on == due && p.title.to_lowercase() == lt)
        {
            continue;
        }
        let duplicate_of = h
            .open_reminders
            .iter()
            .find(|e| e.pet_id == pid && e.due_on == due && e.title.to_lowercase() == lt)
            .map(|e| e.id);
        let same_day = if duplicate_of.is_some() {
            None
        } else {
            h.open_reminders
                .iter()
                .find(|e| e.pet_id == pid && e.due_on == due)
                .map(|e| e.title.clone())
        };
        reminders.push(PlannedReminder {
            // A different-titled reminder the same day is probably the same thing
            // worded differently; the person ticks it if not.
            selected: duplicate_of.is_none() && same_day.is_none(),
            pet_id: pid,
            due_on: due,
            title,
            duplicate_of,
            same_day,
        });
    }

    // A document can only complete care it shows being done, so a reminder due
    // long after the latest service date in it is the clinic's NEXT due date, not
    // a completed one. Measured live: claude-sonnet-5 read "Lepto due 03-05-27,
    // last done 03-05-26" as "the 2027 reminder is done", which would have hidden
    // a year of upcoming care. The prompt says so too; this makes it hold.
    // Per PET: a document only shows care done for the pets it has visits for.
    // A Pedro invoice can't complete Lulu's reminder (nor can an injected one).
    // Saved visits this exact file is attached to count as service dates too:
    // for a re-uploaded invoice the model may report no visits at all (seen
    // live), yet the fingerprint proves the visit happened.
    let latest_service = |pet: &str| {
        records
            .iter()
            .filter(|r| r.pet_id.as_deref() == Some(pet) && !r.occurred_on.is_empty())
            .map(|r| r.occurred_on.as_str())
            .chain(file_duplicates.iter().filter(|d| d.pet_id == pet).map(|d| d.occurred_on.as_str()))
            .max()
            .and_then(|d| add_days(d, COMPLETION_WINDOW_DAYS))
    };
    let mut completed_reminders: Vec<PlannedCompletion> = Vec::new();
    for c in x.completed_reminders {
        let Some(rem) = c
            .reminder_id
            .and_then(|id| h.open_reminders.iter().find(|r| r.id == id))
        else {
            continue;
        };
        if latest_service(&rem.pet_id).as_deref().is_none_or(|cut| rem.due_on.as_str() > cut) {
            continue;
        }
        if completed_reminders.iter().any(|p| p.reminder_id == rem.id) {
            continue;
        }
        completed_reminders.push(PlannedCompletion {
            selected: true,
            reminder_id: rem.id,
            pet_id: rem.pet_id.clone(),
            title: rem.title.clone(),
            due_on: rem.due_on.clone(),
            reason: clean(c.reason),
        });
    }

    // Profile updates: filling a blank is on by default; overwriting a value the
    // owner already saved is a suggestion they have to tick — EXCEPT a spay or
    // neuter the document shows being performed (#72). That document is the
    // evidence the stored "female" is out of date, so the update is ticked.
    //
    // A spay/neuter is "shown" when the model says so (`sterilisation_performed`)
    // OR one of this document's records for that pet is the procedure by its
    // title or billed lines — the deterministic check, so a model that forgets
    // the flag can't leave Lulu "Female" after importing her spay invoice.
    let sterilised_by_doc = |pet: &Pet| -> Option<&'static str> {
        records
            .iter()
            .filter(|r| r.pet_id.as_deref() == Some(pet.id.as_str()))
            .find_map(|r| {
                let items = r.line_items.iter().map(|i| i.description.as_str());
                sex::sterilised_form(pet.sex.as_deref(), sex::record_sterilisation(&r.title, items)?)
            })
    };
    let mut pet_updates: Vec<PlannedPetUpdate> = Vec::new();
    for u in x.pet_updates {
        let (Some(pet), Some(field), Some(value)) = (
            u.pet_id.as_deref().and_then(|id| pet_by_id.get(id)),
            u.field.filter(|f| PET_UPDATE_FIELDS.contains(&f.as_str())),
            clean(u.value),
        ) else {
            continue;
        };
        let mut reason = clean(u.reason);
        let value = match field.as_str() {
            "birth_date" => match valid_date(Some(value)) {
                Some(d) => d,
                None => continue,
            },
            // A procedure this document's records show beats whatever sex the
            // model proposed: a castration invoice makes the pet male_neutered
            // even if the model misread the header as "female_spayed".
            "sex" => match (sterilised_by_doc(pet), sex::sex_from_text(&value)) {
                (Some(v), from_model) => {
                    if from_model != Some(v) {
                        reason = Some(procedure_reason(pet, v));
                    }
                    v.to_string()
                }
                // Read the document's wording ("Neutered Male", "MN", "FS")
                // onto the closed set. `unknown` adds nothing a profile can use.
                (None, Some(v)) if v != "unknown" => v.to_string(),
                _ => continue,
            },
            _ => value,
        };
        let current = clean(match field.as_str() {
            "name" => Some(pet.name.clone()),
            "breed" => pet.breed.clone(),
            "sex" => pet.sex.clone(),
            "color" => pet.color.clone(),
            "microchip" => pet.microchip.clone(),
            _ => pet.birth_date.clone(),
        });
        if current.as_deref().map(str::to_lowercase) == Some(value.to_lowercase())
            || pet_updates.iter().any(|p| p.pet_id == pet.id && p.field == field)
        {
            continue;
        }
        // An `unknown` sex is as good as blank: filling it in is not an overwrite.
        let fills_blank = current.is_none() || (field == "sex" && current.as_deref() == Some("unknown"));
        // The performed procedure must point the same way as the update: a
        // "spayed" proposal for a recorded male is a misread, not evidence.
        let evidence = field == "sex"
            && (sterilised_by_doc(pet) == Some(value.as_str())
                || (u.sterilisation_performed && sex::sterilises(pet.sex.as_deref(), &value)));
        pet_updates.push(PlannedPetUpdate {
            selected: fills_blank || evidence,
            pet_id: pet.id.clone(),
            field,
            current,
            value,
            reason,
        });
    }
    // The model read the procedure but proposed no sex change: propose it
    // anyway, ticked, from the record that shows it.
    for pet in &h.pets {
        if pet_updates.iter().any(|p| p.pet_id == pet.id && p.field == "sex") {
            continue;
        }
        let Some(value) = sterilised_by_doc(pet) else {
            continue;
        };
        pet_updates.push(PlannedPetUpdate {
            selected: true,
            pet_id: pet.id.clone(),
            field: "sex".into(),
            current: clean(pet.sex.clone()),
            value: value.into(),
            reason: Some(procedure_reason(pet, value)),
        });
    }

    let mut regimens: Vec<PlannedRegimen> = Vec::new();
    for r in x.regimens {
        let (Some(pid), Some(kind), Some(product)) = (
            r.pet_id.filter(|id| pet_by_id.contains_key(id.as_str())),
            r.kind.filter(|k| REGIMEN_KINDS.contains(&k.as_str())),
            clean(r.product),
        ) else {
            continue;
        };
        let pk = name_key(&product);
        let amount = clean(r.amount);
        let frequency = clean(r.frequency);
        if regimens.iter().any(|g| g.pet_id == pid && name_key(&g.product) == pk) {
            continue; // the same product twice in one document
        }
        // The current entry for this product, if any (the model's id first).
        let current = r
            .duplicate_of_regimen_id
            .and_then(|id| {
                // The model's id only counts if it IS this product: pointing a
                // "Cytopoint" at the saved "Apoquel" must not hide or end Apoquel.
                h.current_regimens
                    .iter()
                    .find(|e| e.id == id && e.pet_id == pid && name_key(&e.product) == pk)
            })
            .or_else(|| {
                h.current_regimens
                    .iter()
                    .find(|e| e.pet_id == pid && name_key(&e.product) == pk)
            });
        // Same product: a duplicate unless the document states a DIFFERENT dose
        // or frequency, which is a change — saving it ends the old entry.
        let differs = |new: &Option<String>, old: &Option<String>| {
            new.as_deref().is_some_and(|n| old.as_deref().map(name_key) != Some(name_key(n)))
        };
        let (duplicate_of, replaces) = match current {
            Some(e) if differs(&amount, &e.amount) || differs(&frequency, &e.frequency) => {
                (None, Some(e.id.clone()))
            }
            Some(e) => (Some(e.id.clone()), None),
            None => (None, None),
        };
        let protects = if kind == "preventative" {
            r.protects
                .into_iter()
                .map(|p| p.trim().to_lowercase())
                .filter(|p| PROTECTIONS.contains(&p.as_str()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        regimens.push(PlannedRegimen {
            selected: duplicate_of.is_none(),
            pet_id: pid,
            kind,
            product,
            amount,
            frequency,
            protects,
            started_on: valid_date(r.started_on),
            ended_on: valid_date(r.ended_on),
            notes: clean(r.notes),
            duplicate_of,
            replaces,
        });
    }

    // Claims: onto the visit they reimbursed (same pet, same treatment date),
    // or a visit of their own when none is saved. Already recorded → off.
    let mut claims: Vec<PlannedClaim> = Vec::new();
    for c in x.claims {
        let (Some(number), Some(date)) = (clean(c.claim_number), valid_date(c.treatment_date)) else {
            continue;
        };
        let Some(paid_cents) = c.total_paid.and_then(dollars_to_cents) else { continue };
        let pet_id = c.pet_id.filter(|id| pet_by_id.contains_key(id.as_str()));
        let key = claim_key(&number);
        // One claim per pet. Two UNASSIGNED claims with one number are kept
        // apart (a two-pet EOB the model couldn't split): the person assigns each.
        if pet_id.is_some() && claims.iter().any(|k| k.pet_id == pet_id && claim_key(&k.claim_number) == key) {
            continue;
        }
        let mine = |b: &&RecordBrief| Some(b.pet_id.as_str()) == pet_id.as_deref();
        let already_recorded = h
            .records
            .iter()
            .filter(mine)
            .any(|b| b.extra.as_ref().is_some_and(|e| e.0.keys().any(|k| k.eq_ignore_ascii_case(&key))));
        let items: Vec<ClaimItem> = c
            .items
            .into_iter()
            .filter(|i| !i.description.trim().is_empty())
            .map(|i| ClaimItem {
                description: i.description.trim().to_string(),
                cost_cents: i.cost.and_then(dollars_to_cents),
                covered_cents: i.covered.and_then(dollars_to_cents),
                note: clean(i.note),
            })
            .collect();
        let billed: Option<i64> = items.iter().filter_map(|i| i.cost_cents).reduce(|a, b| a + b);
        // The visit it reimbursed: that pet's visit on the treatment date; with
        // several, the one whose total is closest to what was billed.
        let same_day: Vec<&RecordBrief> = h.records.iter().filter(mine).filter(|b| b.occurred_on == date).collect();
        let visit = same_day.iter().copied().min_by_key(|b| match (b.cost_cents, billed) {
            (Some(c), Some(bl)) => (c - bl).abs(),
            _ => i64::MAX,
        });
        // A best guess only: with several visits that day the screen offers them all.
        let visit_options: Vec<VisitOption> = if same_day.len() > 1 {
            same_day
                .iter()
                .map(|b| VisitOption { record_id: b.id, title: b.title.clone(), cost_cents: b.cost_cents })
                .collect()
        } else {
            Vec::new()
        };
        claims.push(PlannedClaim {
            selected: !already_recorded, // no pet → the screen makes you pick one
            pet_id,
            claim_number: number,
            policy_number: clean(c.policy_number),
            insurer: clean(c.insurer),
            treatment_date: date,
            items,
            paid_cents,
            record_id: visit.map(|b| b.id),
            record_title: visit.map(|b| b.title.clone()),
            visit_options,
            already_recorded,
        });
    }
    // A model that ALSO wrote a record for a claimed visit would double it. Only
    // a record that IS the claim goes: same pet and day, and either the claim's
    // billed or paid total, or a title that says it's the claim. A distinct visit
    // that day stays, priced or not (an unpriced lab visit is not a claim).
    records.retain(|r| {
        let looks_like_claim = {
            let t = r.title.to_lowercase();
            t.contains("claim") || t.contains("insurance") || t.contains("reimburs") || t.contains("explanation of benefits")
        };
        !claims.iter().any(|c| {
            let billed = c.items.iter().filter_map(|i| i.cost_cents).reduce(|a, b| a + b);
            c.pet_id == r.pet_id
                && c.treatment_date == r.occurred_on
                && (looks_like_claim
                    || (r.cost_cents.is_some() && (r.cost_cents == billed || r.cost_cents == Some(c.paid_cents))))
        })
    });

    // Policies: new unless this pet already has one with that number. A new
    // policy from the same provider that starts inside an active policy's term
    // REPLACES it (Lemonade re-issues on a move), so that one is ended.
    let mut policies: Vec<PlannedPolicy> = Vec::new();
    let mut policy_ends: Vec<PlannedPolicyEnd> = Vec::new();
    for p in x.policies {
        let f = p.fields;
        let Some(provider) = clean(f.provider) else { continue };
        let pet_id = p.pet_id.filter(|id| pet_by_id.contains_key(id.as_str()));
        let number = clean(f.policy_number);
        let kind = f
            .kind
            .filter(|k| INSURANCE_KINDS.contains(&k.as_str()))
            .unwrap_or_else(|| "insurance".into());
        let start_date = valid_date(f.start_date);
        let existing_id = h
            .policies
            .iter()
            .find(|e| {
                Some(e.pet_id.as_str()) == pet_id.as_deref()
                    && same_policy(e, &provider, &kind, number.as_deref(), start_date.as_deref())
            })
            .map(|e| e.id.clone());
        if existing_id.is_none() {
            // With no pet yet, offer the end for every pet's matching policy,
            // unticked: the screen ticks the one matching the pet you pick.
            if let Some(start) = start_date.as_deref() {
                for e in h.policies.iter().filter(|e| {
                    pet_id.as_deref().is_none_or(|pid| e.pet_id == pid)
                        && e.status == "active"
                        && e.kind == kind
                        && name_key(&e.provider) == name_key(&provider)
                        && e.start_date.as_deref().is_some_and(|s| s < start)
                        && e.end_date.as_deref().is_none_or(|end| end > start)
                }) {
                    policy_ends.push(PlannedPolicyEnd {
                        selected: pet_id.is_some(),
                        policy_id: e.id.clone(),
                        pet_id: e.pet_id.clone(),
                        provider: e.provider.clone(),
                        policy_number: e.policy_number.clone(),
                        end_date: start.to_string(),
                        replaced_by: policies.len(),
                        seen_end_date: e.end_date.clone(),
                    });
                }
            }
        }
        policies.push(PlannedPolicy {
            selected: existing_id.is_none(), // no pet → the screen makes you pick one
            pet_id,
            kind,
            provider,
            plan_name: clean(f.plan_name),
            policy_number: number,
            start_date,
            end_date: valid_date(f.end_date),
            premium_cents: f.premium.and_then(dollars_to_cents),
            premium_interval: f.premium_interval.filter(|i| i == "year" || i == "month"),
            reimburse_percent: f.reimburse_percent.filter(|r| (0..=100).contains(r)),
            deductible_cents: f.deductible.and_then(dollars_to_cents),
            annual_limit_cents: f.annual_limit.and_then(dollars_to_cents),
            notes: clean(f.notes),
            coverages: f.coverages,
            exclusions: f.exclusions,
            existing_id,
        });
    }

    // A clinic only means something for a VET document: one with visits, or one
    // the server already knows by fingerprint is attached to saved visits (a
    // re-uploaded invoice the model rightly reported no NEW visits for). An
    // insurance document's "clinic" is the insurer, which must never become a
    // vet contact.
    let vet_document = !records.is_empty()
        || (!file_duplicates.is_empty() && claims.is_empty() && policies.is_empty());
    let clinic = if vet_document { clinic } else { None };

    // Care team: every pet this document shows at the clinic — including a pet
    // whose visit is a duplicate or already carries this file, since that still
    // proves who its vet is (the "I changed vets; does FurFiles know?" case) —
    // unless already on the team.
    let mut care_team = Vec::new();
    if let Some(c) = &clinic {
        let mut seen = HashSet::new();
        let pets_seen = records
            .iter()
            .filter_map(|r| r.pet_id.as_deref())
            .chain(file_duplicates.iter().map(|d| d.pet_id.as_str()));
        for pid in pets_seen {
            let on_team = c
                .contact_id
                .as_deref()
                .is_some_and(|cid| h.care_team.iter().any(|(p, k)| p == pid && k == cid));
            if !on_team && seen.insert(pid.to_string()) {
                care_team.push(PlannedLink { selected: true, pet_id: pid.to_string() });
            }
        }
    }
    let mut clinic_links: Vec<PlannedClinicLink> = Vec::new();
    if clinic.is_some() {
        let dup_ids = records
            .iter()
            .filter_map(|r| r.duplicate_of.as_ref().map(|d| d.record_id))
            .chain(file_duplicates.iter().map(|d| d.record_id));
        for rid in dup_ids {
            let Some(b) = h.records.iter().find(|b| b.id == rid && b.clinic.is_none()) else { continue };
            if clinic_links.iter().any(|l| l.record_id == rid) {
                continue;
            }
            clinic_links.push(PlannedClinicLink {
                selected: true,
                record_id: b.id,
                pet_id: b.pet_id.clone(),
                title: b.title.clone(),
                occurred_on: b.occurred_on.clone(),
            });
        }
    }

    ImportPlan {
        sha256,
        file_duplicates,
        summary: clean(x.summary),
        clinic,
        records,
        care_team,
        clinic_links,
        weights,
        reminders,
        completed_reminders,
        pet_updates,
        regimens,
        claims,
        policies,
        policy_ends,
        low_confidence_fields: x.low_confidence_fields,
        provider: String::new(),
        model: String::new(),
    }
}

/// One policy identity rule for analyze AND commit: same provider and kind, and
/// the same number when there is one, else the same start date (missing on both
/// counts as the same). A number alone isn't an identity (two insurers can
/// reuse a format), and an unnumbered plan still needs one or retries duplicate.
/// A numbered policy with a DIFFERENT recorded start is a renewal under the
/// same number (many insurers keep it across terms): a new term, its own row,
/// because insurance value (#74) prices each row as one term.
pub(crate) fn same_policy(
    e: &InsurancePolicy,
    provider: &str,
    kind: &str,
    number: Option<&str>,
    start: Option<&str>,
) -> bool {
    if name_key(&e.provider) != name_key(provider) || e.kind != kind {
        return false;
    }
    match number.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            e.policy_number.as_deref().map(name_key) == Some(name_key(n))
                && (start.is_none() || e.start_date.is_none() || e.start_date.as_deref() == start)
        }
        None => e.policy_number.is_none() && e.start_date.as_deref() == start,
    }
}
