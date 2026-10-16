//! Limits on the mail a request can trigger.
//!
//! Verification, password-reset, security and contact mail used to share one
//! process-wide hourly budget, and nothing stopped a client from asking for
//! mail again and again. One IP could empty the budget (after which every
//! verification and reset mail was silently dropped while the API said it was
//! sent), and each reset request replaced the token, so the link already in the
//! victim's inbox died.
//!
//! These tests run the real router with a real `EmailService` pointed at a
//! local SMTP server (`tests/common/smtp_stub.rs`), so they count the mail that
//! was actually handed to SMTP. Clients are told apart by `CF-Connecting-IP`,
//! as behind the production proxy.

mod common;
#[path = "common/smtp_stub.rs"]
mod smtp_stub;

use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue};
use axum_test::{TestResponse, TestServer};
use opn_onl_backend::entity::users;
use opn_onl_backend::handlers::auth::hash_secret_token;
use opn_onl_backend::utils::email::EmailService;
use opn_onl_backend::utils::email_limits::CLIENT_HOURLY_CAP;
use opn_onl_backend::utils::jwt::{create_jwt, hash_password};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};
use smtp_stub::smtp_stub;

use common::unique_email;

const CLIENT_A: &str = "198.51.100.1";
const CLIENT_B: &str = "198.51.100.2";

// ---------------------------------------------------------------- fixtures

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

/// The per-IP request limiters are not what these tests are about; the
/// sign-in budget (10 a minute) would stop a test that sends dozens of requests.
fn open_limiters() -> Arc<RateLimiters> {
    Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
        report: roomy(),
    })
}

fn pin_environment() {
    // Pin before dotenvy runs, which never overrides a variable that is set.
    common::set_env("FORCE_HTTPS", "false");
    // Tell clients apart by the header, as behind the production proxy. The
    // setting is read once per process and every test here wants it on.
    common::set_env("TRUST_PROXY_HEADERS", "true");
    // A display name with a comma used to make every email fail to build.
    common::set_env("SMTP_FROM_NAME", "Acme, Inc.");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
}

/// The real router with a configured `EmailService` that has `hourly_limit`
/// mails an hour to spend (`EMAIL_RATE_LIMIT_PER_HOUR`).
async fn mail_app(hourly_limit: u64) -> (TestServer, DatabaseConnection) {
    // Starts the SMTP server and points SMTP_* at it before the service exists.
    smtp_stub();
    pin_environment();
    let db = common::setup_test_db().await;
    let service = Arc::new(EmailService::new().with_hourly_limit(hourly_limit));
    assert!(service.is_configured(), "the SMTP stub is not configured");

    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.email_service = Some(service);
    state.rate_limiters = open_limiters();
    let server =
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server");
    (server, db)
}

/// The same router on an instance with no SMTP at all.
async fn app_without_mail() -> (TestServer, DatabaseConnection) {
    pin_environment();
    let db = common::setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.rate_limiters = open_limiters();
    let server =
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server");
    (server, db)
}

/// A user inserted directly (no HTTP, no bcrypt).
async fn account(db: &DatabaseConnection, verified: bool) -> users::Model {
    users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(verified),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

async fn stored(db: &DatabaseConnection, id: i32) -> users::Model {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("db")
        .expect("user row")
}

fn bearer_for(user: &users::Model) -> String {
    create_jwt(user.id, &user.email, user.token_version).expect("mint token")
}

/// Wait for the mail that requests have handed to background tasks: the
/// handlers answer before SMTP is done, so what the server has received is only
/// complete (and "nothing was sent" only means something) once they finish.
async fn mail_sent() {
    assert!(
        opn_onl_backend::utils::email::wait_for_background_mail(std::time::Duration::from_secs(20))
            .await,
        "background mail did not finish"
    );
}

/// POST `body` to `path` as the client at `ip`. Forged forwarding headers ride
/// along on every request: they must not give anyone a budget of their own.
/// Returns once the mail the request triggered has been handed to SMTP.
async fn post_as(server: &TestServer, ip: &str, path: &str, body: Value) -> TestResponse {
    let forged = format!("203.0.113.{}", uuid::Uuid::new_v4().as_u128() % 250 + 1);
    let res = server
        .post(path)
        .add_header(
            HeaderName::from_static("cf-connecting-ip"),
            HeaderValue::from_str(ip).unwrap(),
        )
        .add_header(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_str(&format!("{forged}, {ip}")).unwrap(),
        )
        .add_header(
            HeaderName::from_static("x-real-ip"),
            HeaderValue::from_str(&forged).unwrap(),
        )
        .json(&body)
        .await;
    mail_sent().await;
    res
}

async fn forgot(server: &TestServer, ip: &str, email: &str) -> TestResponse {
    post_as(
        server,
        ip,
        "/auth/forgot-password",
        json!({ "email": email }),
    )
    .await
}

async fn resend(server: &TestServer, ip: &str, email: &str) -> TestResponse {
    post_as(
        server,
        ip,
        "/auth/resend-verification",
        json!({ "email": email }),
    )
    .await
}

/// Status and body, to compare answers.
fn answer(res: &TestResponse) -> (u16, String) {
    (res.status_code().as_u16(), res.text())
}

// ---------------------------------------------- the mailed link stays valid

/// A second reset request must not replace the token: the link already in the
/// inbox is the one the owner will click.
#[tokio::test]
async fn repeating_a_reset_request_sends_nothing_and_keeps_the_mailed_link_valid() {
    let (server, db) = mail_app(500).await;
    let stub = smtp_stub();
    let user = account(&db, true).await;

    let first = forgot(&server, CLIENT_A, &user.email).await;
    assert_eq!(first.status_code(), 200, "{}", first.text());
    let mails = stub.to(&user.email);
    assert_eq!(mails.len(), 1, "the first request mails the owner");
    assert!(mails[0].subject().contains("Reset your password"));
    assert_eq!(
        mails[0].sender_name(),
        "Acme, Inc.",
        "a sender name with a comma must still send"
    );
    let mailed = mails[0].link_token();
    let after_first = stored(&db, user.id).await.password_reset_token;
    assert_eq!(after_first, Some(hash_secret_token(&mailed)));

    // The same request again, from the same client and from another one (an
    // attacker holding only the victim's address).
    let again = forgot(&server, CLIENT_A, &user.email).await;
    let elsewhere = forgot(&server, CLIENT_B, &user.email).await;
    assert_eq!(
        answer(&again),
        answer(&first),
        "same answer inside the cooldown"
    );
    assert_eq!(answer(&elsewhere), answer(&first));
    assert_eq!(
        stub.to(&user.email).len(),
        1,
        "nothing more is sent inside the cooldown"
    );
    assert_eq!(
        stored(&db, user.id).await.password_reset_token,
        after_first,
        "the stored token must not be replaced inside the cooldown"
    );

    // An address that does not exist gets the same answer too.
    let unknown = forgot(&server, CLIENT_A, &unique_email()).await;
    assert_eq!(answer(&unknown), answer(&first), "no enumeration");

    let reset = post_as(
        &server,
        CLIENT_A,
        "/auth/reset-password",
        json!({ "token": mailed, "password": "brand-new-password-1" }),
    )
    .await;
    assert_eq!(
        reset.status_code(),
        200,
        "the link that was mailed still resets the password: {}",
        reset.text()
    );
}

#[tokio::test]
async fn repeating_a_verification_request_sends_nothing_and_keeps_the_mailed_link_valid() {
    let (server, db) = mail_app(500).await;
    let stub = smtp_stub();
    let user = account(&db, false).await;

    let first = resend(&server, CLIENT_A, &user.email).await;
    assert_eq!(first.status_code(), 200, "{}", first.text());
    let mails = stub.to(&user.email);
    assert_eq!(mails.len(), 1);
    assert!(mails[0].subject().contains("Verify your email"));
    let mailed = mails[0].link_token();

    for ip in [CLIENT_A, CLIENT_B] {
        let again = resend(&server, ip, &user.email).await;
        assert_eq!(
            answer(&again),
            answer(&first),
            "same answer inside the cooldown"
        );
    }
    assert_eq!(stub.to(&user.email).len(), 1, "nothing more is sent");
    assert_eq!(
        stored(&db, user.id).await.verification_token,
        Some(hash_secret_token(&mailed)),
        "the stored token must not be replaced inside the cooldown"
    );

    let verify = post_as(
        &server,
        CLIENT_A,
        "/auth/verify-email",
        json!({ "token": mailed }),
    )
    .await;
    assert_eq!(
        verify.status_code(),
        200,
        "the link that was mailed still verifies: {}",
        verify.text()
    );
}

/// The mail sent when an account is created is the address's first
/// verification mail, so a resend right after it is inside the cooldown.
#[tokio::test]
async fn registering_sends_the_first_verification_mail_and_a_resend_waits() {
    let (server, db) = mail_app(500).await;
    let stub = smtp_stub();
    let email = unique_email();

    let registered = post_as(
        &server,
        CLIENT_A,
        "/auth/register",
        json!({ "email": email, "password": "password123" }),
    )
    .await;
    assert_eq!(registered.status_code(), 201, "{}", registered.text());
    let user_id = registered.json::<Value>()["user_id"].as_i64().unwrap() as i32;
    let mails = stub.to(&email);
    assert_eq!(mails.len(), 1, "registration mails the address once");
    let mailed = mails[0].link_token();

    let again = resend(&server, CLIENT_B, &email).await;
    assert_eq!(again.status_code(), 200, "{}", again.text());
    assert_eq!(stub.to(&email).len(), 1);
    assert_eq!(
        stored(&db, user_id).await.verification_token,
        Some(hash_secret_token(&mailed))
    );
}

// ------------------------------------------------------ one client's budget

/// One client IP may trigger 20 mails an hour; past that the request is
/// answered as if the mail went out, nothing is sent and no token changes. The
/// instance-wide budget is still there for everyone else.
#[tokio::test]
async fn one_client_cannot_trigger_more_than_twenty_mails_an_hour() {
    // 33 an hour in total leaves the account pool 21: one more than a single
    // client may take, and fewer than the 30 requests the attacker makes.
    let (server, db) = mail_app(33).await;
    let stub = smtp_stub();

    let mut targets = Vec::new();
    for _ in 0..30 {
        targets.push(account(&db, false).await);
    }
    let answers = futures::future::join_all(
        targets
            .iter()
            .map(|user| resend(&server, CLIENT_A, &user.email)),
    )
    .await;

    let first = answer(&answers[0]);
    assert_eq!(first.0, 200, "{}", first.1);
    for res in &answers {
        assert_eq!(answer(res), first, "a withheld mail looks like a sent one");
    }
    let mailed = targets
        .iter()
        .filter(|user| !stub.to(&user.email).is_empty())
        .count();
    assert_eq!(
        mailed, CLIENT_HOURLY_CAP,
        "one client IP triggers at most {CLIENT_HOURLY_CAP} mails an hour"
    );
    let mut with_token = 0;
    for user in &targets {
        let has_token = stored(&db, user.id).await.verification_token.is_some();
        assert_eq!(
            has_token,
            !stub.to(&user.email).is_empty(),
            "a request that sent nothing must not touch the account"
        );
        with_token += usize::from(has_token);
    }
    assert_eq!(with_token, CLIENT_HOURLY_CAP);

    // Registering still creates the account, but the address is not mailed.
    let email = unique_email();
    let registered = post_as(
        &server,
        CLIENT_A,
        "/auth/register",
        json!({ "email": email, "password": "password123" }),
    )
    .await;
    assert_eq!(registered.status_code(), 201, "{}", registered.text());
    assert!(registered.json::<Value>()["token"].is_string());
    assert!(
        stub.to(&email).is_empty(),
        "a client past its budget does not get its sign-up mailed"
    );

    // Someone on another IP still gets their mail: the attacker did not take
    // the instance-wide budget.
    let victim = account(&db, false).await;
    let res = resend(&server, CLIENT_B, &victim.email).await;
    assert_eq!(answer(&res), first);
    assert_eq!(
        stub.to(&victim.email).len(),
        1,
        "a client past its budget must not starve the others"
    );
}

/// A request the client budget refuses must not start the address's cooldown:
/// otherwise an over-budget client could keep a victim from ever getting a
/// recovery mail just by asking for it.
#[tokio::test]
async fn a_refused_request_does_not_put_the_address_in_cooldown() {
    let (server, db) = mail_app(500).await;
    let stub = smtp_stub();

    for _ in 0..CLIENT_HOURLY_CAP {
        let user = account(&db, false).await;
        resend(&server, CLIENT_A, &user.email).await;
        assert_eq!(stub.to(&user.email).len(), 1);
    }

    let victim = account(&db, false).await;
    let refused = resend(&server, CLIENT_A, &victim.email).await;
    assert_eq!(refused.status_code(), 200);
    assert!(
        stub.to(&victim.email).is_empty(),
        "the attacker's IP is over budget"
    );

    let res = resend(&server, CLIENT_B, &victim.email).await;
    assert_eq!(res.status_code(), 200);
    assert_eq!(
        stub.to(&victim.email).len(),
        1,
        "the victim's own request is served; the refused one spent nothing"
    );
}

// ------------------------------------------------------------ separate pools

/// The account pool (verification, reset, welcome) can run dry, but the
/// "your password was changed" notice has a pool of its own.
#[tokio::test]
async fn a_password_changed_notice_is_sent_when_the_account_pool_is_spent() {
    // 10 an hour: 6 for account mail, 2 for security notices, 1 for contact,
    // 1 for expiry reminders.
    let (server, db) = mail_app(10).await;
    let stub = smtp_stub();

    for _ in 0..6 {
        let user = account(&db, false).await;
        let res = resend(&server, CLIENT_A, &user.email).await;
        assert_eq!(res.status_code(), 200);
        assert_eq!(stub.to(&user.email).len(), 1);
    }

    // The account pool is spent: the request is answered like any other, but
    // nothing is sent and the account is left alone.
    let starved = account(&db, false).await;
    let res = resend(&server, CLIENT_B, &starved.email).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert!(stub.to(&starved.email).is_empty());
    assert!(
        stored(&db, starved.id).await.verification_token.is_none(),
        "no token is issued when the mail cannot be sent"
    );

    // A signed-in user is told the truth.
    let own = account(&db, false).await;
    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer(bearer_for(&own))
        .json(&json!({ "email": own.email }))
        .await;
    assert_eq!(res.status_code(), 503, "{}", res.text());
    let retry_after: u64 = res.header("retry-after").to_str().unwrap().parse().unwrap();
    assert!((1..=3600).contains(&retry_after));
    assert!(
        res.json::<Value>()["error"]
            .as_str()
            .unwrap()
            .contains("hourly email limit")
    );

    // The security notice still goes out.
    let mut owner = account(&db, true).await;
    let mut active: users::ActiveModel = owner.clone().into();
    active.password_hash = Set(hash_password("old-password-123").await.unwrap());
    owner = active.update(&db).await.unwrap();
    let res = server
        .post("/auth/change-password")
        .authorization_bearer(bearer_for(&owner))
        .json(&json!({
            "current_password": "old-password-123",
            "new_password": "new-password-456"
        }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let notices = stub.wait_for(&owner.email, 1).await;
    assert_eq!(
        notices.len(),
        1,
        "the password-changed notice must not be starved by account mail"
    );
    assert!(notices[0].subject().contains("Your password was changed"));
}

static CONTACT_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn contact_message() -> Value {
    json!({
        "name": "A Visitor",
        "email": "visitor@visitors.test",
        "subject": "Hello",
        "message": "A message long enough to pass validation."
    })
}

/// The contact form has a pool of its own: a flood of messages cannot use up
/// what verification and reset mail need.
#[tokio::test]
async fn the_contact_form_cannot_use_up_the_account_pool() {
    let _env = CONTACT_ENV.lock().await;
    let owner = format!("owner-{}@contact.test", uuid::Uuid::new_v4());
    common::set_env("ADMIN_EMAIL", &owner);
    // 3 an hour: one mail each for account mail, security notices, contact
    // and expiry reminders.
    let (server, db) = mail_app(3).await;
    let stub = smtp_stub();

    let statuses = [
        server.post("/contact").json(&contact_message()).await,
        server.post("/contact").json(&contact_message()).await,
        server.post("/contact").json(&contact_message()).await,
    ]
    .map(|res| res.status_code().as_u16());
    assert_eq!(
        statuses[0], 200,
        "the pool has room for one contact message"
    );
    assert_eq!(stub.to(&owner).len(), 1);
    assert_ne!(statuses[2], 200, "the contact pool is spent: {statuses:?}");
    assert_eq!(stub.to(&owner).len(), 1);

    let user = account(&db, false).await;
    let res = resend(&server, CLIENT_A, &user.email).await;
    assert_eq!(res.status_code(), 200);
    assert_eq!(
        stub.to(&user.email).len(),
        1,
        "contact messages must not use up the account pool"
    );
    common::remove_env("ADMIN_EMAIL");
}

// ------------------------------------------------------------ contact address

/// With no `ADMIN_EMAIL` the contact form says so instead of mailing the
/// upstream project's mailbox (or the From address) and claiming success.
#[tokio::test]
async fn the_contact_form_mails_only_the_configured_address() {
    let _env = CONTACT_ENV.lock().await;
    let (server, _db) = mail_app(500).await;
    let stub = smtp_stub();

    for unset in [None, Some(""), Some("   ")] {
        match unset {
            None => common::remove_env("ADMIN_EMAIL"),
            Some(blank) => common::set_env("ADMIN_EMAIL", blank),
        }
        let res = server.post("/contact").json(&contact_message()).await;
        assert_eq!(
            res.status_code(),
            503,
            "ADMIN_EMAIL={unset:?}: {}",
            res.text()
        );
        let body: Value = res.json();
        assert_eq!(body["success"], false);
        assert!(
            body["message"].as_str().unwrap().contains("not delivered"),
            "{body}"
        );
    }
    let strays = ["admin@opn.onl", "noreply@mail.test", "support@opn.onl"];
    assert!(
        stub.all()
            .iter()
            .all(|mail| !strays.contains(&mail.rcpt.as_str())),
        "the message went to a default address: {:?}",
        stub.all()
    );

    let owner = format!("owner-{}@contact.test", uuid::Uuid::new_v4());
    common::set_env("ADMIN_EMAIL", &owner);
    let res = server.post("/contact").json(&contact_message()).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let delivered = stub.to(&owner);
    assert_eq!(
        delivered.len(),
        1,
        "the configured address gets the message"
    );
    assert!(
        delivered[0]
            .data
            .contains("Reply-To: visitor@visitors.test")
    );
    common::remove_env("ADMIN_EMAIL");
}

// ---------------------------------- a signed-in user asking for their own mail

#[tokio::test]
async fn a_signed_in_user_is_told_when_the_server_cannot_send_email() {
    let (server, db) = app_without_mail().await;
    let user = account(&db, false).await;

    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer(bearer_for(&user))
        .json(&json!({ "email": user.email }))
        .await;
    assert_eq!(res.status_code(), 503, "{}", res.text());
    assert_eq!(
        res.json::<Value>()["error"],
        "Email is not configured on this instance"
    );

    // No SMTP is the same 503 for an anonymous caller, known or not, so the
    // refusal still reveals nothing.
    let anonymous = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": user.email }))
        .await;
    assert_eq!(anonymous.status_code(), 503, "{}", anonymous.text());
    assert_eq!(
        anonymous.json::<Value>()["error"],
        "Email is not configured on this instance"
    );

    // Nothing to resend for a verified account, with or without mail.
    let verified = account(&db, true).await;
    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer(bearer_for(&verified))
        .json(&json!({ "email": verified.email }))
        .await;
    assert_eq!(res.status_code(), 200);
    assert!(res.text().contains("already verified"), "{}", res.text());
}

#[tokio::test]
async fn a_signed_in_user_inside_the_cooldown_gets_429_with_retry_after() {
    let (server, db) = mail_app(500).await;
    let stub = smtp_stub();
    let user = account(&db, false).await;
    let token = bearer_for(&user);
    let ask = || {
        server
            .post("/auth/resend-verification")
            .authorization_bearer(&token)
            .json(&json!({ "email": user.email }))
    };

    let first = ask().await;
    assert_eq!(first.status_code(), 200, "{}", first.text());
    assert_eq!(first.json::<Value>()["message"], "Verification email sent");
    let mailed = stub.to(&user.email);
    assert_eq!(mailed.len(), 1);
    let after_first = stored(&db, user.id).await.verification_token;

    let second = ask().await;
    assert_eq!(second.status_code(), 429, "{}", second.text());
    let retry_after: u64 = second
        .header("retry-after")
        .to_str()
        .unwrap()
        .parse()
        .expect("Retry-After is a number of seconds");
    assert!(
        (1..=300).contains(&retry_after),
        "Retry-After {retry_after} is within the five minute cooldown"
    );
    let body: Value = second.json();
    assert_eq!(body["error"], "Too many requests");
    assert_eq!(body["retry_after"], retry_after);
    assert!(
        body["message"].as_str().unwrap().contains("inbox"),
        "the message says where the mail is: {body}"
    );
    assert_eq!(stub.to(&user.email).len(), 1, "nothing more was sent");
    assert_eq!(
        stored(&db, user.id).await.verification_token,
        after_first,
        "the mailed link stays valid"
    );

    let verify = server
        .post("/auth/verify-email")
        .json(&json!({ "token": mailed[0].link_token() }))
        .await;
    assert_eq!(verify.status_code(), 200, "{}", verify.text());
}

/// Only the owner of the account named in the body gets the specific answers;
/// everyone else gets the uniform one, so the endpoint stays no oracle.
#[tokio::test]
async fn asking_about_another_account_while_signed_in_gets_the_uniform_answer() {
    let (server, db) = mail_app(500).await;
    let me = account(&db, false).await;
    let someone_else = account(&db, false).await;

    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer(bearer_for(&me))
        .json(&json!({ "email": someone_else.email }))
        .await;
    assert_eq!(res.status_code(), 200);
    assert!(res.text().contains("If account exists"), "{}", res.text());

    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer("not-a-valid-token")
        .json(&json!({ "email": me.email }))
        .await;
    assert_eq!(
        res.status_code(),
        401,
        "credentials that do not authenticate are refused, not downgraded"
    );
}
