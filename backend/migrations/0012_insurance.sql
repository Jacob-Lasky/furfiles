-- Insurance & wellness plans (#8 layer 1 + #29's insurance slice). A pet's
-- insurance is not a single string: a real policy (Lemonade, Trupanion, a vet's
-- wellness membership) carries a provider, policy number, coverage %, deductible,
-- annual limit, premium, dates, and a "what's covered / not covered" breakdown —
-- far more than the pet's thin `insurer`/`insurer_reimburse` columns could hold.
-- So a policy is its own pet-level entity (a pet may hold more than one: a switch
-- of insurer, or insurance PLUS a clinic wellness plan). The policy also embeds
-- ONE primary document (the policy PDF), stored on the /data volume the same way
-- pet photos are (write-new-before-delete-old); the row keeps the metadata.
--
-- `kind` separates true insurance (reimbursement %, deductible, annual limit)
-- from a vet wellness / preventative-care membership (copay + monthly fee, no
-- reimbursement) so the UI labels each correctly. Money is INTEGER cents, matching
-- medical_records.cost_cents. `coverages`/`exclusions` reuse the {label, detail}
-- LabeledDetail shape (contacts' requirements/hours) as JSON TEXT, NULL when empty.
CREATE TABLE insurance_policies (
    id                  TEXT PRIMARY KEY,                    -- uuid v4
    pet_id              TEXT NOT NULL REFERENCES pets(id) ON DELETE CASCADE,
    kind                TEXT NOT NULL DEFAULT 'insurance',   -- insurance | wellness
    provider            TEXT NOT NULL,                       -- "Lemonade", clinic name
    plan_name           TEXT,                                -- "Pet Insurance + Preventative", "Puppy Gold"
    policy_number       TEXT,                                -- "DEMO-POL-0401"
    status              TEXT NOT NULL DEFAULT 'active',      -- active | expired | cancelled
    start_date          TEXT,                                -- ISO date
    end_date            TEXT,                                -- ISO date (renewal)
    premium_cents       INTEGER,                             -- premium amount, cents
    premium_interval    TEXT,                                -- 'year' | 'month' (how it's quoted)
    reimburse_percent   INTEGER,                             -- 90 (insurance co-insurance)
    deductible_cents    INTEGER,                             -- 50000 = $500
    annual_limit_cents  INTEGER,                             -- 5000000 = $50k; NULL = unlimited
    notes               TEXT,                                -- markdown
    coverages           TEXT,                                -- JSON [{label,detail}]; NULL when empty
    exclusions          TEXT,                                -- JSON [{label,detail}]; NULL when empty
    document_path       TEXT,                                -- stored policy file, relative to attachments dir
    document_name       TEXT,                                -- original filename, for display/download
    document_media_type TEXT,                                -- MIME of the stored document
    document_size_bytes INTEGER,
    created_by          TEXT REFERENCES users(id),
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_insurance_pet ON insurance_policies(pet_id);
