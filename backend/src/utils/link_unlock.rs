use base64::Engine as _;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const LINK_UNLOCK_TTL_SECONDS: i64 = 120;
const UNLOCK_PURPOSE: &str = "link_password_unlock";

#[derive(Debug, Serialize, Deserialize)]
struct LinkUnlockClaims {
    sub: String,
    exp: usize,
    link_id: i32,
    code: String,
    password_fingerprint: String,
}

fn password_fingerprint(password_hash: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(password_hash.as_bytes()))
}

fn configured_secret() -> Option<String> {
    std::env::var("JWT_SECRET")
        .ok()
        .filter(|secret| secret.len() >= 32)
}

fn create_with_secret(
    secret: &str,
    link_id: i32,
    code: &str,
    password_hash: &str,
) -> Option<String> {
    let exp = Utc::now()
        .checked_add_signed(Duration::seconds(LINK_UNLOCK_TTL_SECONDS))?
        .timestamp() as usize;
    let claims = LinkUnlockClaims {
        sub: UNLOCK_PURPOSE.to_string(),
        exp,
        link_id,
        code: code.to_string(),
        password_fingerprint: password_fingerprint(password_hash),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .ok()
}

fn validate_with_secret(
    secret: &str,
    token: &str,
    link_id: i32,
    code: &str,
    password_hash: &str,
) -> bool {
    let validation = Validation::new(Algorithm::HS256);
    let Ok(data) = decode::<LinkUnlockClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ) else {
        return false;
    };
    let claims = data.claims;
    claims.sub == UNLOCK_PURPOSE
        && claims.link_id == link_id
        && claims.code == code
        && claims.password_fingerprint == password_fingerprint(password_hash)
}

/// Issue a short-lived, code-specific password unlock token.
///
/// The password-hash fingerprint revokes outstanding tokens immediately when
/// the link password changes, without storing server-side session state.
pub fn create_link_unlock_token(link_id: i32, code: &str, password_hash: &str) -> Option<String> {
    let secret = configured_secret()?;
    create_with_secret(&secret, link_id, code, password_hash)
}

/// Validate a password unlock token against the link's current security state.
pub fn validate_link_unlock_token(
    token: &str,
    link_id: i32,
    code: &str,
    password_hash: &str,
) -> bool {
    let Some(secret) = configured_secret() else {
        return false;
    };
    validate_with_secret(&secret, token, link_id, code, password_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "unit-test-unlock-secret-that-is-long-enough";

    #[test]
    fn token_is_link_and_password_version_specific() {
        let token = create_with_secret(SECRET, 7, "private-code", "hash-v1").unwrap();
        assert!(validate_with_secret(
            SECRET,
            &token,
            7,
            "private-code",
            "hash-v1"
        ));
        assert!(!validate_with_secret(
            SECRET,
            &token,
            8,
            "private-code",
            "hash-v1"
        ));
        assert!(!validate_with_secret(
            SECRET,
            &token,
            7,
            "other-code",
            "hash-v1"
        ));
        assert!(!validate_with_secret(
            SECRET,
            &token,
            7,
            "private-code",
            "hash-v2"
        ));
    }
}
