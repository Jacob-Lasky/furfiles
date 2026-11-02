-- Standing care facts: what a pet IS and what it's ON, as opposed to what
-- HAPPENED to it (medical_records). Driven by a typical vet intake form, whose
-- questions a household should never have to answer
-- from memory: color, known allergies, current diet, current medications, and
-- whether the pet is current on flea / tick / heartworm prevention (with brand).

-- Coat colour / markings ("Blue Merle", "Gold"). Identity, next to breed —
-- every intake form asks it and it is how a lost pet gets described.
ALTER TABLE pets ADD COLUMN color TEXT;

-- Known allergies as JSON [{label, detail}] — the LabeledDetail shape already
-- used for contacts' requirements/hours and a policy's coverages/exclusions.
-- label = the allergen ("Chicken"), detail = the reaction ("hives, ear infections").
-- NULL when none are recorded, which is also the form's "No" answer.
ALTER TABLE pets ADD COLUMN allergies TEXT;

-- Everything a pet is ONGOINGLY on: its food, its preventatives, its regular
-- medications, its supplements. One table, not four columns, because the intake
-- form asks for all of them in the SAME shape — "(diet name, amount, frequency)"
-- and "(drug name, dose, frequency)" — and because ONE product routinely covers
-- SEVERAL protections: Simparica Trio is flea AND tick AND heartworm in a single
-- monthly pill. Flat `flea_prevention` / `tick_prevention` / `heartworm_prevention`
-- columns would store that product name three times and could never grow to a
-- dewormer or a joint supplement. DO NOT collapse this back into pet columns.
CREATE TABLE pet_regimens (
    id         TEXT PRIMARY KEY,                     -- uuid v4
    pet_id     TEXT NOT NULL REFERENCES pets(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL DEFAULT 'medication',   -- food | preventative | medication | supplement
    product    TEXT NOT NULL,                        -- "Hill's Science Diet…", "Simparica Trio"
    amount     TEXT,                                 -- dose or serving: "1 cup", "1 pill", "45+ lbs"
    frequency  TEXT,                                 -- "2x/day", "monthly"
    -- What a preventative protects against, JSON ["flea","tick","heartworm"].
    -- NULL for food/medication/supplement. This is what answers the form's
    -- "Is your pet current on Heartworm Prevention? Brand: ____".
    protects   TEXT,
    started_on TEXT,                                 -- ISO date, optional
    ended_on   TEXT,                                 -- ISO date; NULL = still on it (the "current" test)
    notes      TEXT,                                 -- markdown, same renderer as records
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_regimens_pet ON pet_regimens(pet_id);
