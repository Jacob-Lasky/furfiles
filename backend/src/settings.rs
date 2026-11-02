//! Household settings backed by the `app_settings` key/value table: the LLM
//! ingestion config (#4) — the encrypted API key lives under `ingestion.api_key`
//! and is decrypted only at the moment of an extraction call — and display
//! preferences (#72).
//!
//! Setters take a connection rather than the pool so a multi-key update can run
//! in ONE transaction: a rejected or failed save must not leave half of it
//! written (a new model stored beside the old key).

use sqlx::{SqliteConnection, SqlitePool};

use crate::error::AppResult;

pub const DEFAULT_PROVIDER: &str = "anthropic";
pub const DEFAULT_MODEL: &str = "claude-sonnet-5";

const K_ENABLED: &str = "ingestion.enabled";
const K_PROVIDER: &str = "ingestion.provider";
const K_MODEL: &str = "ingestion.model";
const K_API_KEY: &str = "ingestion.api_key";
const K_BREEDER_TERMS: &str = "display.breeder_terms";

/// Resolved ingestion config. `api_key_enc` is the encrypted blob as stored, or
/// `None` when no key has been set.
#[derive(Debug, Clone)]
pub struct IngestionConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub api_key_enc: Option<String>,
}

async fn get_kv(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(v)
}

pub(crate) async fn set_kv(conn: &mut SqliteConnection, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?, ?, datetime('now')) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
    )
    .bind(key)
    .bind(value)
    .execute(conn)
    .await?;
    Ok(())
}

pub(crate) async fn del_kv(conn: &mut SqliteConnection, key: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM app_settings WHERE key = ?")
        .bind(key)
        .execute(conn)
        .await?;
    Ok(())
}

/// Load the ingestion config, applying defaults for any unset key.
pub async fn load(pool: &SqlitePool) -> AppResult<IngestionConfig> {
    Ok(IngestionConfig {
        enabled: get_kv(pool, K_ENABLED).await?.as_deref() == Some("true"),
        provider: get_kv(pool, K_PROVIDER)
            .await?
            .unwrap_or_else(|| DEFAULT_PROVIDER.to_string()),
        model: get_kv(pool, K_MODEL)
            .await?
            .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        api_key_enc: get_kv(pool, K_API_KEY).await?,
    })
}

pub async fn set_enabled(conn: &mut SqliteConnection, enabled: bool) -> AppResult<()> {
    set_kv(conn, K_ENABLED, if enabled { "true" } else { "false" }).await
}

pub async fn set_provider(conn: &mut SqliteConnection, provider: &str) -> AppResult<()> {
    set_kv(conn, K_PROVIDER, provider).await
}

pub async fn set_model(conn: &mut SqliteConnection, model: &str) -> AppResult<()> {
    set_kv(conn, K_MODEL, model).await
}

/// Store the already-encrypted API-key blob.
pub async fn set_api_key_enc(conn: &mut SqliteConnection, blob: &str) -> AppResult<()> {
    set_kv(conn, K_API_KEY, blob).await
}

/// Remove the stored key (used by DELETE, and when disabling).
pub async fn clear_api_key(conn: &mut SqliteConnection) -> AppResult<()> {
    del_kv(conn, K_API_KEY).await
}

/// Whether the household wants breeder terms (Dog / Bitch, Tom / Queen) for
/// intact dogs and cats (#72). Off unless set.
pub async fn breeder_terms(pool: &SqlitePool) -> AppResult<bool> {
    Ok(get_kv(pool, K_BREEDER_TERMS).await?.as_deref() == Some("true"))
}

pub async fn set_breeder_terms(conn: &mut SqliteConnection, on: bool) -> AppResult<()> {
    set_kv(conn, K_BREEDER_TERMS, if on { "true" } else { "false" }).await
}
