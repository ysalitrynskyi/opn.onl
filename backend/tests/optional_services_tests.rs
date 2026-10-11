//! Optional services (email, S3 backup) and compose.
//!
//! The compose files pass every optional setting as `NAME: ${NAME:-}`, so a
//! service the operator never configured reaches the backend with empty
//! `SMTP_*` / `BACKUP_S3_*` variables, not missing ones. An empty value used to
//! count as set: `/health` said email was configured, the contact form tried to
//! send through a host called "" and answered 500, and every register or reset
//! request spent the instance's hourly mail budget on a send that could not
//! work.

mod common;

use std::sync::Arc;

use opn_onl_backend::utils::email::EmailService;
use opn_onl_backend::{AppState, build_router};
use serde_json::{Value, json};

/// The tests set process-wide environment that the services read when they are
/// built. Run them one at a time.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const OPTIONAL_VARS: &[&str] = &[
    "SMTP_HOST",
    "SMTP_USER",
    "SMTP_PASS",
    "SMTP_TLS",
    "SMTP_PORT",
    "SMTP_FROM_EMAIL",
    "ADMIN_EMAIL",
    "BACKUP_S3_ENDPOINT",
    "BACKUP_S3_BUCKET",
    "BACKUP_S3_ACCESS_KEY",
    "BACKUP_S3_SECRET_KEY",
];

/// Takes the lock, starts from a clean slate and restores nothing afterwards:
/// every other test in this binary starts from a clean slate too.
async fn lock_and_clear() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().await;
    for var in OPTIONAL_VARS {
        common::remove_env(var);
    }
    guard
}

fn set_all(pairs: &[(&str, &str)]) {
    for (key, value) in pairs {
        common::set_env(key, value);
    }
}

/// The real router with services built from the environment as it is now, the
/// way `main` builds them. The email service is installed even when it is not
/// configured, so the handlers' own gate is what gets exercised.
async fn app_from_env() -> axum_test::TestServer {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    let db = common::setup_test_db().await;
    let mut state = AppState::for_tests(db).await;
    state.email_service = Some(Arc::new(EmailService::new()));
    axum_test::TestServer::new(build_router(state)).expect("test server")
}

fn contact_message() -> Value {
    json!({
        "name": "Visitor",
        "email": "visitor@example.test",
        "subject": "Hello",
        "message": "A message long enough to pass validation.",
    })
}

#[tokio::test]
async fn empty_smtp_variables_mean_email_is_not_configured() {
    let _env = lock_and_clear().await;
    set_all(&[
        ("SMTP_HOST", ""),
        ("SMTP_USER", ""),
        ("SMTP_PASS", ""),
        ("SMTP_PORT", "587"),
        ("SMTP_TLS", "starttls"),
        ("ADMIN_EMAIL", "owner@example.test"),
    ]);
    let server = app_from_env().await;

    let health: Value = server.get("/health").await.json();
    assert_eq!(health["email"], "disabled", "{health}");

    let res = server.post("/contact").json(&contact_message()).await;
    assert_eq!(
        res.status_code(),
        503,
        "an unconfigured instance says so instead of failing to send: {}",
        res.text()
    );
}

#[tokio::test]
async fn blank_smtp_variables_with_stray_whitespace_are_not_configured_either() {
    let _env = lock_and_clear().await;
    set_all(&[
        ("SMTP_HOST", "   "),
        ("SMTP_USER", "\t"),
        ("SMTP_PASS", " "),
        ("ADMIN_EMAIL", "owner@example.test"),
    ]);
    let server = app_from_env().await;

    let health: Value = server.get("/health").await.json();
    assert_eq!(health["email"], "disabled", "{health}");
}

#[tokio::test]
async fn a_partly_filled_smtp_setup_is_still_not_configured() {
    let _env = lock_and_clear().await;
    set_all(&[
        ("SMTP_HOST", "smtp.example.test"),
        ("SMTP_USER", ""),
        ("SMTP_PASS", ""),
    ]);
    let server = app_from_env().await;

    let health: Value = server.get("/health").await.json();
    assert_eq!(health["email"], "disabled", "{health}");
}

/// Control: real values still configure the service. Building the transport
/// does not connect, so no server is needed.
#[tokio::test]
async fn filled_in_smtp_variables_still_configure_email() {
    let _env = lock_and_clear().await;
    set_all(&[
        ("SMTP_HOST", "smtp.example.test"),
        ("SMTP_USER", "mailer"),
        ("SMTP_PASS", "pass word with spaces "),
        ("SMTP_TLS", "none"),
    ]);
    let server = app_from_env().await;

    let health: Value = server.get("/health").await.json();
    assert_eq!(health["email"], "configured", "{health}");
}

#[tokio::test]
async fn empty_backup_variables_mean_backup_is_not_configured() {
    let _env = lock_and_clear().await;
    set_all(&[
        ("BACKUP_S3_ENDPOINT", ""),
        ("BACKUP_S3_BUCKET", "opn-backups"),
        ("BACKUP_S3_ACCESS_KEY", ""),
        ("BACKUP_S3_SECRET_KEY", ""),
    ]);
    let server = app_from_env().await;

    let health: Value = server.get("/health").await.json();
    assert_eq!(health["backup"], "disabled", "{health}");
}

/// Control: a complete backup setup is configured. Building the client does
/// not connect.
#[tokio::test]
async fn filled_in_backup_variables_still_configure_backup() {
    let _env = lock_and_clear().await;
    set_all(&[
        ("BACKUP_S3_ENDPOINT", "https://s3.example.test"),
        ("BACKUP_S3_BUCKET", "opn-backups"),
        ("BACKUP_S3_ACCESS_KEY", "access"),
        ("BACKUP_S3_SECRET_KEY", "secret"),
    ]);
    let server = app_from_env().await;

    let health: Value = server.get("/health").await.json();
    assert_eq!(health["backup"], "configured", "{health}");
}
