//! `POST /links` accepts anonymous callers, but only when no credential is
//! sent. A credential that is sent and rejected (expired, revoked, unknown) is
//! a 401: answering 201 with an ownerless link would hand a signed-in user a
//! link they can never see or manage.

use crate::common;

use axum_test::TestServer;
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{links, users};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

/// `ENABLE_API_KEYS` is process-wide, and these tests flip it. Every test that
/// creates or presents an API key holds this lock for its whole body.
static API_KEYS_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Register and verify an account. Returns (JWT, user id, email).
async fn register_verified(server: &TestServer, db: &DatabaseConnection) -> (String, i32, String) {
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id, email)
}

/// Revoke every JWT issued so far, the way a password change does.
async fn bump_token_version(db: &DatabaseConnection, user_id: i32) {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    let next = user.token_version + 1;
    let mut active: users::ActiveModel = user.into();
    active.token_version = Set(next);
    active.update(db).await.unwrap();
}

/// A JWT signed like `utils::jwt::create_jwt` but with an arbitrary `exp`.
fn mint_jwt(user_id: i32, email: &str, token_version: i32, exp: i64) -> String {
    use jsonwebtoken::{EncodingKey, Header, encode};
    use opn_onl_backend::utils::jwt::Claims;
    let secret = std::env::var("JWT_SECRET").expect("spawn_real_app sets JWT_SECRET");
    let claims = Claims {
        sub: email.to_string(),
        exp: exp as usize,
        user_id,
        token_version,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("encode jwt")
}

/// Create an API key for the account and return its secret and id.
async fn create_api_key(server: &TestServer, jwt: &str) -> (String, i64) {
    common::set_env("ENABLE_API_KEYS", "true");
    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(jwt)
        .json(&json!({ "name": "link create" }))
        .await;
    assert_eq!(res.status_code(), 201, "create API key: {}", res.text());
    let body: Value = res.json();
    (
        body["key"].as_str().unwrap().to_string(),
        body["id"].as_i64().unwrap(),
    )
}

fn unique_url(tag: &str) -> String {
    format!(
        "https://iana.org/create-auth-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    )
}

async fn link_by_code(db: &DatabaseConnection, code: &str) -> Option<links::Model> {
    links::Entity::find()
        .filter(links::Column::Code.eq(code))
        .one(db)
        .await
        .unwrap()
}

/// POST /links with a unique alias and whatever `Authorization` value the
/// caller wants (or none). Returns the response and the alias.
async fn create_with_authorization(
    server: &TestServer,
    authorization: Option<&str>,
) -> (axum_test::TestResponse, String) {
    let alias = format!("auth{}", unique_code());
    let req = server.post("/links").json(&json!({
        "original_url": unique_url("create"),
        "custom_alias": alias,
    }));
    let res = match authorization {
        Some(value) => req.add_header("authorization", value).await,
        None => req.await,
    };
    (res, alias)
}

/// A rejected credential is a 401 in the API's usual `{"error": ...}` shape and
/// leaves no row behind (the alias is not burned either).
async fn assert_refused(
    server: &TestServer,
    db: &DatabaseConnection,
    what: &str,
    authorization: &str,
) {
    let (res, alias) = create_with_authorization(server, Some(authorization)).await;
    let status = res.status_code();
    let text = res.text();
    let row = link_by_code(db, &alias).await;
    assert_eq!(
        status, 401,
        "POST /links with {what} must be 401, got {status} (row: {row:?}); body={text}"
    );
    let body: Value = serde_json::from_str(&text).expect("401 body is JSON");
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "401 for {what} has no `error` field: {text}"
    );
    assert!(
        row.is_none(),
        "POST /links with {what} must not create a link, found {row:?}"
    );
}

#[tokio::test]
async fn expired_jwt_is_refused_not_turned_into_an_anonymous_link() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (_jwt, user_id, email) = register_verified(&server, &db).await;
    let now = chrono::Utc::now().timestamp();

    // Control: the same minting code with a future expiry owns its link, so
    // the expiry is the only difference between the two requests.
    let fresh = mint_jwt(user_id, &email, 0, now + 3600);
    let (res, alias) = create_with_authorization(&server, Some(&format!("Bearer {fresh}"))).await;
    assert_eq!(res.status_code(), 201, "fresh minted jwt: {}", res.text());
    let row = link_by_code(&db, &alias).await.expect("row exists");
    assert_eq!(row.user_id, Some(user_id));

    // Two days ago: well past the validator's 60 s leeway.
    let expired = mint_jwt(user_id, &email, 0, now - 2 * 24 * 3600);
    assert_refused(&server, &db, "an expired JWT", &format!("Bearer {expired}")).await;
}

#[tokio::test]
async fn revoked_session_token_is_refused_not_turned_into_an_anonymous_link() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id, _email) = register_verified(&server, &db).await;

    let (res, alias) = create_with_authorization(&server, Some(&format!("Bearer {jwt}"))).await;
    assert_eq!(res.status_code(), 201, "before revoking: {}", res.text());
    assert_eq!(
        link_by_code(&db, &alias).await.unwrap().user_id,
        Some(user_id)
    );

    bump_token_version(&db, user_id).await;

    // Every other authenticated endpoint already refuses the revoked token.
    let list = server.get("/links").authorization_bearer(&jwt).await;
    assert_eq!(list.status_code(), 401);

    assert_refused(
        &server,
        &db,
        "a JWT with a stale token_version",
        &format!("Bearer {jwt}"),
    )
    .await;
}

#[tokio::test]
async fn token_of_a_disabled_account_is_refused() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id, _email) = register_verified(&server, &db).await;

    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.disabled_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.unwrap();

    assert_refused(
        &server,
        &db,
        "the JWT of a disabled account",
        &format!("Bearer {jwt}"),
    )
    .await;
}

#[tokio::test]
async fn unknown_api_key_is_refused() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    assert_refused(
        &server,
        &db,
        "an unknown `opn_` key",
        "Bearer opn_doesnotexist",
    )
    .await;
}

#[tokio::test]
async fn garbage_bearer_is_refused() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    assert_refused(&server, &db, "`Bearer garbage`", "Bearer garbage").await;
}

#[tokio::test]
async fn authorization_header_with_another_scheme_is_refused() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    assert_refused(&server, &db, "a Basic credential", "Basic dXNlcjpwYXNz").await;
    assert_refused(&server, &db, "an empty header value", "").await;
}

#[tokio::test]
async fn rejected_credential_is_refused_before_the_url_is_looked_at() {
    let _serial = crate::common::serial_guard().await;
    let (server, _db) = spawn_real_app().await;
    let res = server
        .post("/links")
        .add_header("authorization", "Bearer garbage")
        .json(&json!({ "original_url": "not a url" }))
        .await;
    assert_eq!(res.status_code(), 401, "body: {}", res.text());
}

/// A stale token sent with folder scope used to answer 403, which the SPA does
/// not treat as an expired session.
#[tokio::test]
async fn stale_token_with_folder_scope_is_401_not_403() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id, _email) = register_verified(&server, &db).await;
    bump_token_version(&db, user_id).await;

    for scope in [
        json!({ "folder_id": 1 }),
        json!({ "org_id": 1 }),
        json!({ "tag_ids": [1] }),
    ] {
        let mut payload = json!({ "original_url": unique_url("scope") });
        payload
            .as_object_mut()
            .unwrap()
            .extend(scope.as_object().unwrap().clone());
        let res = server
            .post("/links")
            .authorization_bearer(&jwt)
            .json(&payload)
            .await;
        assert_eq!(
            res.status_code(),
            401,
            "stale token + {scope}: {}",
            res.text()
        );
    }
}

#[tokio::test]
async fn revoked_api_key_is_refused_while_the_live_key_owns_its_links() {
    let _serial = crate::common::serial_guard().await;
    let _env = API_KEYS_ENV.lock().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id, _email) = register_verified(&server, &db).await;
    let (key, key_id) = create_api_key(&server, &jwt).await;

    // Control: a live key creates an owned link.
    let (res, alias) = create_with_authorization(&server, Some(&format!("Bearer {key}"))).await;
    assert_eq!(res.status_code(), 201, "live key: {}", res.text());
    let row = link_by_code(&db, &alias).await.expect("row exists");
    assert_eq!(
        row.user_id,
        Some(user_id),
        "API key links belong to the key's owner"
    );

    let res = server
        .delete(&format!("/auth/api-keys/{key_id}"))
        .authorization_bearer(&jwt)
        .await;
    assert_eq!(res.status_code(), 200, "revoke: {}", res.text());

    assert_refused(&server, &db, "a revoked API key", &format!("Bearer {key}")).await;
}

#[tokio::test]
async fn api_key_is_refused_when_api_keys_are_switched_off() {
    let _serial = crate::common::serial_guard().await;
    let _env = API_KEYS_ENV.lock().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, _user_id, _email) = register_verified(&server, &db).await;
    let (key, _key_id) = create_api_key(&server, &jwt).await;

    common::set_env("ENABLE_API_KEYS", "false");
    assert_refused(
        &server,
        &db,
        "an API key while ENABLE_API_KEYS=false",
        &format!("Bearer {key}"),
    )
    .await;
    common::set_env("ENABLE_API_KEYS", "true");
}

/// Control: with no `Authorization` header the link is anonymous, as before.
#[tokio::test]
async fn no_credential_still_creates_an_anonymous_link() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (res, alias) = create_with_authorization(&server, None).await;
    assert_eq!(res.status_code(), 201, "anonymous create: {}", res.text());
    let row = link_by_code(&db, &alias).await.expect("row exists");
    assert_eq!(row.user_id, None, "an anonymous link has no owner");
}

/// Control: anonymous callers still cannot use folders, tags or orgs. That
/// stays a 403: nothing was rejected, there is just nobody to scope the link to.
#[tokio::test]
async fn anonymous_caller_with_folder_scope_is_still_forbidden() {
    let _serial = crate::common::serial_guard().await;
    let (server, _db) = spawn_real_app().await;
    let res = server
        .post("/links")
        .json(&json!({ "original_url": unique_url("anon-folder"), "folder_id": 1 }))
        .await;
    assert_eq!(res.status_code(), 403, "body: {}", res.text());
}

/// Control: a valid JWT creates an owned link.
#[tokio::test]
async fn valid_jwt_creates_an_owned_link() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id, _email) = register_verified(&server, &db).await;
    let (res, alias) = create_with_authorization(&server, Some(&format!("Bearer {jwt}"))).await;
    assert_eq!(res.status_code(), 201, "jwt create: {}", res.text());
    let row = link_by_code(&db, &alias).await.expect("row exists");
    assert_eq!(row.user_id, Some(user_id));
}

/// Bulk create never allowed anonymous callers and already refuses a rejected
/// credential. Pinned so the two create endpoints keep agreeing.
#[tokio::test]
async fn bulk_create_refuses_missing_and_rejected_credentials() {
    let _serial = crate::common::serial_guard().await;
    let (server, db) = spawn_real_app().await;
    let (jwt, user_id, _email) = register_verified(&server, &db).await;
    bump_token_version(&db, user_id).await;

    let body = json!({ "urls": [unique_url("bulk")] });
    let anonymous = server.post("/links/bulk").json(&body).await;
    assert_eq!(
        anonymous.status_code(),
        401,
        "no header: {}",
        anonymous.text()
    );
    for (what, authorization) in [
        ("stale token", format!("Bearer {jwt}")),
        ("unknown key", "Bearer opn_doesnotexist".to_string()),
        ("garbage", "Bearer garbage".to_string()),
    ] {
        let res = server
            .post("/links/bulk")
            .add_header("authorization", authorization.as_str())
            .json(&body)
            .await;
        assert_eq!(res.status_code(), 401, "bulk with {what}: {}", res.text());
    }
}
