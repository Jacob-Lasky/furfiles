-- Content fingerprint for every stored document (#23 bulk import). Bulk import
-- has to answer "is this file already in FurFiles?" for documents that come back
-- under a different name: a re-download ("AVIEE50DDBC (1).PDF" vs
-- "AVIEE50DDBC_1.PDF"), a renamed copy, the same invoice from email and from the
-- clinic portal. The filename can't answer that; the bytes can.
-- Existing rows are filled in at startup by hashing the files already on the
-- volume (`backfill_attachment_hashes`), since SQLite can't read them itself.
ALTER TABLE attachments ADD COLUMN sha256 TEXT;
CREATE INDEX idx_attachments_sha256 ON attachments(sha256);
