//! Argon2id password hashing using the PHC string format.
//!
//! The PHC string embeds the algorithm, params, salt, and digest, so it is
//! self-describing — verification needs only the stored hash plus the candidate
//! password. We use [`Argon2::default()`], which is Argon2id with the crate's
//! recommended params (currently m=19456 KiB, t=2, p=1).

use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
};
use argon2::{Argon2, password_hash::Error as Argon2Error};

/// Hash `password` and return its PHC-formatted string.
pub fn hash(password: &str) -> Result<String, Argon2Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
}

/// Verify `password` against a PHC-formatted hash.
///
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, `Err` on malformed hash.
#[allow(dead_code)]
pub fn verify(password: &str, phc: &str) -> Result<bool, Argon2Error> {
    let parsed = PasswordHash::new(phc)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(Argon2Error::Password) => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let h = hash("strongpassword123").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify("strongpassword123", &h).unwrap());
    }

    #[test]
    fn verify_wrong_password_returns_false() {
        let h = hash("strongpassword123").unwrap();
        assert!(!verify("wrongpassword", &h).unwrap());
    }

    #[test]
    fn each_hash_has_unique_salt() {
        let a = hash("samepass").unwrap();
        let b = hash("samepass").unwrap();
        assert_ne!(a, b, "salt must differ across hashes");
    }
}
