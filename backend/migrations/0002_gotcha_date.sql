-- "Gotcha day" — the day a pet joined the family — is first-class pet-record
-- data every owner remembers, distinct from birth_date. Nullable: not always
-- known (e.g. a horse acquired second-hand before papers surfaced).
ALTER TABLE pets ADD COLUMN gotcha_date TEXT;  -- ISO date, NULL = unknown
