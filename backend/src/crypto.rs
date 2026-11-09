//! At-rest encryption for the single secret FurFiles stores: the user's LLM API
//! key (#4). XChaCha20-Poly1305 with a random 24-byte nonce per value; the blob
//! is `base64(nonce || ciphertext)`. The key is NEVER logged and the plaintext
//! never leaves `settings`/`handlers`.

use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use sha2::{Digest, Sha256};

const NONCE_LEN: usize = 24;

/// Resolve the 32-byte symmetric key used to encrypt secrets at rest.
///
/// Preference order:
/// 1. `FURFILES_SECRET` env var (hashed to 32 bytes) — the recommended path; the
///    key material never touches the data volume.
/// 2. A `furfiles-secret.key` file next to the database, so a key survives
///    restarts even when no env secret is set. Created with 0600 perms if absent.
///
/// Losing/rotating this key makes an already-stored API key undecryptable — the
/// user simply re-enters it. That is an acceptable failure for a single secret.
pub fn resolve_key(db_dir: &Path) -> [u8; 32] {
    if let Ok(secret) = std::env::var("FURFILES_SECRET") {
        if !secret.trim().is_empty() {
            let mut h = Sha256::new();
            h.update(secret.as_bytes());
            return h.finalize().into();
        }
    }

    let path = db_dir.join("furfiles-secret.key");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            return k;
        }
        tracing::warn!("secret key file {} is malformed; regenerating", path.display());
    }

    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    if let Err(e) = write_key_file(&path, &k) {
        tracing::warn!("could not persist secret key to {}: {e}", path.display());
    }
    tracing::warn!(
        "FURFILES_SECRET is not set; using a generated key at {}. Set FURFILES_SECRET for stronger secret management.",
        path.display()
    );
    k
}

#[cfg(unix)]
fn write_key_file(path: &Path, key: &[u8; 32]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(key)
}

#[cfg(not(unix))]
fn write_key_file(path: &Path, key: &[u8; 32]) -> std::io::Result<()> {
    std::fs::write(path, key)
}

/// Encrypt UTF-8 plaintext into a `base64(nonce || ciphertext)` blob.
pub fn encrypt(key: &[u8; 32], plaintext: &str) -> String {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plaintext.as_bytes())
        .expect("XChaCha20-Poly1305 encryption cannot fail for valid input");
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    B64.encode(out)
}

/// Decrypt a blob produced by [`encrypt`]. Errors if the blob is malformed or the
/// key/tag doesn't match (e.g. the secret changed).
pub fn decrypt(key: &[u8; 32], blob: &str) -> anyhow::Result<String> {
    let raw = B64.decode(blob.trim())?;
    if raw.len() <= NONCE_LEN {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce, ct) = raw.split_at(NONCE_LEN);
    let nonce = XNonce::try_from(nonce).map_err(|_| anyhow::anyhow!("malformed nonce"))?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let pt = cipher
        .decrypt(&nonce, ct)
        .map_err(|_| anyhow::anyhow!("decryption failed (wrong key or corrupt data)"))?;
    Ok(String::from_utf8(pt)?)
}
