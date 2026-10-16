//! `REQUIRE_EMAIL_VERIFICATION` is an explicit switch (default on). Missing SMTP
//! does not turn it off, and turning it off does not mark anyone verified.
//! Guards all go through `verification_satisfied`, so an unverified account on
//! an instance that set the switch false can create links, API keys and start
//! passkey registration. Resend with no SMTP is a 503, not a pretend success.

mod common;

use std::sync::Arc;

use common::{mark_email_verified, unique_code, unique_email};
use opn_onl_backend::entity::users;
use opn_onl_backend::utils::EmailService;
use opn_onl_backend::{AppState, build_router};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

const NOT_CONFIGURED: &str = "Email is not configured on this instance";
const LINK_BLOCK: &str = "Please verify your email address before creating links";
const API_KEY_BLOCK: &str = "Verify your email before creating an API key";
const PASSKEY_BLOCK: &str = "Verify your email before registering a passkey";

async fn spawn_with(
    require_email_verification: bool,
    email_configured: bool,
) -> (axum_test::TestServer, DatabaseConnection) {
    // Same pins as `spawn_real_app`. The switch itself is a field on the
    // state, never a process environment variable: tests run in parallel.
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    common::set_env("ENABLE_API_KEYS", "true");
    common::set_env("ENABLE_PASSKEYS", "true");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }

    let db = common::setup_test_db().await;
    let mut state = AppState::for_tests(db.clone()).await;
    state.require_email_verification = require_email_verification;
    if email_configured {
        state.email_service = Some(Arc::new(EmailService::configured_for_tests()));
    }
    let server = axum_test::TestServer::new(build_router(state)).expect("test server");
    (server, db)
}

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register {email}: {}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["email_verified"], false,
        "register stores unverified: {body}"
    );
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

async fn stored_verified(db: &DatabaseConnection, user_id: i32) -> bool {
    users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user")
        .email_verified
}

async fn stored_token(db: &DatabaseConnection, user_id: i32) -> Option<String> {
    users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user")
        .verification_token
}

fn url(label: &str) -> String {
    format!("https://iana.org/{label}-{}", unique_code())
}

async fn create_link(server: &axum_test::TestServer, token: &str) -> (u16, Value) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url("link") }))
        .await;
    (res.status_code().as_u16(), res.json())
}

async fn create_bulk(server: &axum_test::TestServer, token: &str) -> (u16, Value) {
    let res = server
        .post("/links/bulk")
        .authorization_bearer(token)
        .json(&json!({ "urls": [url("bulk")] }))
        .await;
    (res.status_code().as_u16(), res.json())
}

async fn create_api_key(server: &axum_test::TestServer, token: &str) -> (u16, Value) {
    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(token)
        .json(&json!({ "name": "switch" }))
        .await;
    (res.status_code().as_u16(), res.json())
}

async fn start_passkey(server: &axum_test::TestServer, token: &str, email: &str) -> (u16, Value) {
    let res = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(token)
        .json(&json!({ "username": email }))
        .await;
    (res.status_code().as_u16(), res.json())
}

fn assert_allowed(
    link: (u16, Value),
    bulk: (u16, Value),
    key: (u16, Value),
    passkey: (u16, Value),
) {
    assert_eq!(link.0, 201, "link: {}", link.1);
    assert!(link.1["code"].as_str().is_some(), "link code: {}", link.1);
    assert_eq!(bulk.0, 200, "bulk: {}", bulk.1);
    assert_eq!(
        bulk.1["links"].as_array().map(|rows| rows.len()),
        Some(1),
        "{}",
        bulk.1
    );
    assert_eq!(
        bulk.1["errors"].as_array().map(|rows| rows.len()),
        Some(0),
        "{}",
        bulk.1
    );
    assert_eq!(key.0, 201, "api key: {}", key.1);
    assert!(key.1["key"].as_str().is_some(), "api key secret: {}", key.1);
    assert_eq!(passkey.0, 200, "passkey start: {}", passkey.1);
    assert!(
        passkey.1["options"].is_object(),
        "passkey options: {}",
        passkey.1
    );
}

fn assert_refused(
    link: (u16, Value),
    bulk: (u16, Value),
    key: (u16, Value),
    passkey: (u16, Value),
) {
    assert_eq!(link.0, 403, "link: {}", link.1);
    assert_eq!(link.1["error"], LINK_BLOCK);
    assert_eq!(bulk.0, 403, "bulk: {}", bulk.1);
    assert_eq!(bulk.1["errors"][0], LINK_BLOCK);
    assert_eq!(bulk.1["links"].as_array().map(|rows| rows.len()), Some(0));
    assert_eq!(key.0, 403, "api key: {}", key.1);
    assert_eq!(key.1["error"], API_KEY_BLOCK);
    assert_eq!(passkey.0, 403, "passkey start: {}", passkey.1);
    assert_eq!(passkey.1["error"], PASSKEY_BLOCK);
}

#[tokio::test]
async fn auth_settings_reports_the_switch_and_whether_mail_is_configured() {
    for (require, mail) in [(true, false), (false, false), (true, true), (false, true)] {
        let (server, _) = spawn_with(require, mail).await;
        let res = server.get("/auth/settings").await;
        assert_eq!(res.status_code(), 200, "{}", res.text());
        let body: Value = res.json();
        assert_eq!(
            body["require_email_verification"], require,
            "require={require} mail={mail}: {body}"
        );
        assert_eq!(
            body["email_configured"], mail,
            "require={require} mail={mail}: {body}"
        );
    }
}

#[tokio::test]
async fn unverified_account_can_create_when_verification_is_optional() {
    let (server, db) = spawn_with(false, false).await;
    let settings: Value = server.get("/auth/settings").await.json();
    assert_eq!(settings["require_email_verification"], false);
    assert_eq!(settings["email_configured"], false);

    let email = unique_email();
    let (token, user_id) = register(&server, &email).await;
    assert_allowed(
        create_link(&server, &token).await,
        create_bulk(&server, &token).await,
        create_api_key(&server, &token).await,
        start_passkey(&server, &token, &email).await,
    );
    assert!(
        !stored_verified(&db, user_id).await,
        "the switch does not write email_verified"
    );

    // A verified account on the same instance still can.
    let verified_email = unique_email();
    let (verified_token, verified_id) = register(&server, &verified_email).await;
    mark_email_verified(&db, verified_id).await;
    assert_allowed(
        create_link(&server, &verified_token).await,
        create_bulk(&server, &verified_token).await,
        create_api_key(&server, &verified_token).await,
        start_passkey(&server, &verified_token, &verified_email).await,
    );
}

#[tokio::test]
async fn unverified_account_is_refused_when_verification_is_required() {
    let (server, db) = spawn_with(true, false).await;
    let settings: Value = server.get("/auth/settings").await.json();
    assert_eq!(settings["require_email_verification"], true);
    assert_eq!(settings["email_configured"], false);

    let email = unique_email();
    let (token, user_id) = register(&server, &email).await;
    assert_refused(
        create_link(&server, &token).await,
        create_bulk(&server, &token).await,
        create_api_key(&server, &token).await,
        start_passkey(&server, &token, &email).await,
    );
    assert!(!stored_verified(&db, user_id).await);

    let verified_email = unique_email();
    let (verified_token, verified_id) = register(&server, &verified_email).await;
    mark_email_verified(&db, verified_id).await;
    assert_allowed(
        create_link(&server, &verified_token).await,
        create_bulk(&server, &verified_token).await,
        create_api_key(&server, &verified_token).await,
        start_passkey(&server, &verified_token, &verified_email).await,
    );
}

#[tokio::test]
async fn optional_verification_still_blocks_nobody_when_smtp_is_configured() {
    // Mail being configured must not put the requirement back. An unverified
    // account can create, and a resend is still accepted.
    let (server, db) = spawn_with(false, true).await;
    let settings: Value = server.get("/auth/settings").await.json();
    assert_eq!(settings["require_email_verification"], false);
    assert_eq!(settings["email_configured"], true);

    let email = unique_email();
    let (token, user_id) = register(&server, &email).await;
    let before = stored_token(&db, user_id).await;
    assert!(before.is_some(), "register stores a verification token");
    assert_allowed(
        create_link(&server, &token).await,
        create_bulk(&server, &token).await,
        create_api_key(&server, &token).await,
        start_passkey(&server, &token, &email).await,
    );
    assert!(!stored_verified(&db, user_id).await);

    // Register just reserved this address's verification mail, so a second ask
    // is inside the cooldown: 200, nothing sent, and the token already stored
    // stays. A 503 here would mean the switch had blocked the resend.
    let resend = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": email }))
        .await;
    assert_eq!(resend.status_code(), 200, "{}", resend.text());
    assert!(
        resend.text().contains("If account exists"),
        "{}",
        resend.text()
    );
    assert_eq!(
        stored_token(&db, user_id).await,
        before,
        "the cooldown must leave the token register stored"
    );
    let fresh = unique_email();
    let fresh_id = users::ActiveModel {
        email: Set(fresh.clone()),
        password_hash: Set("not-a-real-hash".into()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(false),
        verification_token: Set(Some("existing-token".into())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert unverified user")
    .id;
    let fresh_resend = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": fresh }))
        .await;
    assert_eq!(fresh_resend.status_code(), 200, "{}", fresh_resend.text());
    assert_ne!(
        stored_token(&db, fresh_id).await.as_deref(),
        Some("existing-token"),
        "with SMTP configured the resend replaces the token"
    );
    let unknown = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": unique_email() }))
        .await;
    assert_eq!(
        (unknown.status_code(), unknown.text()),
        (resend.status_code(), resend.text()),
        "a configured resend stays indistinguishable"
    );

    let verified_email = unique_email();
    let (verified_token, verified_id) = register(&server, &verified_email).await;
    mark_email_verified(&db, verified_id).await;
    let (status, body) = create_link(&server, &verified_token).await;
    assert_eq!(status, 201, "verified control: {body}");
}

#[tokio::test]
async fn resend_without_smtp_is_503_and_leaves_the_token_alone() {
    for require in [false, true] {
        let (server, db) = spawn_with(require, false).await;
        let email = unique_email();
        let (token, user_id) = register(&server, &email).await;
        let before = stored_token(&db, user_id).await;
        assert!(
            before.is_some(),
            "require={require}: register stores a token"
        );

        let anonymous = server
            .post("/auth/resend-verification")
            .json(&json!({ "email": email }))
            .await;
        assert_eq!(
            anonymous.status_code(),
            503,
            "require={require}: {}",
            anonymous.text()
        );
        assert_eq!(anonymous.json::<Value>()["error"], NOT_CONFIGURED);
        assert_eq!(
            stored_token(&db, user_id).await,
            before,
            "require={require}"
        );

        let signed_in = server
            .post("/auth/resend-verification")
            .authorization_bearer(&token)
            .json(&json!({ "email": email }))
            .await;
        assert_eq!(
            signed_in.status_code(),
            503,
            "require={require}: {}",
            signed_in.text()
        );
        assert_eq!(signed_in.json::<Value>()["error"], NOT_CONFIGURED);
        assert_eq!(
            stored_token(&db, user_id).await,
            before,
            "require={require}: signed-in resend rotated the token"
        );

        let unknown = server
            .post("/auth/resend-verification")
            .json(&json!({ "email": unique_email() }))
            .await;
        assert_eq!(unknown.status_code(), 503);
        assert_eq!(unknown.json::<Value>()["error"], NOT_CONFIGURED);

        mark_email_verified(&db, user_id).await;
        let already = server
            .post("/auth/resend-verification")
            .authorization_bearer(&token)
            .json(&json!({ "email": email }))
            .await;
        assert_eq!(
            already.status_code(),
            200,
            "require={require}: {}",
            already.text()
        );
        assert_eq!(
            already.json::<Value>()["message"],
            "Your email address is already verified"
        );
        assert_eq!(
            stored_token(&db, user_id).await,
            before,
            "require={require}: already-verified resend rotated the token"
        );
    }
}
