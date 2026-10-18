//! Outgoing mail must not write the recipient's address to the logs.
//!
//! Every verification, reset and welcome mail used to log
//! `Sending email to <address>` at info, so the log files (kept without
//! limit) held a list of every user's mailbox, which the contact handler's own
//! comment calls PII sitting in log aggregation.
//!
//! This is its own test binary. `tracing` caches callsite interest for the
//! process: another test that reaches the same callsite with no subscriber can
//! cache it as disabled, and the capture would then see nothing.

mod common;
use common::smtp_stub;

use std::sync::{Arc, Mutex};

use opn_onl_backend::entity::users;
use opn_onl_backend::utils::email::EmailService;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::json;
use smtp_stub::smtp_stub;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

impl<S> Layer<S> for LogCapture
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields(event.metadata().target().to_string());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn outgoing_mail_logs_the_domain_and_never_the_address() {
    let captured = LogCapture::default();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .with(captured.clone());
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();

    let stub = smtp_stub();
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = common::setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.email_service = Some(Arc::new(EmailService::new()));
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("failed to start test server");

    // A mailbox name nothing else in the log could contain.
    let mailbox = format!("mailbox-{}", uuid::Uuid::new_v4().simple());
    let email = format!("{mailbox}@users.opn.onl");
    let user = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert user");
    assert!(user.id > 0);

    let res = server
        .post("/auth/forgot-password")
        .json(&json!({ "email": email }))
        .await;
    assert_eq!(res.status_code(), 200);
    let res = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": email }))
        .await;
    assert_eq!(res.status_code(), 200);
    // The handlers answer before SMTP is done; the log lines are written while
    // the mail is sent, so wait for both to arrive before reading them.
    assert_eq!(
        stub.wait_for(&email, 2).await.len(),
        2,
        "both mails were sent"
    );

    let lines = captured.0.lock().unwrap().clone();
    let sending: Vec<&String> = lines.iter().filter(|l| l.contains("Sending ")).collect();
    assert_eq!(sending.len(), 2, "one log line per mail: {lines:#?}");
    assert!(
        sending.iter().all(|line| line.contains("users.opn.onl")),
        "the domain is still logged: {sending:#?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains(&mailbox)),
        "the recipient's address must not appear in the logs: {lines:#?}"
    );
}
