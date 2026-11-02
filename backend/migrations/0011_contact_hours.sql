-- Contact hours (#42 follow-up). Vet open-hours, daycare drop-off/pick-up windows,
-- and boarding pick-up deadlines were being dumped into free-text `notes`. This
-- formalizes them as a structured JSON array of {label, detail} (e.g.
-- {"label":"Open","detail":"Mon–Fri 7am–7pm"}, {"label":"Drop-off","detail":"6–10am"}),
-- NULL when unknown. A labeled list, not a rigid weekly grid, because the real
-- world mixes open-hours with drop-off/pick-up windows. Applies to any contact.
ALTER TABLE contacts ADD COLUMN hours TEXT;
