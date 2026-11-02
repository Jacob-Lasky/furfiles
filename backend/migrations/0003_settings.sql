-- Optional LLM document ingestion (#4).
-- Generic household settings live in a key/value store so future global settings
-- don't each need their own migration. The ingestion API key is stored here
-- ENCRYPTED at rest (XChaCha20-Poly1305) under key `ingestion.api_key` — it is
-- never persisted in plaintext, never logged, and never returned by the API
-- (the settings endpoint exposes only `has_key: bool`).
CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
