//! API key generation + SHA-256 hash storage.
//!
//! API key format: `mnemo_<32 base62 chars>`. The plaintext is shown exactly
//! once at creation and never persisted. The DB stores only the hex-encoded
//! SHA-256 hash.

use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// A freshly-generated API key, including the plaintext.
#[derive(Debug, Clone)]
pub struct ApiKey {
    /// Opaque key id (UUID) — used for revocation, never sent to clients
    /// after the initial response.
    pub id: String,
    /// User-friendly label, e.g. "claude-dev-laptop".
    pub name: String,
    /// Plaintext key, e.g. `mnemo_a1b2c3…`. Show this to the user once.
    pub plaintext: String,
    /// SHA-256 hash of the plaintext, hex-encoded. Persist this.
    pub hash: String,
}

const PREFIX: &str = "mnemo_";
const BODY_LEN: usize = 32; // 32 base62 chars ≈ 190 bits of entropy

const BASE62_ALPHABET: &[u8; 62] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Generate a new API key with a fresh random body.
pub fn generate(id: impl Into<String>, name: impl Into<String>) -> ApiKey {
    let mut body = String::with_capacity(BODY_LEN);
    let mut buf = [0u8; BODY_LEN];
    rand::thread_rng().fill_bytes(&mut buf);
    for b in buf {
        body.push(BASE62_ALPHABET[(b as usize) % BASE62_ALPHABET.len()] as char);
    }
    let plaintext = format!("{PREFIX}{body}");
    let hash = hash_api_key(&plaintext);
    ApiKey {
        id: id.into(),
        name: name.into(),
        plaintext,
        hash,
    }
}

/// Compute the canonical SHA-256 hex digest of an API key. Constant-time
/// comparison is not necessary at this layer (the hex digest is the
/// deterministic lookup key, not the secret).
pub fn hash_api_key(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// Validate the format of an API key. Returns `true` if the prefix and body
/// length look correct. The caller is still responsible for the DB lookup.
pub fn looks_like_api_key(plaintext: &str) -> bool {
    plaintext.starts_with(PREFIX) && plaintext.len() == PREFIX.len() + BODY_LEN
}

/// Encode arbitrary bytes as base62. Used in tests.
#[allow(dead_code)]
pub(crate) fn base62_encode(bytes: &[u8]) -> String {
    // Pad input length so we always produce at least 1 character.
    let mut out = String::with_capacity(bytes.len());
    for b in bytes {
        out.push(BASE62_ALPHABET[(*b as usize) % BASE62_ALPHABET.len()] as char);
    }
    out
}

/// Build the SHA-256 hash as raw bytes (used in some tests).
#[allow(dead_code)]
pub(crate) fn hash_bytes(b: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

/// Base64 helper used by some downstream consumers; re-exported for tests.
pub fn base64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_is_stable() {
        let k = generate("id-1", "test");
        assert!(k.plaintext.starts_with(PREFIX));
        assert_eq!(k.plaintext.len(), PREFIX.len() + BODY_LEN);
        assert_eq!(k.hash, hash_api_key(&k.plaintext));
        assert!(looks_like_api_key(&k.plaintext));
    }

    #[test]
    fn uniqueness_smoke() {
        // Two consecutive keys must differ.
        let a = generate("id-a", "a");
        let b = generate("id-b", "b");
        assert_ne!(a.plaintext, b.plaintext);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn rejects_garbage() {
        assert!(!looks_like_api_key("not-a-key"));
        assert!(!looks_like_api_key("mnemo_short"));
        assert!(!looks_like_api_key("MNEMO_CAPS"));
    }

    #[test]
    fn base64_helper() {
        assert_eq!(base64_encode(b"hi"), "aGk=");
    }
}
