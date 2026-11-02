-- Facility requirements (#42). A daycare/boarding facility imposes requirements
-- on the pet (up-to-date vaccinations, a negative fecal test, pre-portioned food,
-- no toys). Those are structured, per-facility data — a checklist that surfaces
-- when booking or packing — not free text buried in notes. Stored as a JSON array
-- of {label, detail} on the shared contact, NULL when there are none. The LLM web
-- parser (#42) populates this from the facility's website; it's also hand-editable.
ALTER TABLE contacts ADD COLUMN requirements TEXT;
