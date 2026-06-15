//! JWT issuance and verification (HS256, shared secret from config).

use chrono::Utc;
use jsonwebtoken::{
    DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::Error as JwtError,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ISSUER: &str = "nexium-auth";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
}

#[derive(Clone)]
pub struct JwtIssuer {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
    expiry_secs: i64,
}

impl JwtIssuer {
    pub fn new(secret: &str, expiry_secs: u64) -> Self {
        let mut validation = Validation::default();
        validation.set_issuer(&[ISSUER]);
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            validation,
            expiry_secs: expiry_secs as i64,
        }
    }

    /// Issue an access token for `user_id`. Returns `(token, expires_in_secs)`.
    pub fn issue(&self, user_id: Uuid) -> Result<(String, i64), JwtError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: user_id,
            iat: now,
            exp: now + self.expiry_secs,
            iss: ISSUER.to_string(),
        };
        let token = encode(&Header::default(), &claims, &self.encoding_key)?;
        Ok((token, self.expiry_secs))
    }

    /// Decode and validate a token (signature, expiry, issuer).
    #[allow(dead_code)] // used by /auth/me in the next task
    pub fn verify(&self, token: &str) -> Result<Claims, JwtError> {
        Ok(decode::<Claims>(token, &self.decoding_key, &self.validation)?.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_then_verify_roundtrip() {
        let issuer = JwtIssuer::new("test-secret-32-chars-long-enough", 3600);
        let uid = Uuid::new_v4();
        let (token, expires_in) = issuer.issue(uid).unwrap();
        assert_eq!(expires_in, 3600);
        let claims = issuer.verify(&token).unwrap();
        assert_eq!(claims.sub, uid);
        assert_eq!(claims.iss, ISSUER);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let a = JwtIssuer::new("secret-a", 60);
        let b = JwtIssuer::new("secret-b", 60);
        let (token, _) = a.issue(Uuid::new_v4()).unwrap();
        assert!(b.verify(&token).is_err());
    }

    #[test]
    fn issued_token_carries_configured_expiry() {
        let issuer = JwtIssuer::new("secret", 1800);
        let (token, expires_in) = issuer.issue(Uuid::new_v4()).unwrap();
        assert_eq!(expires_in, 1800);
        let claims = issuer.verify(&token).unwrap();
        assert_eq!(claims.exp - claims.iat, 1800);
    }
}
