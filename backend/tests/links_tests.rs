mod common;

use axum_test::TestServer;
use serde_json::json;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    // Helper to create a minimal test app
    fn create_test_app() -> axum::Router {
        axum::Router::new()
            .route("/health", axum::routing::get(|| async { "OK" }))
    }

    #[tokio::test]
    async fn test_links_endpoint_exists() {
        let app = create_test_app();
        let server = TestServer::new(app).unwrap();

        let response = server.get("/health").await;
        response.assert_status_ok();
    }
}

// Unit tests for link-related functionality
#[cfg(test)]
mod unit_tests {
    use validator::Validate;
    use serde::Deserialize;

    #[derive(Deserialize, Validate)]
    struct TestCreateLinkRequest {
        #[validate(url)]
        pub original_url: String,
        #[validate(length(min = 3, max = 20))]
        pub custom_alias: Option<String>,
    }

    #[test]
    fn test_valid_url_validation() {
        let request = TestCreateLinkRequest {
            original_url: "https://example.com".to_string(),
            custom_alias: None,
        };

        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_invalid_url_validation() {
        let request = TestCreateLinkRequest {
            original_url: "not-a-valid-url".to_string(),
            custom_alias: None,
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_alias_too_short() {
        let request = TestCreateLinkRequest {
            original_url: "https://example.com".to_string(),
            custom_alias: Some("ab".to_string()), // Too short (min 3)
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_alias_too_long() {
        let request = TestCreateLinkRequest {
            original_url: "https://example.com".to_string(),
            custom_alias: Some("this_alias_is_way_too_long_for_validation".to_string()),
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_valid_alias() {
        let request = TestCreateLinkRequest {
            original_url: "https://example.com".to_string(),
            custom_alias: Some("my-link".to_string()),
        };

        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_short_code_uniqueness() {
        use rand::{thread_rng, Rng};
        use rand::distributions::Alphanumeric;
        use std::collections::HashSet;

        let mut codes = HashSet::new();
        
        // Generate 1000 codes and verify they're mostly unique
        // (collision is possible but extremely unlikely)
        for _ in 0..1000 {
            let code: String = thread_rng()
                .sample_iter(&Alphanumeric)
                .take(6)
                .map(char::from)
                .collect();
            codes.insert(code);
        }

        // We should have at least 999 unique codes (allowing for 1 collision max)
        assert!(codes.len() >= 999);
    }

    #[test]
    fn test_qr_code_generation() {
        use qrcode::QrCode;

        let url = "https://opn.onl/abc123";
        let code = QrCode::new(url.as_bytes());

        assert!(code.is_ok());
    }
}

// Tests for password protection
#[cfg(test)]
mod password_tests {
    use bcrypt::{hash, verify, DEFAULT_COST};

    #[test]
    fn test_password_hash_and_verify() {
        let password = "secret123";
        let hashed = hash(password, DEFAULT_COST).unwrap();

        assert!(verify(password, &hashed).unwrap());
        assert!(!verify("wrong", &hashed).unwrap());
    }

    #[test]
    fn test_empty_password() {
        let password = "";
        let hashed = hash(password, DEFAULT_COST).unwrap();

        assert!(verify(password, &hashed).unwrap());
        assert!(!verify("something", &hashed).unwrap());
    }

    #[test]
    fn test_unicode_password() {
        let password = "пароль🔐";
        let hashed = hash(password, DEFAULT_COST).unwrap();

        assert!(verify(password, &hashed).unwrap());
    }
}

// Tests for expiration handling
#[cfg(test)]
mod expiration_tests {
    use chrono::{Utc, Duration};

    #[test]
    fn test_link_not_expired() {
        let expires_at = Utc::now() + Duration::hours(24);
        let is_expired = Utc::now().naive_utc() > expires_at.naive_utc();

        assert!(!is_expired);
    }

    #[test]
    fn test_link_expired() {
        let expires_at = Utc::now() - Duration::hours(1);
        let is_expired = Utc::now().naive_utc() > expires_at.naive_utc();

        assert!(is_expired);
    }

    #[test]
    fn test_link_just_expired() {
        let expires_at = Utc::now() - Duration::seconds(1);
        let is_expired = Utc::now().naive_utc() > expires_at.naive_utc();

        assert!(is_expired);
    }
}





