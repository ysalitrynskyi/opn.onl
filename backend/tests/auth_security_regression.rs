//! Regression coverage for credential-boundary and session-revocation fixes.
//! Real router + real Postgres via `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{api_keys, passkeys, users};
use opn_onl_backend::handlers::links::hash_api_key;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
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
    std::env::set_var("ENABLE_API_KEYS", "true");
    std::env::set_var("ENABLE_PASSKEYS", "true");

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
}

#[tokio::test]
async fn self_delete_revokes_sessions_and_credentials() {
    std::env::set_var("ENABLE_ACCOUNT_DELETION", "true");

    let (server, db) = spawn_real_app().await;
    let (jwt, user_id) = register(&server, &unique_email()).await;
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
