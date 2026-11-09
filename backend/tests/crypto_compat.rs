//! Known-answer vectors for the at-rest encryption of the stored AI key (#23).
//!
//! The blob below was written by `crypto::encrypt` built against
//! chacha20poly1305 0.10, sha2 0.10, base64 0.22 and rand 0.8 (2026-11-09),
//! before those crates moved a major. A live install holds a blob exactly like
//! it in `app_settings`, so any dependency bump that cannot read it back would
//! silently lose every household's saved API key. DO NOT regenerate these
//! vectors with the current crates: that would make the test prove nothing.
//! The key-file tests below cover the generate/persist/reload path the rand
//! upgrade touched.

use furfiles::crypto;

const KEY: [u8; 32] = [7u8; 32];
const PLAINTEXT: &str = "sk-ant-known-answer-vector-0001";
const OLD_BLOB: &str =
    "K/P4RjnWgl8Szr6AWcavOEhKUMkmFkwaYAic8o+PSkejrit1Jji37pMS5w9d4qgBPSJwYVs9ue7mcUcBkiU9gRTinV7geXA=";

#[test]
fn decrypts_a_blob_written_by_the_previous_crates() {
    assert_eq!(crypto::decrypt(&KEY, OLD_BLOB).unwrap(), PLAINTEXT);
    // And the current crates still round-trip, with a fresh nonce each time.
    let a = crypto::encrypt(&KEY, PLAINTEXT);
    let b = crypto::encrypt(&KEY, PLAINTEXT);
    assert_ne!(a, b);
    assert_eq!(crypto::decrypt(&KEY, &a).unwrap(), PLAINTEXT);
    assert!(crypto::decrypt(&[8u8; 32], OLD_BLOB).is_err());
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("furfiles-crypto-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_passphrase_derives_the_same_key_as_before() {
    let dir = scratch_dir("pass");
    let key = crypto::resolve_key_from(Some("passphrase"), &dir);
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    // sha256("passphrase"), as `sha256sum` prints it.
    assert_eq!(hex, "1e089e3c5323ad80a90767bdd5907297b4138163f027097fd3bdbeab528d2d68");
    // A passphrase never writes a key file.
    assert!(!dir.join("furfiles-secret.key").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_passphrase_a_private_key_file_is_generated_once() {
    let dir = scratch_dir("file");
    // Blank counts as unset.
    let first = crypto::resolve_key_from(Some("   "), &dir);
    let path = dir.join("furfiles-secret.key");
    assert_eq!(std::fs::read(&path).unwrap(), first.to_vec());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_ne!(first, [0u8; 32], "random, not zeroed");
    // The same key comes back on every start.
    assert_eq!(crypto::resolve_key_from(None, &dir), first);
    // A stored value decrypts after a restart.
    let blob = crypto::encrypt(&first, "sk-ant-restart");
    assert_eq!(crypto::decrypt(&crypto::resolve_key_from(None, &dir), &blob).unwrap(), "sk-ant-restart");

    // A malformed file is replaced with a fresh 32-byte key.
    std::fs::write(&path, b"short").unwrap();
    let regenerated = crypto::resolve_key_from(None, &dir);
    assert_ne!(regenerated, first);
    assert_eq!(std::fs::read(&path).unwrap(), regenerated.to_vec());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_blobs_are_errors_not_panics() {
    assert!(crypto::decrypt(&KEY, "not base64 !!").is_err());
    assert!(crypto::decrypt(&KEY, "AAAA").is_err(), "shorter than a nonce");
}
