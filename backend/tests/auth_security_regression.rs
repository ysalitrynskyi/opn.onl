//! Regression coverage for credential-boundary and session-revocation fixes.
//! Real router + real Postgres via `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{api_keys, passkeys, users};
use opn_onl_backend::handlers::auth::hash_secret_token;
use opn_onl_backend::handlers::links::hash_api_key;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};

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

async fn promote_directly(db: &DatabaseConnection, user_id: i32) {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("promote admin");
}

/// Session-scoped advisory lock so last-admin and non-last-admin tests cannot
/// interleave while one of them temporarily demotes other live admins.
const ADMIN_COUNT_LOCK: i64 = 872_364_198;

async fn lock_admin_count() -> DatabaseConnection {
    let mut opts = sea_orm::ConnectOptions::new(
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
    );
    opts.max_connections(1);
    let db = sea_orm::Database::connect(opts)
        .await
        .expect("admin-count lock connection");
    db.execute_unprepared(&format!("SELECT pg_advisory_lock({ADMIN_COUNT_LOCK})"))
        .await
        .expect("pg_advisory_lock");
    db
}

async fn unlock_admin_count(lock_db: &DatabaseConnection) {
    let _ = lock_db
        .execute_unprepared(&format!("SELECT pg_advisory_unlock({ADMIN_COUNT_LOCK})"))
        .await;
}

async fn demote_other_live_admins(db: &DatabaseConnection, keep_id: i32) -> Vec<i32> {
    let others: Vec<i32> = users::Entity::find()
        .filter(users::Column::IsAdmin.eq(true))
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::Id.ne(keep_id))
        .all(db)
        .await
        .expect("list other live admins")
        .into_iter()
        .map(|u| u.id)
        .collect();
    for id in &others {
        let user = users::Entity::find_by_id(*id)
            .one(db)
            .await
            .expect("db")
            .expect("user");
        let mut active: users::ActiveModel = user.into();
        active.is_admin = Set(false);
        active.update(db).await.expect("demote other admin");
    }
    others
}

async fn seed_credentials(db: &DatabaseConnection, user_id: i32) -> String {
    let raw_key = format!("opn_{}", uuid::Uuid::new_v4().simple());
    api_keys::ActiveModel {
        user_id: Set(user_id),
        name: Set("regression key".to_string()),
        key_hash: Set(hash_api_key(&raw_key)),
        key_prefix: Set(raw_key.chars().take(12).collect()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert API key");

    passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(format!("regression-cred-{}", uuid::Uuid::new_v4())),
        cred_public_key: Set("test-public-key".to_string()),
        counter: Set(0),
        name: Set(Some("regression passkey".to_string())),
        created_at: Set(chrono::Utc::now().naive_utc()),
        last_used: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert passkey");

    raw_key
}

async fn assert_credentials_gone(db: &DatabaseConnection, user_id: i32) {
    let api_key_count = api_keys::Entity::find()
        .filter(api_keys::Column::UserId.eq(user_id))
        .count(db)
        .await
        .expect("count API keys");
    let passkey_count = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .count(db)
        .await
        .expect("count passkeys");
    assert_eq!(api_key_count, 0, "soft delete must revoke API keys");
    assert_eq!(passkey_count, 0, "soft delete must revoke passkeys");
}

#[tokio::test]
async fn credential_creation_requires_a_verified_jwt() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_API_KEYS", "true") };
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_PASSKEYS", "true") };

    let (server, db) = spawn_real_app().await;
    let email = unique_email();
    let (jwt, user_id) = register(&server, &email).await;

    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(&jwt)
        .json(&json!({ "name": "before verification" }))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "unverified user created API key: {}",
        res.text()
    );

    let res = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(&jwt)
        .json(&json!({ "username": &email }))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "unverified user started passkey enrollment: {}",
        res.text()
    );

    mark_email_verified(&db, user_id).await;
    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(&jwt)
        .json(&json!({ "name": "verified key" }))
        .await;
    assert_eq!(res.status_code(), 201, "create API key: {}", res.text());
    let created_key = res.json::<Value>();
    let api_key_id = created_key["id"].as_i64().expect("API key id");
    let api_key = created_key["key"]
        .as_str()
        .expect("raw API key")
        .to_string();

    // Sanity: this is a valid API key for ordinary API work.
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&api_key)
            .await
            .status_code(),
        200
    );

    assert_eq!(
        server
            .get("/auth/api-keys")
            .authorization_bearer(&api_key)
            .await
            .status_code(),
        401,
        "API key listed API keys"
    );

    assert_eq!(
        server
            .delete(&format!("/auth/api-keys/{api_key_id}"))
            .authorization_bearer(&api_key)
            .await
            .status_code(),
        401,
        "API key revoked API keys"
    );

    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(&api_key)
        .json(&json!({ "name": "key from key" }))
        .await;
    assert_eq!(res.status_code(), 401, "API key created another key");

    let res = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(&api_key)
        .json(&json!({ "username": &email }))
        .await;
    assert_eq!(res.status_code(), 401, "API key enrolled a passkey");

    let res = server
        .post("/auth/change-password")
        .authorization_bearer(&api_key)
        .json(&json!({
            "current_password": "password123",
            "new_password": "password456"
        }))
        .await;
    assert_eq!(res.status_code(), 401, "API key reached JWT-minting path");
}

#[tokio::test]
async fn admin_delete_and_restore_revoke_sessions_and_credentials() {
    let lock_db = lock_admin_count().await;
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &unique_email()).await;
    promote_directly(&db, admin_id).await;

    let email = unique_email();
    let (old_jwt, user_id) = register(&server, &email).await;
    mark_email_verified(&db, user_id).await;
    let old_api_key = seed_credentials(&db, user_id).await;
    let original_version = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    let res = server
        .delete(&format!("/admin/users/{user_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "admin delete: {}", res.text());

    let deleted = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(deleted.deleted_at.is_some());
    assert_eq!(deleted.token_version, original_version + 1);
    assert_credentials_gone(&db, user_id).await;
    assert_eq!(
        server
            .get("/auth/me")
            .authorization_bearer(&old_jwt)
            .await
            .status_code(),
        401
    );
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&old_api_key)
            .await
            .status_code(),
        401
    );

    let res = server
        .post(&format!("/admin/users/{user_id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "restore: {}", res.text());

    let restored = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(restored.deleted_at.is_none());
    assert_eq!(restored.token_version, original_version + 2);
    assert_credentials_gone(&db, user_id).await;
    assert_eq!(
        server
            .get("/auth/me")
            .authorization_bearer(&old_jwt)
            .await
            .status_code(),
        401,
        "restore must not revive old JWT"
    );
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&old_api_key)
            .await
            .status_code(),
        401,
        "restore must not revive old API key"
    );
    unlock_admin_count(&lock_db).await;
}

#[tokio::test]
async fn self_delete_revokes_sessions_and_credentials() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };

    let (server, db) = spawn_real_app().await;
    let (jwt, user_id) = register(&server, &unique_email()).await;
    // First-user bootstrap may grant admin on an empty table; last-admin
    // protection would then refuse this delete. This test is about credential
    // revocation, not admin tenure.
    let registered = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    if registered.is_admin {
        let mut active: users::ActiveModel = registered.into();
        active.is_admin = Set(false);
        active.update(&db).await.expect("demote bootstrap admin");
    }
    mark_email_verified(&db, user_id).await;
    let api_key = seed_credentials(&db, user_id).await;
    let original_version = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&jwt)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "self delete: {}", res.text());

    let deleted = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(deleted.deleted_at.is_some());
    assert_eq!(deleted.token_version, original_version + 1);
    assert_credentials_gone(&db, user_id).await;
    assert_eq!(
        server
            .get("/auth/me")
            .authorization_bearer(&jwt)
            .await
            .status_code(),
        401
    );
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&api_key)
            .await
            .status_code(),
        401
    );
}

#[tokio::test]
async fn admin_promotion_revokes_the_pre_promotion_jwt() {
    let lock_db = lock_admin_count().await;
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &unique_email()).await;
    promote_directly(&db, admin_id).await;

    let email = unique_email();
    let (old_token, user_id) = register(&server, &email).await;
    let before = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    let res = server
        .post(&format!("/admin/users/{user_id}/make-admin"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "promote: {}", res.text());

    let after = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;
    assert_eq!(after, before + 1);
    assert_eq!(
        server
            .get("/admin/stats")
            .authorization_bearer(&old_token)
            .await
            .status_code(),
        401,
        "pre-promotion JWT must not gain admin rights"
    );

    let login = server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(login.status_code(), 200, "login: {}", login.text());
    let fresh_token = login.json::<Value>()["token"]
        .as_str()
        .expect("fresh token")
        .to_string();
    assert_eq!(
        server
            .get("/admin/stats")
            .authorization_bearer(&fresh_token)
            .await
            .status_code(),
        200
    );
    unlock_admin_count(&lock_db).await;
}

#[tokio::test]
async fn password_change_consumes_outstanding_reset_token() {
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id) = register(&server, &unique_email()).await;

    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.password_reset_token = Set(Some(format!("reset-{}", uuid::Uuid::new_v4())));
    active.password_reset_expires = Set(Some(
        (chrono::Utc::now() + chrono::Duration::hours(1)).naive_utc(),
    ));
    active.update(&db).await.expect("seed reset token");

    let res = server
        .post("/auth/change-password")
        .authorization_bearer(&jwt)
        .json(&json!({
            "current_password": "password123",
            "new_password": "password456"
        }))
        .await;
    assert_eq!(res.status_code(), 200, "change password: {}", res.text());

    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(user.password_reset_token.is_none());
    assert!(user.password_reset_expires.is_none());
}

#[tokio::test]
async fn a_non_last_admin_can_self_delete() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let lock_db = lock_admin_count().await;
    let (server, db) = spawn_real_app().await;

    let (jwt_a, user_a) = register(&server, &unique_email()).await;
    promote_directly(&db, user_a).await;
    let (_jwt_b, user_b) = register(&server, &unique_email()).await;
    promote_directly(&db, user_b).await;

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&jwt_a)
        .json(&json!({ "password": "password123" }))
        .await;
    let status = res.status_code();
    let body = res.text();
    unlock_admin_count(&lock_db).await;
    assert_eq!(
        status, 200,
        "non-last admin must still be able to leave: {body}"
    );

    let deleted = users::Entity::find_by_id(user_a)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(deleted.deleted_at.is_some());
    let remaining = users::Entity::find_by_id(user_b)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(remaining.deleted_at.is_none());
    assert!(remaining.is_admin);
}

#[tokio::test]
async fn last_remaining_admin_cannot_self_delete() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let lock_db = lock_admin_count().await;
    let (server, db) = spawn_real_app().await;

    let (jwt, user_id) = register(&server, &unique_email()).await;
    promote_directly(&db, user_id).await;

    // The production guard counts every live admin in the shared database.
    // Temporarily demote the others so this test is actually the last admin
    // and can assert 409 unconditionally, then restore them. Parallel tests
    // in other binaries may promote during the gap; re-demote until we are
    // the sole live admin, then delete immediately.
    let mut others = Vec::new();
    for _ in 0..8 {
        others = demote_other_live_admins(&db, user_id).await;
        let live = users::Entity::find()
            .filter(users::Column::IsAdmin.eq(true))
            .filter(users::Column::DeletedAt.is_null())
            .count(&db)
            .await
            .expect("count live admins");
        if live == 1 {
            break;
        }
    }

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&jwt)
        .json(&json!({ "password": "password123" }))
        .await;
    let status = res.status_code();
    let body = res.text();
    let still = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    for id in others {
        promote_directly(&db, id).await;
    }
    unlock_admin_count(&lock_db).await;

    assert_eq!(status, 409, "last admin must be refused: {body}");
    assert!(
        body.contains("last remaining admin"),
        "last admin body: {body}"
    );
    assert!(still.deleted_at.is_none());
    assert!(still.is_admin);
}

#[tokio::test]
async fn forgot_password_does_not_enumerate_accounts() {
    let (server, db) = spawn_real_app().await;

    let known_email = unique_email();
    register(&server, &known_email).await;
    let unknown_email = unique_email();

    let post = |email: &str| {
        server
            .post("/auth/forgot-password")
            .json(&json!({ "email": email }))
    };

    let known = post(&known_email).await;
    let unknown = post(&unknown_email).await;

    assert_eq!(known.status_code(), 200, "known: {}", known.text());
    assert!(
        known.text().contains("If account exists"),
        "generic body: {}",
        known.text()
    );
    assert_eq!(
        (unknown.status_code(), unknown.text()),
        (known.status_code(), known.text()),
        "unknown vs known must be indistinguishable"
    );

    let stored = users::Entity::find()
        .filter(users::Column::Email.eq(&known_email))
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .password_reset_token
        .expect("known account must receive a reset token");
    assert_eq!(stored.len(), 64);
    assert!(
        stored.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "reset token must be sha256 hex, got {stored}"
    );
}

#[tokio::test]
async fn verification_and_reset_tokens_are_stored_hashed() {
    let (server, db) = spawn_real_app().await;
    let email = unique_email();
    let (_, user_id) = register(&server, &email).await;

    let stored = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .verification_token
        .expect("register must store a verification token");
    assert_eq!(stored.len(), 64);
    assert!(
        stored.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "verification_token must be sha256 hex, got {stored}"
    );

    let raw_verify = format!("verify-{}", uuid::Uuid::new_v4());
    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.verification_token = Set(Some(hash_secret_token(&raw_verify)));
    active.verification_token_expires = Set(Some(
        (chrono::Utc::now() + chrono::Duration::hours(24)).naive_utc(),
    ));
    active
        .update(&db)
        .await
        .expect("seed hashed verification token");

    let stored_hash = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .verification_token
        .unwrap();
    let steal = server
        .post("/auth/verify-email")
        .json(&json!({ "token": stored_hash }))
        .await;
    assert_eq!(
        steal.status_code(),
        400,
        "presenting the stored digest must not verify: {}",
        steal.text()
    );

    let ok = server
        .post("/auth/verify-email")
        .json(&json!({ "token": raw_verify }))
        .await;
    assert_eq!(
        ok.status_code(),
        200,
        "raw token must verify: {}",
        ok.text()
    );
    let verified = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(verified.email_verified);
    assert!(verified.verification_token.is_none());

    let raw_reset = format!("reset-{}", uuid::Uuid::new_v4());
    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.password_reset_token = Set(Some(hash_secret_token(&raw_reset)));
    active.password_reset_expires = Set(Some(
        (chrono::Utc::now() + chrono::Duration::hours(1)).naive_utc(),
    ));
    active.update(&db).await.expect("seed hashed reset token");

    let stored_reset = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .password_reset_token
        .unwrap();
    let steal = server
        .post("/auth/reset-password")
        .json(&json!({ "token": stored_reset, "password": "newpassword1" }))
        .await;
    assert_eq!(
        steal.status_code(),
        400,
        "presenting the stored digest must not reset: {}",
        steal.text()
    );

    let ok = server
        .post("/auth/reset-password")
        .json(&json!({ "token": raw_reset, "password": "newpassword1" }))
        .await;
    assert_eq!(ok.status_code(), 200, "raw token must reset: {}", ok.text());
    let reset = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(reset.password_reset_token.is_none());
}

#[tokio::test]
async fn second_registered_user_is_not_admin() {
    let (server, _db) = spawn_real_app().await;

    let first = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(first.status_code(), 201, "first register: {}", first.text());

    let second = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(
        second.status_code(),
        201,
        "second register: {}",
        second.text()
    );
    let body: Value = second.json();
    assert_eq!(
        body["is_admin"].as_bool(),
        Some(false),
        "a later registrant must not be granted admin: {body}"
    );
}

#[tokio::test]
async fn resend_verification_does_not_enumerate_accounts() {
    let (server, db) = spawn_real_app().await;

    let unverified_email = unique_email();
    register(&server, &unverified_email).await;

    let verified_email = unique_email();
    let (_, verified_id) = register(&server, &verified_email).await;
    mark_email_verified(&db, verified_id).await;

    let unknown_email = unique_email();

    let post = |email: &str| {
        server
            .post("/auth/resend-verification")
            .json(&json!({ "email": email }))
    };

    let unverified = post(&unverified_email).await;
    let verified = post(&verified_email).await;
    let unknown = post(&unknown_email).await;

    assert_eq!(unverified.status_code(), 200, "{}", unverified.text());
    assert!(
        unverified.text().contains("If account exists"),
        "generic body: {}",
        unverified.text()
    );
    assert_eq!(
        (verified.status_code(), verified.text()),
        (unverified.status_code(), unverified.text()),
        "verified vs unverified must be indistinguishable"
    );
    assert_eq!(
        (unknown.status_code(), unknown.text()),
        (unverified.status_code(), unverified.text()),
        "unknown vs unverified must be indistinguishable"
    );
}
