//! Shared helpers for the integration tests.
//!
//! Most tests are built into two binaries, because a test binary links the
//! whole app and one per file made a debug build 20–30 GB. A binary is one
//! process, so its tests share the process environment and run against the
//! shared database at the same time:
//!
//! - **`integration`** runs every test in parallel on one fixed environment,
//!   [`baseline_env`]. A test there may set a variable only to its baseline
//!   value, and must not sweep the whole database.
//! - **`integration_serial`** holds the tests that change the environment or
//!   sweep the database. Each one starts with
//!   `let _serial = common::serial_guard().await;`, so they run one at a time
//!   and the environment is put back after each.
//!
//! The files left at the top of `tests/` stay binaries of their own: a counting
//! global allocator, log capture that depends on `tracing`'s per-process
//! callsite cache, and proxy-header trust, which the app reads once per process.

use sea_orm::{Database, DatabaseConnection};
use std::env;
use std::ffi::OsString;

pub mod smtp_stub;
pub mod soft_passkey;

/// The environment every test in a binary starts from, applied once per
/// process before the first database connection or app.
///
/// Fixed values, so a test cannot see one setting at its start and another
/// after a test in some other file switched it. Each is set only when unset:
/// a binary that pins something else first (proxy-header trust, say) keeps it.
/// The values are what the tests that set them always set: the switches on, the
/// frontend and WebAuthn origin the SMTP stub and the passkey helper use.
pub fn baseline_env() {
    static BASELINE: std::sync::Once = std::sync::Once::new();
    BASELINE.call_once(|| {
        for (key, value) in [
            ("FORCE_HTTPS", "false"),
            ("TRUST_PROXY_HEADERS", "false"),
            ("JWT_SECRET", "integration-test-secret-0123456789abcdef"),
            ("ENABLE_ACCOUNT_DELETION", "true"),
            ("ENABLE_API_KEYS", "true"),
            ("ENABLE_PASSKEYS", "true"),
            ("ENABLE_LINK_IN_BIO", "true"),
            ("FRONTEND_URL", soft_passkey::ORIGIN),
            ("WEBAUTHN_RP_ID", soft_passkey::RP_ID),
        ] {
            if env::var_os(key).is_none() {
                set_env(key, value);
            }
        }
    });
}

/// One test in `integration_serial` running alone. Taking it waits for the
/// test before it to finish; dropping it (after a failed assertion too) puts
/// back exactly the environment variables there were when it was taken, so
/// nothing a test sets leaks into the next one.
#[allow(dead_code)]
pub struct SerialGuard {
    saved: Vec<(OsString, OsString)>,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

/// Take the [`SerialGuard`]. Every test in `integration_serial` calls this first.
#[allow(dead_code)]
pub async fn serial_guard() -> SerialGuard {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let lock = LOCK.lock().await;
    baseline_env();
    SerialGuard {
        saved: env::vars_os().collect(),
        _lock: lock,
    }
}

impl Drop for SerialGuard {
    fn drop(&mut self) {
        let saved: std::collections::HashMap<&OsString, &OsString> =
            self.saved.iter().map(|(k, v)| (k, v)).collect();
        for (key, _) in env::vars_os() {
            if !saved.contains_key(&key) {
                remove_env(&key);
            }
        }
        for (key, value) in &self.saved {
            if env::var_os(key).as_ref() != Some(value) {
                set_env(key, value);
            }
        }
    }
}

pub async fn setup_test_db() -> DatabaseConnection {
    baseline_env();
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let db = Database::connect(&db_url)
        .await
        .expect("Failed to connect to test database");
    // Ensure the schema exists. Run migrations exactly once per test process:
    // tests run in parallel, and concurrent Migrator::up calls on a fresh
    // database race on creating the seaql_migrations table.
    static MIGRATIONS: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    MIGRATIONS
        .get_or_init(|| async {
            use migration::{Migrator, MigratorTrait};
            Migrator::up(&db, None)
                .await
                .expect("Failed to run migrations on test database");
        })
        .await;
    db
}

/// Set a process environment variable from a test.
///
/// `set_var` and `remove_var` are `unsafe` since edition 2024 because another
/// thread may read the environment through libc (`getenv`) while it changes.
/// The app reads its settings through `std::env`, which serialises with these
/// calls. What is left is a libc read on another test thread, a DNS lookup for
/// instance. The suite accepts that, as it did before the edition change.
#[allow(dead_code)]
pub fn set_env<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    // SAFETY: test-only; see the function comment.
    unsafe { env::set_var(key, value) };
}

/// Remove a process environment variable from a test. See [`set_env`].
#[allow(dead_code)]
pub fn remove_env<K: AsRef<std::ffi::OsStr>>(key: K) {
    // SAFETY: test-only; see [`set_env`].
    unsafe { env::remove_var(key) };
}

/// Spawn the REAL application router (all routes, all middleware) backed by
/// the Postgres database from `DATABASE_URL`, plus a handle to that database
/// for test fixtures. This is what integration tests should use — never a
/// stub router.
#[allow(dead_code)]
pub async fn spawn_real_app() -> (axum_test::TestServer, DatabaseConnection) {
    baseline_env();
    // Pin environment-dependent middleware before dotenvy runs so a developer
    // .env (e.g. FORCE_HTTPS=true) can't change test behavior: dotenvy never
    // overrides variables that are already set.
    set_env("FORCE_HTTPS", "false");
    set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }

    let db = setup_test_db().await;
    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("failed to start test server");
    (server, db)
}

/// Spawn the REAL router over an HTTP transport (required for WebSocket
/// upgrades — the default mock transport cannot upgrade) with a REAL
/// `WsState` installed on `AppState`. Returns the shared `WsState` handle so a
/// test can broadcast click events and observe what a connected `/ws` or `/sse`
/// subscriber receives. Same DB/JWT rules as [`spawn_real_app`].
#[allow(dead_code)]
pub async fn spawn_real_app_ws() -> (
    axum_test::TestServer,
    DatabaseConnection,
    std::sync::Arc<opn_onl_backend::handlers::websocket::WsState>,
) {
    use std::sync::Arc;
    let ws_state = Arc::new(opn_onl_backend::handlers::websocket::WsState::new());
    spawn_real_app_ws_with_state(ws_state).await
}

/// WebSocket/SSE app with a short auth-revalidation interval for revocation
/// tests; production keeps the default 30-second interval.
#[allow(dead_code)]
pub async fn spawn_real_app_ws_with_interval(
    interval: std::time::Duration,
) -> (
    axum_test::TestServer,
    DatabaseConnection,
    std::sync::Arc<opn_onl_backend::handlers::websocket::WsState>,
) {
    use std::sync::Arc;
    let ws_state = Arc::new(
        opn_onl_backend::handlers::websocket::WsState::with_auth_revalidate_interval(interval),
    );
    spawn_real_app_ws_with_state(ws_state).await
}

/// WebSocket/SSE app around a `WsState` the test built itself, for settings
/// the plain constructors do not expose (a per-user connection limit).
#[allow(dead_code)]
pub async fn spawn_real_app_ws_with_state(
    ws_state: std::sync::Arc<opn_onl_backend::handlers::websocket::WsState>,
) -> (
    axum_test::TestServer,
    DatabaseConnection,
    std::sync::Arc<opn_onl_backend::handlers::websocket::WsState>,
) {
    baseline_env();
    set_env("FORCE_HTTPS", "false");
    set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }

    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.ws_state = Some(ws_state.clone());

    let server = axum_test::TestServer::builder()
        .http_transport()
        .build(opn_onl_backend::build_router(state))
        .expect("failed to start ws test server");
    (server, db, ws_state)
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
    active
        .update(db)
        .await
        .expect("failed to mark user verified");
}

/// Generate a unique test email
#[allow(dead_code)]
pub fn unique_email() -> String {
    format!("test_{}@users.opn.onl", uuid::Uuid::new_v4())
}

/// Generate a unique short code
#[allow(dead_code)]
pub fn unique_code() -> String {
    use rand::distributions::Alphanumeric;
    use rand::{Rng, thread_rng};

    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect()
}
