//! Argon2id password hashing with the OWASP-recommended default parameters.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

use crate::error::{AppError, Result};

/// Hash a password. Returns the PHC string (algorithm + params + salt + hash).
pub fn hash_password(password: &str) -> Result<String> {
    if password.is_empty() {
        return Err(AppError::Validation("password must not be empty".into()));
    }
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    let hash = argon
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("argon2 hash failed: {e}")))?
        .to_string();
    Ok(hash)
}

/// Verify a plaintext password against a stored PHC string.
pub fn verify_password(password: &str, phc: &str) -> Result<bool> {
    let parsed = PasswordHash::new(phc)
        .map_err(|e| AppError::Internal(format!("invalid stored hash: {e}")))?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(AppError::Internal(format!("argon2 verify failed: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ok() {
        let h = hash_password("hunter2").expect("hash");
        assert!(h.starts_with("$argon2"));
        assert!(verify_password("hunter2", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }

    #[test]
    fn empty_password_rejected() {
        assert!(hash_password("").is_err());
    }
}
