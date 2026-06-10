//! Integration tests for the API key module.

use std::collections::HashSet;

use mnemos::auth::api_key::{generate, hash_api_key, looks_like_api_key};

#[test]
fn format_starts_with_mnemo() {
    let k = generate("id-1", "laptop");
    assert!(k.plaintext.starts_with("mnemo_"));
    assert!(looks_like_api_key(&k.plaintext));
}

#[test]
fn body_length_is_32() {
    let k = generate("id", "n");
    let body_len = k.plaintext.len() - "mnemo_".len();
    assert_eq!(body_len, 32);
}

#[test]
fn hash_is_sha256_hex() {
    let k = generate("id", "n");
    let expected = hash_api_key(&k.plaintext);
    assert_eq!(k.hash, expected);
    // 32 bytes = 64 hex chars.
    assert_eq!(expected.len(), 64);
    assert!(expected.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn hash_is_deterministic() {
    let plaintext = "mnemo_abc123";
    let h1 = hash_api_key(plaintext);
    let h2 = hash_api_key(plaintext);
    assert_eq!(h1, h2);
}

#[test]
fn unique_per_invocation() {
    let n = 200;
    let mut seen = HashSet::new();
    for i in 0..n {
        let k = generate(format!("id-{i}"), "n");
        assert!(seen.insert(k.plaintext.clone()), "duplicate plaintext in run {i}");
    }
    assert_eq!(seen.len(), n);
}

#[test]
fn rejection_of_malformed() {
    assert!(!looks_like_api_key(""));
    assert!(!looks_like_api_key("mnemo_"));
    assert!(!looks_like_api_key("mnemo_short"));
    assert!(!looks_like_api_key("prefix_only"));
    assert!(!looks_like_api_key("MNEMO_UPPER"));
    assert!(!looks_like_api_key("mnemo_<not-base62>"));
}

#[test]
fn id_and_name_round_trip() {
    let k = generate("the-id", "my-laptop");
    assert_eq!(k.id, "the-id");
    assert_eq!(k.name, "my-laptop");
}

#[test]
fn base62_charset() {
    // body chars should all be base62 [0-9A-Za-z].
    for _ in 0..20 {
        let k = generate("id", "n");
        let body = &k.plaintext["mnemo_".len()..];
        for c in body.chars() {
            assert!(c.is_ascii_alphanumeric(), "non-alphanumeric char in body: {c:?}");
        }
    }
}
