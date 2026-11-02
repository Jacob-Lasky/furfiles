-- Reminders / calendar events (#26). A vet document carries future due-dates
-- (next vaccine, fecal exam, follow-up) that don't belong on a past-dated record.
-- This is the "Reminder" data-model entity: a dated to-do per pet. The Calendar
-- view merges these alongside records; extraction proposes them for confirmation.
CREATE TABLE reminders (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    pet_id     TEXT NOT NULL REFERENCES pets(id) ON DELETE CASCADE,
    due_on     TEXT NOT NULL,                     -- ISO date
    title      TEXT NOT NULL,
    kind       TEXT,                              -- optional: vaccine / exam / med refill …
    done       INTEGER NOT NULL DEFAULT 0,        -- 0/1
    source     TEXT NOT NULL DEFAULT 'manual',    -- 'manual' | 'scan'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_reminders_pet ON reminders(pet_id);
CREATE INDEX idx_reminders_due ON reminders(due_on);
