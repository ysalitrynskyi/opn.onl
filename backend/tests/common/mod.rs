use sea_orm::{Database, DatabaseConnection};
use std::env;

pub async fn setup_test_db() -> DatabaseConnection {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let db = Database::connect(&db_url).await.expect("Failed to connect to test database");
    // Ensure the schema exists. Run migrations exactly once per test process:
    // tests run in parallel, and concurrent Migrator::up calls on a fresh
    // database race on creating the seaql_migrations table.
    static MIGRATIONS: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    MIGRATIONS
        .get_or_init(|| async {
            use migration::{Migrator, MigratorTrait};
            Migrator::up(&db, None).await.expect("Failed to run migrations on test database");
        })
        .await;
    db
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

/// Spawn the REAL application router (all routes, all middleware) backed by
/// the Postgres database from `DATABASE_URL`, plus a handle to that database
/// for test fixtures. This is what integration tests should use — never a
/// stub router.
#[allow(dead_code)]
pub async fn spawn_real_app() -> (axum_test::TestServer, DatabaseConnection) {
    // Pin environment-dependent middleware before dotenvy runs so a developer
    // .env (e.g. FORCE_HTTPS=true) can't change test behavior: dotenvy never
    // overrides variables that are already set.
    std::env::set_var("FORCE_HTTPS", "false");
    std::env::set_var("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        std::env::set_var("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }

    let db = setup_test_db().await;
    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("failed to start test server");
    (server, db)
}

/// Flip `email_verified` directly in the database (there is no SMTP in tests,
/// so the verification email flow can't be exercised end-to-end here).
#[allow(dead_code)]
pub async fn mark_email_verified(db: &DatabaseConnection, user_id: i32) {
    use opn_onl_backend::entity::users;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db error")
        .expect("user not found");
    let mut active: users::ActiveModel = user.into();
    active.email_verified = Set(true);
    active.update(db).await.expect("failed to mark user verified");
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
