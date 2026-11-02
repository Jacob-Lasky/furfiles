-- Shared contacts (#20). A vet is not per-pet data: a household shares one vet
-- across most pets (the horse Koa has a separate equine vet), and re-typing the
-- clinic on every pet or every record drifts out of sync. So a contact is its own
-- entity, entered once and reused. Generic on purpose (a `role`) so groomer /
-- boarding / daycare (#42) reuse the same table rather than each getting its own.
CREATE TABLE contacts (
    id         TEXT PRIMARY KEY,                 -- uuid v4
    name       TEXT NOT NULL,                    -- clinic / facility name
    role       TEXT NOT NULL DEFAULT 'vet',      -- vet/emergency/specialist/groomer/daycare/boarding
    doctor     TEXT,                             -- specific doctor/contact person at the clinic
    phone      TEXT,
    email      TEXT,
    address    TEXT,
    website    TEXT,
    notes      TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Many-to-many: a pet has a primary vet AND possibly a specialist / emergency /
-- groomer, and one contact is shared by many pets. The join table is the honest
-- model (#20) — a single primary-vet column would fight the specialist case.
CREATE TABLE pet_contacts (
    pet_id     TEXT NOT NULL REFERENCES pets(id) ON DELETE CASCADE,
    contact_id TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (pet_id, contact_id)
);

-- A record can name the clinic it happened at, pointing at the shared contact
-- instead of free text. ON DELETE SET NULL: deleting a contact must not delete
-- history, it just unlinks it. Default NULL is required for an added FK column
-- when foreign_keys are on.
ALTER TABLE medical_records ADD COLUMN contact_id TEXT REFERENCES contacts(id) ON DELETE SET NULL;

CREATE INDEX idx_contacts_role ON contacts(role);
CREATE INDEX idx_pet_contacts_pet ON pet_contacts(pet_id);
CREATE INDEX idx_pet_contacts_contact ON pet_contacts(contact_id);
CREATE INDEX idx_records_contact ON medical_records(contact_id);
