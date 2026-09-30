use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::env;

const KNOWN_INSECURE_JWT_SECRETS: &[&str] = &[
    "your-super-secret-jwt-key-minimum-32-characters-long",
    "your-super-secret-jwt-key-change-in-production",
    "your-very-long-random-jwt-secret-min-32-chars",
];

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // email
    pub exp: usize,
    pub user_id: i32,
    /// Per-user token version; must match the user's current version in the DB
    /// for the token to be accepted. `serde(default)` keeps older tokens (issued
    /// before this field existed) decoding as version 0.
    #[serde(default)]
    pub token_version: i32,
}

/// bcrypt at `DEFAULT_COST` is a few hundred milliseconds of CPU by design. On a
/// Tokio worker that stalls every other request scheduled there, redirects
/// included, so a burst of sign-ins from many addresses could make the whole
/// service unresponsive. Both helpers run bcrypt on the blocking pool instead.
pub async fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    let password = password.to_owned();
    run_blocking(move || hash(password, DEFAULT_COST)).await
}

pub async fn verify_password(password: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
    let (password, hash) = (password.to_owned(), hash.to_owned());
    run_blocking(move || verify(password, &hash)).await
}

async fn run_blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(value) => value,
        // A panic inside bcrypt resurfaces here exactly as it would have inline.
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}

/// bcrypt hashes only the first 72 bytes of its input and ignores the rest,
/// so a longer password is silently weaker than the one that was typed: every
/// password sharing those 72 bytes unlocks it. New account and link passwords
/// are capped here. Login and link unlock still accept any length, so anything
/// set before the cap keeps working.
pub const MAX_PASSWORD_BYTES: usize = 72;

/// Error text for a password over [`MAX_PASSWORD_BYTES`]. Counted in bytes
/// because that is what bcrypt counts; accented letters and emoji take more
/// than one.
pub const PASSWORD_TOO_LONG: &str =
    "Password must be at most 72 bytes (72 plain characters; fewer with accents or emoji)";

pub fn password_exceeds_bcrypt_limit(password: &str) -> bool {
    password.len() > MAX_PASSWORD_BYTES
}

/// Read and validate the JWT signing secret from the environment.
///
/// Panics if `JWT_SECRET` is unset, empty, or shorter than 32 bytes. The server
/// validates this at startup (see [`validate_jwt_secret`]) so a misconfigured
/// deployment fails fast instead of silently signing tokens with a weak,
/// publicly-known key.
fn jwt_secret() -> String {
    let secret = env::var("JWT_SECRET").unwrap_or_default();
    if let Err(message) = validate_jwt_secret_value(&secret) {
        panic!("{message}");
    }
    secret
}

fn validate_jwt_secret_value(secret: &str) -> Result<(), String> {
    if secret.len() < 32 {
        return Err(format!(
            "JWT_SECRET must be set and at least 32 bytes long (got {} bytes). \
             Generate one with `openssl rand -base64 64`.",
            secret.len()
        ));
    }

    let candidate = secret.trim();
    if KNOWN_INSECURE_JWT_SECRETS
        .iter()
        .any(|placeholder| candidate.eq_ignore_ascii_case(placeholder))
    {
        return Err("JWT_SECRET is a publicly known placeholder. \
             Generate a unique secret with `openssl rand -base64 64`."
            .to_string());
    }

    Ok(())
}

/// Validate the JWT secret at startup so the process refuses to boot when it is
/// missing or too weak, rather than failing later on the first token operation.
pub fn validate_jwt_secret() {
    let _ = jwt_secret();
}

pub fn create_jwt(
    user_id: i32,
    email: &str,
    token_version: i32,
) -> Result<String, jsonwebtoken::errors::Error> {
    let secret = jwt_secret();

    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: email.to_owned(),
        exp: expiration as usize,
        user_id,
        token_version,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
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
    use crate::utils::test_env::set_env;

    // Single test (no parallel writes to the shared JWT_SECRET env var) covering
    // both the startup guard (B1) and a normal sign/verify round-trip.
    #[test]
    fn jwt_secret_enforced_and_roundtrips() {
        // A too-short / weak secret must be rejected rather than silently accepted.
        set_env("JWT_SECRET", "short");
        let weak = std::panic::catch_unwind(|| create_jwt(1, "a@b.c", 0));
        assert!(
            weak.is_err(),
            "create_jwt must panic on a <32 byte JWT_SECRET"
        );

        // Length alone is insufficient: sample values are public signing keys.
        for placeholder in KNOWN_INSECURE_JWT_SECRETS {
            assert!(
                validate_jwt_secret_value(placeholder).is_err(),
                "known placeholder must be rejected: {placeholder}"
            );
        }
        set_env(
            "JWT_SECRET",
            "your-super-secret-jwt-key-minimum-32-characters-long",
        );
        let placeholder = std::panic::catch_unwind(|| create_jwt(1, "a@b.c", 0));
        assert!(
            placeholder.is_err(),
            "create_jwt must panic on a known placeholder"
        );

        // A strong secret round-trips and preserves the claims.
        set_env("JWT_SECRET", "a-sufficiently-long-test-secret-0123456789");
        let token = create_jwt(42, "x@y.z", 0).expect("valid secret should sign");
        let claims = decode_jwt(&token).expect("token should decode");
        assert_eq!(claims.user_id, 42);
        assert_eq!(claims.sub, "x@y.z");

        assert!(
            decode_jwt(&format!("{token}x")).is_err(),
            "tampered token must not decode"
        );

        let expired = encode(
            &Header::default(),
            &Claims {
                sub: "x@y.z".into(),
                exp: (Utc::now() - Duration::hours(1)).timestamp() as usize,
                user_id: 1,
                token_version: 0,
            },
            &EncodingKey::from_secret(b"a-sufficiently-long-test-secret-0123456789"),
        )
        .unwrap();
        assert!(
            decode_jwt(&expired).is_err(),
            "expired token must not decode"
        );

        set_env("JWT_SECRET", "a-different-long-test-secret-0123456789ab");
        assert!(
            decode_jwt(&token).is_err(),
            "token signed with another secret must not decode"
        );
        set_env("JWT_SECRET", "a-sufficiently-long-test-secret-0123456789");
    }

    /// On a single-threaded runtime, awaiting bcrypt must let other tasks run:
    /// the hash happens on the blocking pool, not on the thread that serves
    /// requests. Run inline, neither spawned task would get a turn first.
    #[tokio::test(flavor = "current_thread")]
    async fn bcrypt_runs_off_the_async_worker() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let hashed_first = Arc::new(AtomicBool::new(false));
        let flag = hashed_first.clone();
        tokio::spawn(async move { flag.store(true, Ordering::SeqCst) });
        let hashed = hash_password("p@ss word").await.unwrap();
        assert!(
            hashed_first.load(Ordering::SeqCst),
            "hash_password ran bcrypt on the async worker"
        );

        let verified_first = Arc::new(AtomicBool::new(false));
        let flag = verified_first.clone();
        tokio::spawn(async move { flag.store(true, Ordering::SeqCst) });
        assert!(verify_password("p@ss word", &hashed).await.unwrap());
        assert!(
            verified_first.load(Ordering::SeqCst),
            "verify_password ran bcrypt on the async worker"
        );
    }

    #[tokio::test]
    async fn password_hash_roundtrips_and_rejects_mismatch() {
        let hashed = hash_password("p@ss word").await.unwrap();
        assert!(verify_password("p@ss word", &hashed).await.unwrap());
        assert!(!verify_password("p@ss Word", &hashed).await.unwrap());
    }
}
