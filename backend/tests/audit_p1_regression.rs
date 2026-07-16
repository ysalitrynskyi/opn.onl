//! Regression tests for the P1 findings in docs/CODE_AUDIT_2026-07.md.
//! Real router + real Postgres via `common::spawn_real_app`.
//!
//! Covered:
//! - Theme A: the cache-invalidation helper captures a user's active link codes.
//! - Theme B: `/contact` is rate-limited (no longer in the redirect bucket);
//!   bulk create requires auth and is charged per URL.
//! - Passkey revoke bumps `token_version` (session revocation).
//!
//! The rate-limit path classifier itself is unit-tested in
//! `utils::rate_limiter::tests::redirect_classifier_separates_codes_from_api_routes`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{passkeys, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde_json::{json, Value};

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

async fn create_link(server: &axum_test::TestServer, token: &str, url: &str) -> String {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await;
    assert_eq!(res.status_code(), 201, "create link failed: {}", res.text());
    res.json::<Value>()["code"]
        .as_str()
        .expect("code")
        .to_string()
}

/// Theme A: the helper handlers use to know which cached redirects to drop must
/// list exactly the user's active link codes, and drop them once soft-deleted.
#[tokio::test]
async fn active_link_codes_track_soft_delete() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register(&server, &unique_email()).await;
    mark_email_verified(&db, user_id).await;

    let code_a = create_link(&server, &token, "https://iana.org/a").await;
    let code_b = create_link(&server, &token, "https://iana.org/b").await;

    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let codes = opn_onl_backend::handlers::links::active_link_codes_for_user(&state, user_id).await;
    assert!(
        codes.contains(&code_a) && codes.contains(&code_b),
        "helper must list the user's active codes (got {codes:?})"
    );

    // Soft-delete one link through the API; the helper must stop listing it.
    let res = server.get("/links").authorization_bearer(&token).await;
    let id_a = res
        .json::<Value>()
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|l| l["code"].as_str() == Some(code_a.as_str()))
                .and_then(|l| l["id"].as_i64())
        })
        .expect("link id for code_a");
    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [id_a] }))
        .await;
    assert_eq!(del.status_code(), 200, "bulk delete: {}", del.text());

    let codes = opn_onl_backend::handlers::links::active_link_codes_for_user(&state, user_id).await;
    assert!(
        !codes.contains(&code_a) && codes.contains(&code_b),
        "soft-deleted code must drop out of the invalidation set (got {codes:?})"
    );
}

/// Theme B: `POST /contact` used to land in the 100 req/s redirect bucket, so a
/// flood could exhaust the mail quota. It must now be rate-limited (10/hour).
#[tokio::test]
async fn contact_form_is_rate_limited() {
    let (server, _db) = spawn_real_app().await;
    let payload = json!({
        "name": "Test User",
        "email": "sender@iana.org",
        "subject": "Hello",
        "message": "This is a test contact message body."
    });

    // First request is accepted (email is skipped without SMTP, still 200).
    let first = server.post("/contact").json(&payload).await;
    assert_ne!(
        first.status_code(),
        429,
        "first contact request should not be rate-limited: {}",
        first.text()
    );

    // Exceed the 10/hour budget; a later request must be throttled.
    let mut throttled = false;
    for _ in 0..12 {
        if server.post("/contact").json(&payload).await.status_code() == 429 {
            throttled = true;
            break;
        }
    }
    assert!(
        throttled,
        "contact form must be rate-limited once the budget is exceeded"
    );
}

/// Theme B: anonymous bulk create is a rate-limit amplification vector and is now
/// rejected outright.
#[tokio::test]
async fn bulk_create_requires_authentication() {
    let (server, _db) = spawn_real_app().await;
    let res = server
        .post("/links/bulk")
        .json(&json!({ "urls": ["https://iana.org/a", "https://iana.org/b"] }))
        .await;
    assert_eq!(
        res.status_code(),
        401,
        "anonymous bulk create must be 401, got {}: {}",
        res.status_code(),
        res.text()
    );
}

/// Theme B: a single bulk request cannot create more links than the per-hour
/// create budget (100). Extra URLs are reported as rate-limited, not amplified.
#[tokio::test]
async fn bulk_create_is_charged_per_link() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register(&server, &unique_email()).await;
    mark_email_verified(&db, user_id).await;

    let urls: Vec<String> = (0..105)
        .map(|i| format!("https://iana.org/bulk/{i}"))
        .collect();
    let res = server
        .post("/links/bulk")
        .authorization_bearer(&token)
        .json(&json!({ "urls": urls }))
        .await;
    assert_eq!(res.status_code(), 200, "bulk create: {}", res.text());
    let body: Value = res.json();
    let created = body["links"].as_array().map(|a| a.len()).unwrap_or(0);
    let errors = body["errors"].as_array().cloned().unwrap_or_default();

    assert!(
        created <= 100,
        "bulk create must not exceed the 100/hour budget, created {created}"
    );
    assert!(
        errors.iter().any(|e| e
            .as_str()
            .map(|s| s.to_lowercase().contains("rate limit"))
            .unwrap_or(false)),
        "over-budget URLs must be reported as rate-limited (errors: {errors:?})"
    );
}

/// Passkey revoke must bump `token_version`, invalidating existing sessions —
/// the documented revocation invariant that this path previously skipped.
#[tokio::test]
async fn passkey_revoke_bumps_token_version() {
    let (server, db) = spawn_real_app().await;
    let (_token, user_id) = register(&server, &unique_email()).await;

    let before = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    // Insert a passkey directly (the WebAuthn ceremony can't run headless). The
    // user has a password, so deleting this passkey is permitted.
    let passkey = passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(format!("test-cred-{user_id}")),
        cred_public_key: Set("test-public-key".to_string()),
        counter: Set(0),
        name: Set(Some("test-key".to_string())),
        created_at: Set(chrono::Utc::now().naive_utc()),
        last_used: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert passkey");

    let res = server
        .post("/auth/passkey/delete")
        .authorization_bearer(&_token)
        .json(&json!({ "passkey_id": passkey.id }))
        .await;
    assert_eq!(res.status_code(), 200, "passkey delete: {}", res.text());

    let after = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;
    assert_eq!(
        after,
        before + 1,
        "revoking a passkey must bump token_version to revoke sessions"
    );
}
