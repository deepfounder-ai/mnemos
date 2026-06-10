//! Integration tests for the password hashing module.

use mnemos::auth::{hash_password, verify_password};

#[test]
fn hash_is_argon2id() {
    let h = hash_password("hunter2").expect("hash");
    // PHC strings start with $argon2id$ for the default Argon2 algorithm.
    assert!(h.starts_with("$argon2"), "expected argon2 PHC, got: {h}");
    assert!(h.contains("$argon2id$") || h.contains("$argon2i$"), "got: {h}");
}

#[test]
fn verify_roundtrip() {
    let h = hash_password("hunter2").expect("hash");
    assert!(verify_password("hunter2", &h).unwrap());
}

#[test]
fn verify_rejects_wrong_password() {
    let h = hash_password("hunter2").expect("hash");
    assert!(!verify_password("hunter3", &h).unwrap());
    assert!(!verify_password("", &h).unwrap());
    assert!(!verify_password("HUNTER2", &h).unwrap()); // case-sensitive
}

#[test]
fn two_hashes_of_same_password_differ() {
    let a = hash_password("same").unwrap();
    let b = hash_password("same").unwrap();
    assert_ne!(a, b, "salt must be randomised");
    // Both verify.
    assert!(verify_password("same", &a).unwrap());
    assert!(verify_password("same", &b).unwrap());
}

#[test]
fn empty_password_rejected() {
    let err = hash_password("").unwrap_err();
    assert!(matches!(err, mnemos::error::AppError::Validation(_)));
}

#[test]
fn unicode_passwords_supported() {
    let h = hash_password("пароль").expect("hash unicode");
    assert!(verify_password("пароль", &h).unwrap());
    assert!(!verify_password("парол", &h).unwrap());
}

#[test]
fn long_password_works() {
    let pw: String = "a".repeat(1024);
    let h = hash_password(&pw).expect("hash long");
    assert!(verify_password(&pw, &h).unwrap());
}
