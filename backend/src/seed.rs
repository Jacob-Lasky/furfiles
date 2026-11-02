use sqlx::SqlitePool;

/// The household owner's user id. There is no user-management API yet
/// (multi-user is issue #2), so this guarantees the Household panel is never
/// empty on a fresh DB. The id is internal and stable (existing installs
/// attribute their records to it); only the display name is configurable.
pub const OWNER_ID: &str = "jake";

/// The owner's avatar colour (the brand coral), on a new install and in the demo.
pub const OWNER_COLOR: &str = "#F2624C";

/// Display name for the owner on a fresh install, from `FURFILES_OWNER_NAME`.
/// Only used when the owner row is first created; an existing install keeps
/// whatever name it already has.
pub fn owner_name(val: Option<&str>) -> String {
    match val.map(str::trim) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => "Owner".to_string(),
    }
}

/// Whether `FURFILES_OWNER_NAME` chose a name. Same blank rule as
/// [`owner_name`]: whitespace-only is unset.
pub fn owner_name_configured(val: Option<&str>) -> bool {
    val.is_some_and(|v| !v.trim().is_empty())
}

/// Whether the fictional demo household should be seeded. Off by default so a
/// production DB is never repopulated with sample pets after real data lands.
/// Enable with `FURFILES_SEED_DEMO=1` (or true/yes) for a demo, dev or CI run.
pub fn seed_enabled(val: Option<&str>) -> bool {
    crate::truthy(val)
}

/// Ensure the single owner user exists, named from `FURFILES_OWNER_NAME`.
/// Runs on every boot; the OR IGNORE makes it a no-op once present.
pub async fn ensure_owner(pool: &SqlitePool) -> anyhow::Result<()> {
    let name = owner_name(std::env::var("FURFILES_OWNER_NAME").ok().as_deref());
    sqlx::query("INSERT OR IGNORE INTO users (id, name, color) VALUES (?,?,?)")
        .bind(OWNER_ID)
        .bind(name)
        .bind(OWNER_COLOR)
        .execute(pool)
        .await?;
    Ok(())
}

/// Seed a believable, entirely FICTIONAL demo household so a demo, dev or CI
/// instance shows the living-profile design with content: six pets (one
/// remembered), clinics with a care team, visits, vaccinations, medications,
/// allergies, ongoing care, an insurance policy, weights and reminders. Every
/// name, phone number (555-01xx), address and identifier here is made up.
///
/// Dates are RELATIVE to the day the demo is seeded (SQLite `date('now',
/// <offset>)`), so a freshly started demo always has something due soon and a
/// recent visit. They do not move afterwards: a demo left running for weeks
/// ages like real data. Reseed by starting on an empty data directory.
/// No-op if pets already exist. Gated by `seed_enabled` at the call site so it
/// never runs against real data.
pub async fn seed_if_empty(pool: &SqlitePool) -> anyhow::Result<()> {
    let named = owner_name_configured(std::env::var("FURFILES_OWNER_NAME").ok().as_deref());
    seed_demo(pool, named).await
}

/// [`seed_if_empty`] with the environment read already: `owner_named` is
/// whether FURFILES_OWNER_NAME chose the owner's name (then the demo keeps it).
pub async fn seed_demo(pool: &SqlitePool, owner_named: bool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pets")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }
    tracing::info!("seeding the fictional demo household");

    for (id, name, color) in [
        (OWNER_ID, "Alex", OWNER_COLOR),
        ("dana", "Dana", "#C88A67"),
        ("mara", "Mara", "#9B8CE0"),
    ] {
        sqlx::query("INSERT OR IGNORE INTO users (id, name, color) VALUES (?,?,?)")
            .bind(id)
            .bind(name)
            .bind(color)
            .execute(pool)
            .await?;
    }
    // `ensure_owner` normally created the owner first. The demo reads better
    // with a person's name, but a name chosen with FURFILES_OWNER_NAME (even
    // "Owner") or already on the row wins: only the unconfigured default is
    // replaced.
    if !owner_named {
        sqlx::query("UPDATE users SET name = 'Alex' WHERE id = ? AND name = ?")
            .bind(OWNER_ID)
            .bind(owner_name(None))
            .execute(pool)
            .await?;
    }

    // (id, name, species, breed, sex, birth, gotcha, weight, unit, chip, colour,
    //  allergies JSON, insurer, reimburse, gA, gB, by)
    let pets = [
        ("biscuit", "Biscuit", "Dog", "Golden Retriever", "male_neutered", "2022-03-14", Some("2022-05-20"), 32.4, "kg", "000000000000101", "Gold",
            Some(r#"[{"label":"Chicken","detail":"Itchy ears and paws"}]"#), Some("Acme Pet Insurance"), Some("90%"), "#F3B44C", "#E0762F", OWNER_ID),
        ("miso", "Miso", "Cat", "Domestic Shorthair", "female_spayed", "2019-05-02", Some("2019-08-10"), 4.6, "kg", "000000000000102", "Calico",
            Some(r#"[{"label":"Penicillin","detail":"Hives (2023)"}]"#), None, None, "#DE93AE", "#B25E85", "mara"),
        ("comet", "Comet", "Horse", "Quarter Horse", "male_neutered", "2014-04-20", Some("2018-09-01"), 472.0, "kg", "", "Bay",
            None, None, None, "#D28F5F", "#A85C39", "dana"),
        ("pip", "Pip", "Bird", "Cockatiel", "male", "2023-02-11", None, 92.0, "g", "", "Grey pied",
            None, None, None, "#A493E6", "#6E5CC8", "mara"),
        ("bubbles", "Bubbles", "Fish", "Betta", "male", "2025-05-01", None, 0.0, "", "", "Blue",
            None, None, None, "#57C4CC", "#2E8FA8", OWNER_ID),
    ];
    for (id, name, species, breed, sex, birth, gotcha, weight, unit, chip, colour, allergies, ins, reimb, ga, gb, by) in pets {
        let w: Option<f64> = if weight > 0.0 { Some(weight) } else { None };
        let u: Option<&str> = if unit.is_empty() { None } else { Some(unit) };
        let c: Option<&str> = if chip.is_empty() { None } else { Some(chip) };
        sqlx::query(
            "INSERT INTO pets (id,name,species,breed,sex,birth_date,gotcha_date,weight,weight_unit,microchip,color,allergies,insurer,insurer_reimburse,gradient_a,gradient_b,created_by,created_at,updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?, datetime('now'), datetime('now'))",
        )
        .bind(id).bind(name).bind(species).bind(breed).bind(sex).bind(birth).bind(gotcha)
        .bind(w).bind(u).bind(c).bind(colour).bind(allergies).bind(ins).bind(reimb).bind(ga).bind(gb).bind(by)
        .execute(pool).await?;
    }

    // memorial pet (archived as `passed`, #3)
    sqlx::query(
        "INSERT INTO pets (id,name,species,breed,sex,birth_date,gradient_a,gradient_b,created_by,archived_at,archive_kind,memorial_note,created_at,updated_at) \
         VALUES ('rex','Rex','Dog','German Shepherd','male_neutered','2009-06-01','#9AA0A6','#6B7076',?, date('now','-400 days'), 'passed', 'Fifteen very good years. Every record kept.', datetime('now'), datetime('now'))",
    )
    .bind(OWNER_ID)
    .execute(pool).await?;

    // Clinics and facilities (#20). (id, name, role, doctor, phone, email, address, website, hours JSON, requirements JSON)
    let contacts = [
        ("demo-cedar-creek", "Cedar Creek Veterinary Hospital", "vet", Some("Dr. Alma Reyes"), "555-0142", Some("frontdesk@cedarcreek.example"),
            Some("120 Maple Ave, Springfield"), Some("https://cedarcreek.example"),
            Some(r#"[{"label":"Open","detail":"Mon–Fri 7:30am–6pm"},{"label":"Saturday","detail":"8am–12pm"}]"#), None),
        ("demo-riverside", "Riverside Cat Clinic", "vet", Some("Dr. Theo Park"), "555-0178", None,
            Some("48 River Rd, Springfield"), None, None, None),
        ("demo-northside-er", "Northside Animal Emergency", "emergency", None, "555-0199", None,
            Some("9 Hospital Way, Springfield"), None, Some(r#"[{"label":"Open","detail":"24 hours, every day"}]"#), None),
        ("demo-willow-run", "Willow Run Equine", "vet", Some("Dr. Sam Okafor"), "555-0123", None,
            Some("Barn calls only"), None, None, None),
        ("demo-feather-scale", "Feather & Scale Exotics", "specialist", Some("Dr. Iris Novak"), "555-0165", None,
            Some("300 Elm St, Springfield"), None, None, None),
        ("demo-happy-tails", "Happy Tails Daycare & Boarding", "boarding", None, "555-0100", Some("hello@happytails.example"),
            Some("77 Orchard Ln, Springfield"), Some("https://happytails.example"),
            Some(r#"[{"label":"Drop-off","detail":"7–10am"},{"label":"Pick-up","detail":"by 6pm"}]"#),
            Some(r#"[{"label":"Rabies","detail":"Current certificate"},{"label":"DHPP","detail":"Current"},{"label":"Bordetella","detail":"Within the last 6 months"},{"label":"Flea and tick prevention","detail":"Required year-round"}]"#)),
    ];
    for (id, name, role, doctor, phone, email, address, website, hours, reqs) in contacts {
        sqlx::query(
            "INSERT INTO contacts (id,name,role,doctor,phone,email,address,website,hours,requirements,created_by) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(id).bind(name).bind(role).bind(doctor).bind(phone).bind(email).bind(address).bind(website).bind(hours).bind(reqs).bind(OWNER_ID)
        .execute(pool).await?;
    }
    for (pet, contact) in [
        ("biscuit", "demo-cedar-creek"), ("biscuit", "demo-northside-er"), ("biscuit", "demo-happy-tails"),
        ("miso", "demo-riverside"), ("miso", "demo-northside-er"),
        ("comet", "demo-willow-run"), ("pip", "demo-feather-scale"),
        ("rex", "demo-cedar-creek"),
    ] {
        sqlx::query("INSERT INTO pet_contacts (pet_id, contact_id) VALUES (?,?)")
            .bind(pet).bind(contact).execute(pool).await?;
    }

    // (pet, days ago, title, notes, category, cost_cents, covered_cents, clinic, extra JSON, line items JSON, by)
    let recs = [
        ("biscuit", "-24 days", "Dental cleaning + extraction", Some("Recovering well. Soft food for a week."), "Appointments", Some(42000), Some(37800), Some("demo-cedar-creek"),
            None, Some(r#"[{"description":"Dental cleaning under anaesthesia","amount_cents":28000},{"description":"Extraction, premolar","amount_cents":9500},{"description":"Pain relief, 5 days","amount_cents":4500}]"#), OWNER_ID),
        ("biscuit", "-130 days", "Annual wellness exam", Some("Bloodwork all normal. Weight on target."), "Appointments", Some(18500), Some(16600), Some("demo-cedar-creek"), None, None, "dana"),
        ("biscuit", "-130 days", "Vaccinations: Rabies (3-yr) + DHPP", Some("Next rabies in three years."), "Prescriptions", Some(6200), None, Some("demo-cedar-creek"),
            Some(r#"{"Vaccines":"Rabies 3-year, DHPP","Next due":"Rabies in 3 years, DHPP in 1 year"}"#), None, OWNER_ID),
        ("biscuit", "-60 days", "Boarding: long weekend", Some("Four nights. Packed his food and allergy notes."), "Boarding", Some(24000), None, Some("demo-happy-tails"), None, None, OWNER_ID),
        ("biscuit", "-210 days", "Ear infection", Some("Likely the chicken allergy again. Drops for 10 days."), "Appointments", Some(9800), Some(8800), Some("demo-cedar-creek"), None, None, OWNER_ID),
        ("miso", "-90 days", "Dental scaling", None, "Appointments", Some(26000), None, Some("demo-riverside"), None, None, "mara"),
        ("miso", "-300 days", "Wellness exam + FVRCP", Some("Weight stable, coat healthy."), "Appointments", Some(15000), None, Some("demo-riverside"),
            Some(r#"{"Vaccines":"FVRCP"}"#), None, OWNER_ID),
        ("miso", "-45 days", "Groom and nail trim", None, "Grooming", Some(4500), None, None, None, None, "mara"),
        ("comet", "-55 days", "Dental float", Some("Routine."), "Appointments", Some(18000), None, Some("demo-willow-run"), None, None, "dana"),
        ("comet", "-200 days", "Spring vaccines + Coggins", Some("Travel paperwork on file."), "Appointments", Some(21000), None, Some("demo-willow-run"), None, None, "dana"),
        ("pip", "-160 days", "Avian wellness + beak trim", None, "Appointments", Some(12000), None, Some("demo-feather-scale"), None, None, "mara"),
        ("bubbles", "-35 days", "Fin rot treated", Some("Aquarium salt and a water-quality fix."), "Prescriptions", Some(1800), None, None, None, None, OWNER_ID),
    ];
    for (pet, ago, title, notes, cat, cost, cov, clinic, extra, items, by) in recs {
        sqlx::query(
            "INSERT INTO medical_records (pet_id,occurred_on,title,notes,category,cost_cents,covered_cents,contact_id,extra,line_items,source,created_by,created_at) \
             VALUES (?, date('now', ?), ?,?,?,?,?,?,?,?, 'manual', ?, datetime('now'))",
        )
        .bind(pet).bind(ago).bind(title).bind(notes).bind(cat).bind(cost).bind(cov).bind(clinic).bind(extra).bind(items).bind(by)
        .execute(pool).await?;
    }

    // Ongoing care (#63). (id, pet, kind, product, amount, frequency, protects JSON, started days ago, notes)
    let regimens = [
        ("demo-reg-1", "biscuit", "food", "Salmon & rice adult kibble", Some("2 cups"), Some("2x/day"), None, "-400 days", Some("Chicken-free (allergy).")),
        ("demo-reg-2", "biscuit", "preventative", "Monthly chewable (flea, tick, heartworm)", Some("1 chew, 26–50 kg"), Some("monthly"), Some(r#"["flea","tick","heartworm"]"#), "-400 days", None),
        ("demo-reg-3", "biscuit", "supplement", "Fish oil", Some("1 pump"), Some("daily"), None, "-90 days", None),
        ("demo-reg-4", "miso", "medication", "Thyroid tablet", Some("2.5 mg"), Some("2x/day"), None, "-150 days", Some("Recheck bloods every 6 months.")),
        ("demo-reg-5", "miso", "preventative", "Topical (flea, worm)", Some("1 pipette"), Some("monthly"), Some(r#"["flea","worm"]"#), "-300 days", None),
        ("demo-reg-6", "comet", "food", "Timothy hay + senior feed", Some("2 flakes + 1 scoop"), Some("2x/day"), None, "-700 days", None),
    ];
    for (id, pet, kind, product, amount, freq, protects, started, notes) in regimens {
        sqlx::query(
            "INSERT INTO pet_regimens (id,pet_id,kind,product,amount,frequency,protects,started_on,notes,created_by) \
             VALUES (?,?,?,?,?,?,?, date('now', ?), ?, ?)",
        )
        .bind(id).bind(pet).bind(kind).bind(product).bind(amount).bind(freq).bind(protects).bind(started).bind(notes).bind(OWNER_ID)
        .execute(pool).await?;
    }

    // One insurance term (#8/#29, #74), started ~8 months ago.
    sqlx::query(
        "INSERT INTO insurance_policies (id,pet_id,kind,provider,plan_name,policy_number,status,start_date,end_date,premium_cents,premium_interval,reimburse_percent,deductible_cents,annual_limit_cents,created_by) \
         VALUES ('demo-policy-1','biscuit','insurance','Acme Pet Insurance','Accident & Illness','DEMO-0000-0001','active', date('now','-240 days'), date('now','+125 days'), 4500,'month',90,25000,1000000,?)",
    )
    .bind(OWNER_ID)
    .execute(pool).await?;

    let weights = [
        ("biscuit", [30.1, 30.8, 31.2, 31.0, 31.9, 32.4]),
        ("miso", [4.9, 4.8, 4.7, 4.7, 4.6, 4.6]),
        ("comet", [468.0, 470.0, 469.0, 471.0, 470.0, 472.0]),
    ];
    let months_ago = ["-5 months", "-4 months", "-3 months", "-2 months", "-1 months", "-0 months"];
    for (pet, vals) in weights {
        // Weight entries are kg or lb only (the API rejects anything else), so
        // Pip's 92 g lives on the profile and has no history.
        let unit = "kg";
        for (i, v) in vals.iter().enumerate() {
            sqlx::query("INSERT INTO weight_entries (pet_id,recorded_on,value,unit) VALUES (?, date('now', ?), ?, ?)")
                .bind(pet)
                .bind(months_ago[i])
                .bind(*v)
                .bind(unit)
                .execute(pool)
                .await?;
        }
    }

    // Upcoming reminders drive the "what's next" surface (#35). Relative dates:
    // a couple are near-term so the profile and Home banner always show real
    // "next up" content.
    let reminders = [
        ("biscuit", "+6 days", "Heartworm preventative", "med"),
        ("biscuit", "+235 days", "DHPP booster", "vaccine"),
        ("biscuit", "+965 days", "Rabies (3-yr) booster", "vaccine"),
        ("miso", "+12 days", "Thyroid bloodwork recheck", "exam"),
        ("miso", "+65 days", "FVRCP booster", "vaccine"),
        ("comet", "+20 days", "Fall dental float", "appointment"),
        ("pip", "+40 days", "Avian wellness check", "appointment"),
        ("bubbles", "+3 days", "Water change", "care"),
    ];
    for (pet, due, title, kind) in reminders {
        sqlx::query(
            "INSERT INTO reminders (pet_id,due_on,title,kind,source,created_at) \
             VALUES (?, date('now', ?), ?, ?, 'seed', datetime('now'))",
        )
        .bind(pet)
        .bind(due)
        .bind(title)
        .bind(kind)
        .execute(pool)
        .await?;
    }
    Ok(())
}
