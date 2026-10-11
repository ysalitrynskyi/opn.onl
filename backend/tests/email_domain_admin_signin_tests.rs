//! Blocking an email domain disables its non-admin accounts and leaves admins
//! alone, because an admin has to stay able to unblock the domain. The sign-in
//! paths must honour the same exemption: otherwise the admin works until the
//! session token expires and is then locked out of login, password reset and
//! passkey login.

mod common;

use axum_test::TestServer;
use common::{setup_test_db, unique_code, unique_email};
use opn_onl_backend::entity::{passkeys, users};
use opn_onl_backend::handlers::auth::hash_secret_token;
use opn_onl_backend::utils::jwt::{create_jwt, hash_password};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};
use std::sync::Arc;

const PASSWORD: &str = "password123";

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

async fn app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    common::set_env("ENABLE_PASSKEYS", "true");
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

/// A stored passkey that parses, so `login/start` keeps pending state for it.
async fn add_passkey(db: &DatabaseConnection, user_id: i32) {
    let stored = json!({"cred": {
        "cred_id": "uZcVDBVS68E_MtAgeQpElJxldF_6cY9sSvbWqx_qRh8wiu42lyRBRmh5yFeD_r9k130dMbFHBHI9RTFgdJQIzQ",
        "cred": {"type_": "ES256", "key": {"EC_EC2": {
            "curve": "SECP256R1",
            "x": [194,126,127,109,252,23,131,21,252,6,223,99,44,254,140,27,230,17,94,5,133,28,104,41,144,69,171,149,161,26,200,243],
            "y": [143,123,183,156,24,178,21,248,117,159,162,69,171,52,188,252,26,59,6,47,103,92,19,58,117,103,249,0,219,8,95,196],
        }}},
        "counter": 2,
        "transports": null,
        "user_verified": false,
        "backup_eligible": false,
        "backup_state": false,
        "registration_policy": "preferred",
        "extensions": {"cred_protect": "NotRequested", "hmac_create_secret": "NotRequested"},
        "attestation": {"data": "None", "metadata": "None"},
        "attestation_format": "None",
    }})
    .to_string();
    serde_json::from_str::<webauthn_rs::prelude::Passkey>(&stored)
        .expect("the fixture must be a passkey the app can read");
    passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(format!("domain-gate-{}", uuid::Uuid::new_v4())),
        cred_public_key: Set(stored),
        counter: Set(0),
        name: Set(Some("fixture".to_string())),
        created_at: Set(chrono::Utc::now().naive_utc()),
        last_used: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert passkey");
}

async fn login(server: &TestServer, email: &str, password: &str) -> u16 {
    server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await
        .status_code()
        .as_u16()
}

/// Whether `login/start` kept pending state for the username: the finish call
/// then fails the assertion (401) instead of finding no state (400). Start
/// itself answers the same 200 shape either way, by design.
async fn passkey_login_has_pending_state(server: &TestServer, email: &str) -> bool {
    let start = server
        .post("/auth/passkey/login/start")
        .json(&json!({ "username": email }))
        .await;
    assert_eq!(start.status_code(), 200, "start: {}", start.text());
    let finish = server
        .post("/auth/passkey/login/finish")
        .json(&json!({
            "username": email,
            "credential": {
                "id": "AAAA",
                "rawId": "AAAA",
                "type": "public-key",
                "response": {
                    "authenticatorData": "AAAA",
                    "clientDataJSON": "AAAA",
                    "signature": "AAAA"
                }
            }
        }))
        .await;
    match finish.status_code().as_u16() {
        401 => true,
        400 => false,
        other => panic!("unexpected finish status {other}: {}", finish.text()),
    }
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
    add_passkey(&db, peer.id).await;
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
    add_passkey(&db, control.id).await;

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
    assert!(
        passkey_login_has_pending_state(&server, &peer_email).await,
        "the admin's passkey login must start for real"
    );
    assert!(
        !passkey_login_has_pending_state(&server, &control_email).await,
        "a non-admin on the blocked domain still gets only the decoy"
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
