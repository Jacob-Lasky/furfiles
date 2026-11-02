-- How a pet left the active household (#3). Archiving used to mean only one
-- thing — the pet passed — so every archived pet was shown under a memorial.
-- But a horse gets sold, a foster gets adopted, a dog goes to live with family:
-- the pet is ALIVE, and a memorial for it would be wrong and hurtful. So the
-- archive records which it was:
--   'passed'  — died; shown under "Remembering" with memorial wording.
--   'rehomed' — sold / rehomed / moved on, alive; shown under "Past companions",
--               no memorial wording.
-- NULL whenever archived_at is NULL (an active pet). Every pet archived before
-- this column existed was a memorial, so backfill those to 'passed'.
--
-- The old endpoint stored whatever date string it was given, so a blank one
-- could land as archived_at = '' — "archived" to SQL (IS NOT NULL) but "active"
-- to the UI (falsy). Normalise those to genuinely active first, so no row is
-- both at once.
ALTER TABLE pets ADD COLUMN archive_kind TEXT;
UPDATE pets SET archived_at = NULL WHERE trim(archived_at) = '';
UPDATE pets SET archive_kind = 'passed' WHERE archived_at IS NOT NULL;
