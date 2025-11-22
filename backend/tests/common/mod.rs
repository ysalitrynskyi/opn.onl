use axum::Router;
use sea_orm::{Database, DatabaseConnection};
use std::env;

pub async fn setup_test_db() -> DatabaseConnection {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    Database::connect(&db_url).await.expect("Failed to connect to test database")
}

pub fn get_test_token() -> String {
    // Create a test JWT token
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

