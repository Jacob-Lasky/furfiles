//! Known-answer vectors for the at-rest encryption of the stored AI key (#23).
//!
//! The blob below was written by `crypto::encrypt` built against
//! chacha20poly1305 0.10, sha2 0.10, base64 0.22 and rand 0.8 (2026-11-09),
//! before those crates moved a major. A live install holds a blob exactly like
//! it in `app_settings`, so any dependency bump that cannot read it back would
//! silently lose every household's saved API key. DO NOT regenerate these
//! vectors with the current crates: that would make the test prove nothing.

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

#[test]
fn a_passphrase_derives_the_same_key_as_before() {
    // The only test in this binary that touches the environment.
    std::env::set_var("FURFILES_SECRET", "passphrase");
    let dir = std::env::temp_dir().join(format!("furfiles-crypto-compat-{}", std::process::id()));
    let key = crypto::resolve_key(&dir);
    std::env::remove_var("FURFILES_SECRET");
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    // sha256("passphrase"), as `sha256sum` prints it.
    assert_eq!(hex, "1e089e3c5323ad80a90767bdd5907297b4138163f027097fd3bdbeab528d2d68");
}
