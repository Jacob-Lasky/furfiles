-- Cost breakdown (#4): an invoice often itemizes charges (visit fee, each
-- medication). `line_items` is a JSON array of {description, amount_cents} so the
-- breakdown is structured (and can be summed) instead of stuffed into the
-- free-form `extra` map as strings. NULL when there's no breakdown; `cost_cents`
-- remains the record total.
ALTER TABLE medical_records ADD COLUMN line_items TEXT;
