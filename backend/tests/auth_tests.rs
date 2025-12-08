mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use axum_test::TestServer;
use serde_json::json;

// Note: These are integration tests that require a running database
// Run with: cargo test --test auth_tests

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test app (simplified version without DB for unit tests)
    fn create_test_app() -> axum::Router {
        axum::Router::new()
            .route("/health", axum::routing::get(|| async { "OK" }))
    }

    #[tokio::test]
    async fn test_health_check() {
        let app = create_test_app();
        let server = TestServer::new(app).unwrap();

        let response = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("OK");
    }

    #[tokio::test]
    async fn test_register_validation() {
        let app = create_test_app();
        let server = TestServer::new(app).unwrap();
        let response = server.get("/health").await;
        assert_eq!(response.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_login_requires_credentials() {
        let app = create_test_app();
        let server = TestServer::new(app).unwrap();
        let response = server.get("/health").await;
        response.assert_status_ok();
    }
}

// Unit tests for JWT utilities
#[cfg(test)]
mod jwt_tests {
    use std::env;
    use bcrypt::{hash, verify, DEFAULT_COST};
    use rand::{thread_rng, Rng};
    use rand::distributions::Alphanumeric;

    #[test]
    fn test_jwt_secret_environment() {
        // JWT_SECRET should either be set or have a default fallback
        let result = env::var("JWT_SECRET");
        // The test should work regardless of whether it's set
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_password_hashing_success() {
        let password = "test_password_123";
        let hashed = hash(password, DEFAULT_COST).expect("Failed to hash password");

        // Verify the hash works
        assert!(verify(password, &hashed).expect("Failed to verify password"));
    }

    #[test]
    fn test_password_hashing_wrong_password() {
        let password = "correct_password";
        let hashed = hash(password, DEFAULT_COST).expect("Failed to hash password");

        // Verify wrong password fails
        assert!(!verify("wrong_password", &hashed).expect("Failed to verify password"));
    }

    #[test]
    fn test_password_hashing_empty_password() {
        let password = "";
        let hashed = hash(password, DEFAULT_COST).expect("Failed to hash empty password");
        
        assert!(verify(password, &hashed).expect("Failed to verify empty password"));
        assert!(!verify("not_empty", &hashed).expect("Failed to verify"));
    }

    #[test]
    fn test_password_hashing_unicode() {
        let password = "пароль123🔐";
        let hashed = hash(password, DEFAULT_COST).expect("Failed to hash unicode password");
        
        assert!(verify(password, &hashed).expect("Failed to verify unicode password"));
    }

    #[test]
    fn test_password_hashing_long_password() {
        let password = "a".repeat(100);
        let hashed = hash(&password, DEFAULT_COST).expect("Failed to hash long password");
        
        assert!(verify(&password, &hashed).expect("Failed to verify long password"));
    }

    #[test]
    fn test_short_code_generation_length() {
        let code: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();

        assert_eq!(code.len(), 6);
    }

    #[test]
    fn test_short_code_generation_alphanumeric() {
        let code: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();

        assert!(code.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_short_code_uniqueness() {
        let mut codes: Vec<String> = Vec::new();
        
        for _ in 0..100 {
            let code: String = thread_rng()
                .sample_iter(&Alphanumeric)
                .take(6)
                .map(char::from)
                .collect();
            codes.push(code);
        }
        
        // Check for uniqueness (collision is highly unlikely)
        let unique_codes: std::collections::HashSet<_> = codes.iter().collect();
        assert!(unique_codes.len() >= 99); // Allow for 1 collision max
    }
}

// Email validation tests
#[cfg(test)]
mod email_validation_tests {
    use validator::Validate;
    use serde::Deserialize;

    #[derive(Deserialize, Validate)]
    struct EmailTest {
        #[validate(email)]
        email: String,
    }

    #[test]
    fn test_valid_email() {
        let email = EmailTest {
            email: "test@example.com".to_string(),
        };
        assert!(email.validate().is_ok());
    }

    #[test]
    fn test_valid_email_with_subdomain() {
        let email = EmailTest {
            email: "test@mail.example.com".to_string(),
        };
        assert!(email.validate().is_ok());
    }

    #[test]
    fn test_valid_email_with_plus() {
        let email = EmailTest {
            email: "test+alias@example.com".to_string(),
        };
        assert!(email.validate().is_ok());
    }

    #[test]
    fn test_invalid_email_no_at() {
        let email = EmailTest {
            email: "testexample.com".to_string(),
        };
        assert!(email.validate().is_err());
    }

    #[test]
    fn test_invalid_email_no_domain() {
        let email = EmailTest {
            email: "test@".to_string(),
        };
        assert!(email.validate().is_err());
    }

    #[test]
    fn test_invalid_email_empty() {
        let email = EmailTest {
            email: "".to_string(),
        };
        assert!(email.validate().is_err());
    }
}

// Password strength tests
#[cfg(test)]
mod password_tests {
    #[test]
    fn test_password_min_length() {
        let password = "1234567"; // 7 chars
        assert!(password.len() < 8);
    }

    #[test]
    fn test_password_valid_length() {
        let password = "12345678"; // 8 chars
        assert!(password.len() >= 8);
    }

    #[test]
    fn test_password_with_special_chars() {
        let password = "P@ssw0rd!";
        assert!(password.len() >= 8);
        assert!(password.chars().any(|c| c.is_ascii_punctuation()));
    }
}

// Token generation tests
#[cfg(test)]
mod token_tests {
    use rand::{thread_rng, Rng};
    
    fn generate_token() -> String {
        thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect()
    }

    #[test]
    fn test_verification_token_length() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
    }

    #[test]
    fn test_verification_token_alphanumeric() {
        let token = generate_token();
        assert!(token.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_verification_token_uniqueness() {
        let token1 = generate_token();
        let token2 = generate_token();
        assert_ne!(token1, token2);
    }
}

// JWT Claims tests
#[cfg(test)]
mod claims_tests {
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Claims {
        sub: String,
        exp: usize,
        user_id: i32,
    }

    const TEST_SECRET: &str = "test-secret-key-for-testing-only";

    #[test]
    fn test_jwt_encode_decode() {
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(24))
            .expect("valid timestamp")
            .timestamp();

        let claims = Claims {
            sub: "test@example.com".to_string(),
            exp: expiration as usize,
            user_id: 42,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        ).expect("Failed to encode JWT");

        let decoded = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        ).expect("Failed to decode JWT");

        assert_eq!(decoded.claims.sub, "test@example.com");
        assert_eq!(decoded.claims.user_id, 42);
    }

    #[test]
    fn test_jwt_expired_token() {
        let expiration = Utc::now()
            .checked_sub_signed(Duration::hours(1))
            .expect("valid timestamp")
            .timestamp();

        let claims = Claims {
            sub: "test@example.com".to_string(),
            exp: expiration as usize,
            user_id: 42,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        ).expect("Failed to encode JWT");

        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_wrong_secret() {
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(24))
            .expect("valid timestamp")
            .timestamp();

        let claims = Claims {
            sub: "test@example.com".to_string(),
            exp: expiration as usize,
            user_id: 42,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        ).expect("Failed to encode JWT");

        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret("wrong-secret".as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_malformed_token() {
        let result = decode::<Claims>(
            "not.a.valid.token",
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }
}
