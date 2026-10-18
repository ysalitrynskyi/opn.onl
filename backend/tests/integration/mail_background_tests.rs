//! Requests that send mail must answer without waiting for SMTP.
//!
//! They used to await the send, so the response time told an address with an
//! account from one without (the endpoints are meant to answer every address
//! alike), and a slow or unreachable mail server stalled sign-up and recovery
//! for as long as lettre waits, a minute per command.
//!
//! The tests run the real router with a real `EmailService` against the local
//! SMTP server in `tests/common/smtp_stub.rs`, with delivery to one address held
//! back. "The response did not wait" is then decided by what the server did,
//! not by a stopwatch: a handler that waits for the held mail never answers.

use crate::common;
use crate::common::smtp_stub;

use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use axum_test::{TestResponse, TestServer};
use chrono::Utc;
use common::unique_email;
use opn_onl_backend::entity::users;
use opn_onl_backend::utils::email::EmailService;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::{Value, json};
use smtp_stub::smtp_stub;

// ---------------------------------------------------------------- fixtures

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

/// The real router with a real `EmailService` pointed at the SMTP stub.
async fn mail_app() -> (TestServer, DatabaseConnection) {
    // Starts the SMTP server and points SMTP_* at it before the service exists.
    smtp_stub();
    // Pin before dotenvy runs, which never overrides a variable that is set.
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = common::setup_test_db().await;
    let service = Arc::new(EmailService::new());
    assert!(service.is_configured(), "the SMTP stub is not configured");
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.email_service = Some(service);
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        auth: roomy(),
        ..Default::default()
    });
    let server =
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server");
    (server, db)
}

async fn account(db: &DatabaseConnection, verified: bool) -> users::Model {
    users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(verified),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

/// Status and body, to compare answers.
fn answer(res: &TestResponse) -> (u16, String) {
    (res.status_code().as_u16(), res.text())
}

/// A response that arrives although the mail server is stuck. A handler that
/// waits for SMTP never gets here: lettre gives a silent server a minute.
async fn while_mail_is_stuck(request: impl IntoFuture<Output = TestResponse>) -> TestResponse {
    tokio::time::timeout(Duration::from_secs(20), request)
        .await
        .expect("the response waited for the mail server")
}

#[tokio::test]
async fn forgot_password_answers_without_waiting_for_the_mail_server() {
    let (server, db) = mail_app().await;
    let stub = smtp_stub();
    let user = account(&db, true).await;
    let hold = stub.hold_mail_for(&user.email);

    let known = while_mail_is_stuck(
        server
            .post("/auth/forgot-password")
            .json(&json!({ "email": user.email })),
    )
    .await;
    let unknown = while_mail_is_stuck(
        server
            .post("/auth/forgot-password")
            .json(&json!({ "email": unique_email() })),
    )
    .await;

    assert_eq!(known.status_code(), 200, "{}", known.text());
    assert_eq!(answer(&known), answer(&unknown), "no enumeration");
    assert!(
        stub.to(&user.email).is_empty(),
        "the mail is still on its way when the answer is out"
    );

    hold.release();
    let mails = stub.wait_for(&user.email, 1).await;
    assert_eq!(mails.len(), 1, "the reset mail is sent all the same");
    assert!(mails[0].subject().contains("Reset your password"));
}

#[tokio::test]
async fn resend_verification_answers_without_waiting_for_the_mail_server() {
    let (server, db) = mail_app().await;
    let stub = smtp_stub();
    let user = account(&db, false).await;
    let hold = stub.hold_mail_for(&user.email);

    let known = while_mail_is_stuck(
        server
            .post("/auth/resend-verification")
            .json(&json!({ "email": user.email })),
    )
    .await;
    let unknown = while_mail_is_stuck(
        server
            .post("/auth/resend-verification")
            .json(&json!({ "email": unique_email() })),
    )
    .await;

    assert_eq!(known.status_code(), 200, "{}", known.text());
    assert_eq!(answer(&known), answer(&unknown), "no enumeration");
    assert!(stub.to(&user.email).is_empty());

    hold.release();
    let mails = stub.wait_for(&user.email, 1).await;
    assert_eq!(mails.len(), 1, "the verification mail is sent all the same");
    assert!(mails[0].subject().contains("Verify your email"));
}

#[tokio::test]
async fn register_answers_without_waiting_for_the_mail_server() {
    let (server, _db) = mail_app().await;
    let stub = smtp_stub();
    let email = unique_email();
    let hold = stub.hold_mail_for(&email);

    let res = while_mail_is_stuck(
        server
            .post("/auth/register")
            .json(&json!({ "email": email, "password": "password123" })),
    )
    .await;

    assert_eq!(res.status_code(), 201, "{}", res.text());
    assert!(res.json::<Value>()["token"].is_string());
    assert!(stub.to(&email).is_empty());

    hold.release();
    let mails = stub.wait_for(&email, 1).await;
    assert_eq!(mails.len(), 1, "the sign-up is mailed all the same");
    assert!(mails[0].subject().contains("Verify your email"));
}

#[tokio::test]
async fn verifying_an_address_answers_without_waiting_for_the_welcome_mail() {
    let (server, _db) = mail_app().await;
    let stub = smtp_stub();
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "{}", res.text());
    let mails = stub.wait_for(&email, 1).await;
    assert_eq!(mails.len(), 1);
    let token = mails[0].link_token();

    let hold = stub.hold_mail_for(&email);
    let res = while_mail_is_stuck(
        server
            .post("/auth/verify-email")
            .json(&json!({ "token": token })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert_eq!(stub.to(&email).len(), 1, "the welcome mail is still held");

    hold.release();
    let mails = stub.wait_for(&email, 2).await;
    assert_eq!(mails.len(), 2);
    assert!(mails[1].subject().contains("Welcome"));
}
