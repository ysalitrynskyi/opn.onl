//! Blocking an email domain disables its non-admin accounts and leaves admins
//! alone, because an admin has to stay able to unblock the domain. The sign-in
//! paths must honour the same exemption: otherwise the admin works until the
//! session token expires and is then locked out of login, password reset and
//! passkey login.

mod common;
#[path = "common/soft_passkey.rs"]
mod soft_passkey;

use axum_test::TestServer;
use common::{setup_test_db, unique_code, unique_email};
use opn_onl_backend::entity::users;
use opn_onl_backend::handlers::auth::hash_secret_token;
use opn_onl_backend::utils::jwt::{create_jwt, hash_password};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};
use soft_passkey::SoftPasskey;
use std::sync::Arc;

const PASSWORD: &str = "password123";

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

async fn app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    soft_passkey::pin_webauthn_environment();
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
        report: roomy(),
    });
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}

/// `password` is the real password when the test signs in with it; accounts
/// that never do get a placeholder hash (bcrypt is slow).
async fn account(
    db: &DatabaseConnection,
    email: &str,
    admin: bool,
    password: Option<&str>,
) -> users::Model {
    let password_hash = match password {
        Some(password) => hash_password(password).await.expect("hash"),
        None => "not-a-real-hash".to_string(),
    };
    users::ActiveModel {
        email: Set(email.to_string()),
        password_hash: Set(password_hash),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(admin),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

/// Enrol a real passkey for `user` through the registration ceremony, with a
/// software authenticator that can later sign in with it.
async fn enroll_passkey(server: &TestServer, user: &users::Model) -> SoftPasskey {
    let token = create_jwt(user.id, &user.email, user.token_version).unwrap();
    let start = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(&token)
        .json(&json!({ "username": &user.email }))
        .await;
    assert_eq!(start.status_code(), 200, "register start: {}", start.text());
    let options = start.json::<Value>()["options"].clone();
    let authenticator = SoftPasskey::new();
    let finish = server
        .post("/auth/passkey/register/finish")
        .authorization_bearer(&token)
        .json(&json!({
            "username": &user.email,
            "credential": authenticator.registration(&options),
        }))
        .await;
    assert_eq!(
        finish.status_code(),
        200,
        "register finish: {}",
        finish.text()
    );
    authenticator
}

async fn login(server: &TestServer, email: &str, password: &str) -> u16 {
    server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await
        .status_code()
        .as_u16()
}

/// A whole passkey sign-in with `authenticator`: the status `login/finish`
/// answers (200 with a session, or the uniform 401). A blocked non-admin only
/// ever gets a decoy challenge, which no real passkey can satisfy.
async fn passkey_sign_in(server: &TestServer, email: &str, authenticator: &mut SoftPasskey) -> u16 {
    let start = server
        .post("/auth/passkey/login/start")
        .json(&json!({ "username": email }))
        .await;
    assert_eq!(start.status_code(), 200, "start: {}", start.text());
    let start: Value = start.json();
    let finish = server
        .post("/auth/passkey/login/finish")
        .json(&json!({
            "login_id": start["login_id"].as_str().expect("login_id"),
            "credential": authenticator.assertion(&start["options"]),
        }))
        .await;
    finish.status_code().as_u16()
}

async fn stored(db: &DatabaseConnection, id: i32) -> users::Model {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .expect("user row")
}

#[tokio::test]
async fn an_admin_on_a_blocked_domain_can_still_sign_in_and_recover_the_account() {
    let (server, db) = app().await;
    let blocker_email = unique_email();
    let blocker = account(&db, &blocker_email, true, None).await;
    let blocker_token = create_jwt(blocker.id, &blocker.email, blocker.token_version).unwrap();

    let domain = format!("peer-{}.iana.org", unique_code().to_lowercase());
    let peer_email = format!("admin@{domain}");
    let peer = account(&db, &peer_email, true, Some(PASSWORD)).await;
    let mut peer_passkey = enroll_passkey(&server, &peer).await;
    let victim = account(&db, &format!("user@{domain}"), false, None).await;

    let blocked = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(&blocker_token)
        .json(&json!({ "domain": &domain }))
        .await;
    assert_eq!(blocked.status_code(), 201, "block: {}", blocked.text());

    // After the block: the peer admin stays enabled, a regular user is disabled,
    // and an enabled non-admin on the domain (what the block cannot prevent,
    // e.g. a row restored later) is the control for the gate itself.
    assert!(stored(&db, peer.id).await.disabled_at.is_none());
    assert!(stored(&db, victim.id).await.disabled_at.is_some());
    let control_email = format!("late@{domain}");
    let control = account(&db, &control_email, false, Some(PASSWORD)).await;
    let mut control_passkey = enroll_passkey(&server, &control).await;

    // Login.
    let res = server
        .post("/auth/login")
        .json(&json!({ "email": &peer_email, "password": PASSWORD }))
        .await;
    assert_eq!(res.status_code(), 200, "peer admin login: {}", res.text());
    let token = res.json::<Value>()["token"].as_str().unwrap().to_string();
    assert_eq!(
        server
            .get("/admin/stats")
            .authorization_bearer(&token)
            .await
            .status_code(),
        200
    );
    assert_eq!(login(&server, &control_email, PASSWORD).await, 401);
    assert_eq!(login(&server, &victim.email, PASSWORD).await, 401);

    // Forgot password: the reset token is issued for the admin only. (Without
    // SMTP the token is stored and nothing is sent; the answer is the same 200
    // either way, so the stored token is what tells the two apart.)
    for email in [&peer_email, &control_email] {
        let res = server
            .post("/auth/forgot-password")
            .json(&json!({ "email": email }))
            .await;
        assert_eq!(res.status_code(), 200, "forgot: {}", res.text());
    }
    assert!(
        stored(&db, peer.id).await.password_reset_token.is_some(),
        "the admin got a reset token"
    );
    assert!(
        stored(&db, control.id).await.password_reset_token.is_none(),
        "the gate still applies to everyone else"
    );

    // Passkey login (before the reset below: a reset removes the account's
    // passkeys, as account recovery should).
    assert_eq!(
        passkey_sign_in(&server, &peer_email, &mut peer_passkey).await,
        200,
        "the admin signs in with their passkey"
    );
    assert_eq!(
        passkey_sign_in(&server, &control_email, &mut control_passkey).await,
        401,
        "a non-admin on the blocked domain only ever gets a decoy challenge"
    );

    // Reset password with a token the user holds.
    let raw_token = format!("reset-{}", uuid::Uuid::new_v4());
    for id in [peer.id, control.id] {
        let mut user: users::ActiveModel = stored(&db, id).await.into();
        user.password_reset_token = Set(Some(hash_secret_token(&raw_token)));
        user.password_reset_expires = Set(Some(
            chrono::Utc::now().naive_utc() + chrono::Duration::hours(1),
        ));
        user.update(&db).await.unwrap();
        if id == peer.id {
            let res = server
                .post("/auth/reset-password")
                .json(&json!({ "token": &raw_token, "password": "a-new-password-1" }))
                .await;
            assert_eq!(res.status_code(), 200, "admin reset: {}", res.text());
        } else {
            let res = server
                .post("/auth/reset-password")
                .json(&json!({ "token": &raw_token, "password": "a-new-password-1" }))
                .await;
            assert_eq!(res.status_code(), 400, "control reset: {}", res.text());
        }
    }
    assert_eq!(login(&server, &peer_email, "a-new-password-1").await, 200);
}
