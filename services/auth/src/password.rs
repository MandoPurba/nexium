//! Argon2id password hashing using the PHC string format.
//!
//! The PHC string embeds the algorithm, params, salt, and digest, so it is
//! self-describing — verification needs only the stored hash plus the candidate
//! password. We use [`Argon2::default()`], which is Argon2id with the crate's
//! recommended params (currently m=19456 KiB, t=2, p=1).
use rand::rngs::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Argon2, password_hash::Error as Argon2Error};
use std::sync::OnceLock;

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
pub fn verify(password: &str, phc: &str) -> Result<bool, Argon2Error> {
    let parsed = PasswordHash::new(phc)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(Argon2Error::Password) => Ok(false),
        Err(e) => Err(e),
    }
}

/// A precomputed PHC hash used to keep login response time roughly constant
/// when the email isn't registered — prevents user enumeration via timing.
pub fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        hash("nexium-timing-defense-throwaway").expect("dummy hash generation must not fail")
    })
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

    #[test]
    fn dummy_hash_is_verifiable_phc() {
        let d = dummy_hash();
        assert!(d.starts_with("$argon2id$"));
        assert!(!verify("any-attacker-input", d).unwrap());
    }
}
