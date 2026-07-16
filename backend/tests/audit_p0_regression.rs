//! Regression tests for the P0 findings in docs/CODE_AUDIT_2026-07.md.
//! Real router + real Postgres via `common::spawn_real_app`.
//!
//! Covered:
//! - P0-1: `require_admin` must honor `token_version` (JWT revocation) on /admin/*.
//! - P0-2: the redirect password path must enforce the anti-bruteforce limiter.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{json, Value};

/// Register through the real handler. Returns (jwt, user_id); the token carries
/// `token_version` 0.
async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register failed: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

async fn promote_to_admin(db: &DatabaseConnection, user_id: i32) {
    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("promote");
}

/// Bump `token_version`, mirroring what a password change/reset does — this is
/// the revocation signal that must invalidate outstanding JWTs.
async fn bump_token_version(db: &DatabaseConnection, user_id: i32) -> i32 {
    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user");
    let next = user.token_version + 1;
    let mut active: users::ActiveModel = user.into();
    active.token_version = Set(next);
    active.update(db).await.expect("bump token_version");
    next
}

/// P0-1: a pre-reset admin JWT must stop working the moment `token_version` is
/// bumped. Before the fix, `require_admin` ignored `token_version`, so the old
/// token kept full /admin/* access for its full lifetime.
#[tokio::test]
async fn admin_jwt_is_revoked_when_token_version_bumped() {
    let (server, db) = spawn_real_app().await;
    let email = unique_email();
    let (old_token, user_id) = register(&server, &email).await;
    promote_to_admin(&db, user_id).await;

    // Sanity: the admin token works before revocation.
    let res = server
        .get("/admin/stats")
        .authorization_bearer(&old_token)
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "admin token should work before revocation: {}",
        res.text()
    );

    // Simulate "log out everywhere" via a credential change.
    let new_version = bump_token_version(&db, user_id).await;

    // The pre-reset token must now be rejected (regression guard).
    let res = server
        .get("/admin/stats")
        .authorization_bearer(&old_token)
        .await;
    assert_eq!(
        res.status_code(),
        401,
        "revoked admin token must be 401 on /admin/stats, got {}",
        res.text()
    );

    // A freshly issued token with the new version still works — this is
    // revocation of old tokens, not a lockout of the account.
    let fresh_token = create_jwt(user_id, &email, new_version).expect("mint token");
    let res = server
        .get("/admin/stats")
        .authorization_bearer(&fresh_token)
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "re-issued admin token should work: {}",
        res.text()
    );
}

/// P0-2: repeated wrong passwords on the GET /:code redirect path must hit the
/// 5/min anti-bruteforce limiter. Before the fix this path bypassed it entirely
/// (it was classified as a plain redirect at 100 req/s).
#[tokio::test]
async fn redirect_password_attempts_are_rate_limited() {
    let (server, db) = spawn_real_app().await;
    let email = unique_email();
    let (token, user_id) = register(&server, &email).await;
    mark_email_verified(&db, user_id).await;

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/secret",
            "password": "correct-horse-battery-staple"
        }))
        .await;
    assert_eq!(
        res.status_code(),
        201,
        "create password link failed: {}",
        res.text()
    );
    let code = res.json::<Value>()["code"]
        .as_str()
        .expect("code")
        .to_string();

    let pw_header = axum::http::HeaderName::from_static("x-link-password");
    let wrong = axum::http::HeaderValue::from_static("definitely-wrong");

    // In tests TRUST_PROXY_HEADERS is off, so all attempts share one IP bucket
    // keyed by code. The limiter allows 5 attempts/min: the first 5 are rejected
    // as invalid (401), the 6th is throttled (429).
    for attempt in 1..=5 {
        let res = server
            .get(&format!("/{}", code))
            .add_header(pw_header.clone(), wrong.clone())
            .await;
        assert_eq!(
            res.status_code(),
            401,
            "attempt {attempt} should be 401 (invalid password), got {}: {}",
            res.status_code(),
            res.text()
        );
    }

    let res = server
        .get(&format!("/{}", code))
        .add_header(pw_header, wrong)
        .await;
    assert_eq!(
        res.status_code(),
        429,
        "6th attempt must be rate-limited (429), got {}: {}",
        res.status_code(),
        res.text()
    );
}
