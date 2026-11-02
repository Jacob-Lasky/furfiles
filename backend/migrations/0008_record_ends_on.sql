-- Records can span time (#15): a treatment that runs several days, a boarding
-- stay, a course of medication. `ends_on` is the optional end of that range;
-- `occurred_on` remains the start / event date. NULL means a single-day event
-- (the common case). A real date (not a free-text "duration") so a calendar can
-- draw the span later.
ALTER TABLE medical_records ADD COLUMN ends_on TEXT;  -- ISO date, NULL = single day
