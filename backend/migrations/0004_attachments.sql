-- Attachments are first-class (#4): a record can carry photos and documents
-- (vet invoices, discharge notes, vaccine certs). The bytes live on the /data
-- volume; this table holds the metadata. ON DELETE CASCADE drops the rows when a
-- record (or its pet) is deleted; the handler unlinks the files alongside.
CREATE TABLE attachments (
    id          TEXT PRIMARY KEY,                                   -- uuid v4
    record_id   INTEGER NOT NULL REFERENCES medical_records(id) ON DELETE CASCADE,
    filename    TEXT NOT NULL,                                      -- original name, for display + download
    media_type  TEXT NOT NULL,                                      -- MIME type
    size_bytes  INTEGER NOT NULL,
    stored_path TEXT NOT NULL,                                      -- path relative to the attachments base dir
    source      TEXT NOT NULL DEFAULT 'upload',                     -- 'upload' | 'scan' (the doc an LLM read)
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_attachments_record ON attachments(record_id);
