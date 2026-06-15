use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // email
    pub exp: usize,
    pub user_id: i32,
}

pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    hash(password, DEFAULT_COST)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
    verify(password, hash)
}

/// Read and validate the JWT signing secret from the environment.
///
/// Panics if `JWT_SECRET` is unset, empty, or shorter than 32 bytes. The server
/// validates this at startup (see [`validate_jwt_secret`]) so a misconfigured
/// deployment fails fast instead of silently signing tokens with a weak,
/// publicly-known key.
fn jwt_secret() -> String {
    let secret = env::var("JWT_SECRET").unwrap_or_default();
    if secret.len() < 32 {
        panic!(
            "JWT_SECRET must be set and at least 32 bytes long (got {} bytes). \
             Generate one with `openssl rand -base64 64`.",
            secret.len()
        );
    }
    secret
}

/// Validate the JWT secret at startup so the process refuses to boot when it is
/// missing or too weak, rather than failing later on the first token operation.
pub fn validate_jwt_secret() {
    let _ = jwt_secret();
}

pub fn create_jwt(user_id: i32, email: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let secret = jwt_secret();

    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: email.to_owned(),
        exp: expiration as usize,
        user_id,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
}

pub fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let secret = jwt_secret();

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Single test (no parallel writes to the shared JWT_SECRET env var) covering
    // both the startup guard (B1) and a normal sign/verify round-trip.
    #[test]
    fn jwt_secret_enforced_and_roundtrips() {
        // A too-short / weak secret must be rejected rather than silently accepted.
        std::env::set_var("JWT_SECRET", "short");
        let weak = std::panic::catch_unwind(|| create_jwt(1, "a@b.c"));
        assert!(weak.is_err(), "create_jwt must panic on a <32 byte JWT_SECRET");

        // A strong secret round-trips and preserves the claims.
        std::env::set_var("JWT_SECRET", "a-sufficiently-long-test-secret-0123456789");
        let token = create_jwt(42, "x@y.z").expect("valid secret should sign");
        let claims = decode_jwt(&token).expect("token should decode");
        assert_eq!(claims.user_id, 42);
        assert_eq!(claims.sub, "x@y.z");
    }
}
