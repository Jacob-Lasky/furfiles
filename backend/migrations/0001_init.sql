-- FurFiles initial schema.
-- Multi-user is anticipated from day one (users + created_by attribution) so that
-- issue #2 is a feature, not a migration. Species is a first-class column (#1).

CREATE TABLE users (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL DEFAULT '#F2624C',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE pets (
    id                TEXT PRIMARY KEY,          -- url slug, e.g. "biscuit" (deep-linkable, #11)
    name              TEXT NOT NULL,
    species           TEXT NOT NULL,             -- any species: dog/cat/horse/bird/fish/...
    breed             TEXT,
    sex               TEXT,
    birth_date        TEXT,                      -- ISO date
    weight            REAL,
    weight_unit       TEXT,
    microchip         TEXT,
    insurer           TEXT,
    insurer_reimburse TEXT,
    gradient_a        TEXT,                      -- portrait gradient (design.md), also drives identity color
    gradient_b        TEXT,
    photo_path        TEXT,                      -- real photos slot in here later
    archived_at       TEXT,                      -- memorialized when a pet passes (#3); NULL = active
    memorial_note     TEXT,
    created_by        TEXT REFERENCES users(id),
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE medical_records (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    pet_id       TEXT NOT NULL REFERENCES pets(id) ON DELETE CASCADE,
    occurred_on  TEXT NOT NULL,                  -- ISO date
    title        TEXT NOT NULL,
    notes        TEXT,
    category     TEXT NOT NULL DEFAULT 'Appointments',
    cost_cents   INTEGER,                        -- money in cents, never float
    covered_cents INTEGER,                       -- insurance-covered portion (#8)
    source       TEXT NOT NULL DEFAULT 'manual', -- 'manual' | 'scan' (LLM ingestion #4)
    created_by   TEXT REFERENCES users(id),
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE weight_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pet_id      TEXT NOT NULL REFERENCES pets(id) ON DELETE CASCADE,
    recorded_on TEXT NOT NULL,
    value       REAL NOT NULL,
    unit        TEXT NOT NULL DEFAULT 'kg'
);

CREATE INDEX idx_records_pet ON medical_records(pet_id);
CREATE INDEX idx_weights_pet ON weight_entries(pet_id);
