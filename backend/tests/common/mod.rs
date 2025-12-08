use axum::Router;
use sea_orm::{Database, DatabaseConnection};
use std::env;

pub async fn setup_test_db() -> DatabaseConnection {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    Database::connect(&db_url).await.expect("Failed to connect to test database")
}

pub fn get_test_token() -> String {
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, Header, EncodingKey};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        exp: usize,
        user_id: i32,
    }

    let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "test_secret".to_string());
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: "test@example.com".to_owned(),
        exp: expiration as usize,
        user_id: 1,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .expect("Failed to create test token")
}

pub fn get_test_admin_token() -> String {
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, Header, EncodingKey};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        exp: usize,
        user_id: i32,
        is_admin: bool,
    }

    let secret = env::var("JWT_SECRET").unwrap_or_else(|_| "test_secret".to_string());
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: "admin@example.com".to_owned(),
        exp: expiration as usize,
        user_id: 1,
        is_admin: true,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .expect("Failed to create admin test token")
}

pub fn create_test_app() -> Router {
    Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
}

/// Generate a unique test email
pub fn unique_email() -> String {
    format!("test_{}@example.com", uuid::Uuid::new_v4())
}

/// Generate a unique short code
pub fn unique_code() -> String {
    use rand::{thread_rng, Rng};
    use rand::distributions::Alphanumeric;
    
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect()
}

/// Test data generators
pub mod test_data {
    pub fn valid_url() -> &'static str {
        "https://example.com/test-page"
    }

    pub fn valid_password() -> &'static str {
        "TestPassword123!"
    }

    pub fn invalid_url() -> &'static str {
        "not-a-valid-url"
    }

    pub fn weak_password() -> &'static str {
        "123"
    }

    pub fn xss_payload() -> &'static str {
        "<script>alert('xss')</script>"
    }

    pub fn sql_injection_payload() -> &'static str {
        "'; DROP TABLE users; --"
    }
}

/// Assertion helpers
pub mod assertions {
    use axum::http::StatusCode;

    pub fn is_success(status: StatusCode) -> bool {
        status.is_success()
    }

    pub fn is_client_error(status: StatusCode) -> bool {
        status.is_client_error()
    }

    pub fn is_unauthorized(status: StatusCode) -> bool {
        status == StatusCode::UNAUTHORIZED
    }

    pub fn is_forbidden(status: StatusCode) -> bool {
        status == StatusCode::FORBIDDEN
    }

    pub fn is_not_found(status: StatusCode) -> bool {
        status == StatusCode::NOT_FOUND
    }

    pub fn is_rate_limited(status: StatusCode) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS
    }
}
