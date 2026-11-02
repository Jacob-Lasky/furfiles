-- Generalized capture (#4): a document (a receipt, an invoice) carries more than
-- the fixed record fields. `extra` is a JSON object of label -> value pairs so
-- nothing found on a document is dropped, and so a user can add their own custom
-- fields per record by hand. NULL means "no extra details". The reusable,
-- household-level custom-field system is the fuller follow-up (#24).
ALTER TABLE medical_records ADD COLUMN extra TEXT;
