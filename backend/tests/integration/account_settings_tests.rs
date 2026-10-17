//! Account and settings correctness, against the real router and Postgres.
//!
//! - The profile form could not clear a field, and a "cleared" field was NULL
//!   for some columns and an empty string for others.
//! - The bio limit counted bytes while the form counts characters, and the
//!   other profile fields had no server limit at all.
//! - Deleting a passkey signed the caller out of their own session.
//! - A password reset left the API keys and passkeys an intruder may have
//!   added, and the notice mail said sessions were signed out and nothing more.

use crate::common;
use crate::common::smtp_stub;

use std::sync::Arc;

use axum_test::TestServer;
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{api_keys, passkeys, users};
use opn_onl_backend::handlers::auth::hash_secret_token;
use opn_onl_backend::utils::email::EmailService;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use smtp_stub::smtp_stub;

const PASSWORD: &str = "password123";

// ---------------------------------------------------------------- fixtures

struct Account {
    token: String,
    id: i32,
    email: String,
}

async fn register_verified_as(
    server: &TestServer,
    db: &DatabaseConnection,
    email: String,
) -> Account {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let id = body["user_id"].as_i64().expect("user id") as i32;
    mark_email_verified(db, id).await;
    Account {
        token: body["token"].as_str().expect("token").to_string(),
        id,
        email,
    }
}

async fn register_verified(server: &TestServer, db: &DatabaseConnection) -> Account {
    register_verified_as(server, db, unique_email()).await
}

async fn put_profile(server: &TestServer, token: &str, body: Value) -> axum_test::TestResponse {
    server
        .put("/auth/profile")
        .authorization_bearer(token)
        .json(&body)
        .await
}

async fn stored_user(db: &DatabaseConnection, id: i32) -> users::Model {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("db")
        .expect("user row")
}

async fn create_api_key(server: &TestServer, token: &str) -> String {
    common::set_env("ENABLE_API_KEYS", "true");
    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(token)
        .json(&json!({ "name": "settings test key" }))
        .await;
    assert_eq!(res.status_code(), 201, "create API key: {}", res.text());
    res.json::<Value>()["key"]
        .as_str()
        .expect("raw API key")
        .to_string()
}

async fn insert_passkey(
    db: &DatabaseConnection,
    user_id: i32,
    last_used: Option<chrono::NaiveDateTime>,
) -> i32 {
    passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(format!("settings-test-cred-{}", uuid::Uuid::new_v4())),
        cred_public_key: Set("test-public-key".to_string()),
        counter: Set(0),
        name: Set(Some("laptop".to_string())),
        created_at: Set(chrono::Utc::now().naive_utc()),
        last_used: Set(last_used),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert passkey")
    .id
}

async fn passkey_count(db: &DatabaseConnection, user_id: i32) -> u64 {
    passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .count(db)
        .await
        .expect("count passkeys")
}

async fn api_key_count(db: &DatabaseConnection, user_id: i32) -> u64 {
    api_keys::Entity::find()
        .filter(api_keys::Column::UserId.eq(user_id))
        .count(db)
        .await
        .expect("count api keys")
}

async fn seed_reset_token(db: &DatabaseConnection, user_id: i32, raw: &str) {
    let mut active: users::ActiveModel = stored_user(db, user_id).await.into();
    active.password_reset_token = Set(Some(hash_secret_token(raw)));
    active.password_reset_expires = Set(Some(
        (chrono::Utc::now() + chrono::Duration::hours(1)).naive_utc(),
    ));
    active.update(db).await.expect("seed reset token");
}

async fn reset_password(server: &TestServer, db: &DatabaseConnection, user_id: i32) {
    let raw = format!("reset-{}", uuid::Uuid::new_v4());
    seed_reset_token(db, user_id, &raw).await;
    let res = server
        .post("/auth/reset-password")
        .json(&json!({ "token": raw, "password": "newpassword123" }))
        .await;
    assert_eq!(res.status_code(), 200, "reset: {}", res.text());
}

async fn change_password(server: &TestServer, token: &str) -> Value {
    let res = server
        .post("/auth/change-password")
        .authorization_bearer(token)
        .json(&json!({ "current_password": PASSWORD, "new_password": "newpassword123" }))
        .await;
    assert_eq!(res.status_code(), 200, "change password: {}", res.text());
    res.json()
}

/// The real router with a configured `EmailService` pointed at the SMTP stub.
async fn mail_app() -> (TestServer, DatabaseConnection) {
    smtp_stub();
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = common::setup_test_db().await;
    let service = Arc::new(EmailService::new());
    assert!(service.is_configured(), "the SMTP stub is not configured");
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.email_service = Some(service);
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}

/// The text of the password-changed notice sent to `address` (registration
/// mails the same address a verification link, so pick the notice by subject).
async fn password_notice(stub: &smtp_stub::SmtpStub, address: &str) -> String {
    for _ in 0..100 {
        let notices: Vec<_> = stub
            .to(address)
            .into_iter()
            .filter(|mail| mail.subject() == "Your password was changed - opn.onl")
            .collect();
        if let [notice] = notices.as_slice() {
            return notice.decoded();
        }
        assert!(notices.len() < 2, "more than one notice for {address}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("no password-changed notice reached {address}");
}

// ----------------------------------------------------------------- profile

/// Clearing a profile field in Settings sends an empty string. Every field
/// must then be NULL (not "" for some and NULL for others), and the public
/// bio page must stop showing it.
#[tokio::test]
async fn cleared_profile_fields_become_null_and_leave_the_public_bio() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;
    let username = format!("bio{}", unique_code().to_lowercase());

    let res = server
        .put("/auth/bio")
        .authorization_bearer(&user.token)
        .json(&json!({ "bio_username": username, "bio_enabled": true }))
        .await;
    assert_eq!(res.status_code(), 200, "enable bio: {}", res.text());

    let res = put_profile(
        &server,
        &user.token,
        json!({
            "display_name": "Ada",
            "bio": "Analytical engines",
            "website": "https://iana.org/ada",
            "avatar_url": "https://iana.org/ada.png",
            "location": "London",
        }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "fill profile: {}", res.text());

    // Control: the public page shows what was set.
    let page: Value = server.get(&format!("/api/bio/{username}")).await.json();
    assert_eq!(page["display_name"], "Ada");
    assert_eq!(page["bio"], "Analytical engines");
    assert_eq!(page["location"], "London");

    // Clear all five: three with an empty string, two with only whitespace.
    let res = put_profile(
        &server,
        &user.token,
        json!({
            "display_name": "",
            "bio": "",
            "website": "",
            "avatar_url": "   ",
            "location": " \t ",
        }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "clear profile: {}", res.text());
    let body: Value = res.json();
    for field in ["display_name", "bio", "website", "avatar_url", "location"] {
        assert!(body[field].is_null(), "{field} in the response: {body}");
    }

    let row = stored_user(&db, user.id).await;
    assert_eq!(row.display_name, None);
    assert_eq!(row.bio, None);
    assert_eq!(row.website, None);
    assert_eq!(row.avatar_url, None);
    assert_eq!(row.location, None);

    let page: Value = server.get(&format!("/api/bio/{username}")).await.json();
    for field in ["display_name", "bio", "website", "avatar_url", "location"] {
        assert!(page[field].is_null(), "{field} on the public page: {page}");
    }

    // /auth/me agrees.
    let me: Value = server
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await
        .json();
    assert!(me["display_name"].is_null() && me["bio"].is_null(), "{me}");
}

/// Control for the above: a key that is absent leaves the stored value alone,
/// so a client that sends only what it changed does not wipe the rest.
#[tokio::test]
async fn profile_fields_that_are_not_sent_are_left_alone() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;

    let res = put_profile(
        &server,
        &user.token,
        json!({ "display_name": "Ada", "bio": "Hello", "location": "London" }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());

    let res = put_profile(&server, &user.token, json!({ "location": "Paris" })).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());

    let row = stored_user(&db, user.id).await;
    assert_eq!(row.display_name.as_deref(), Some("Ada"));
    assert_eq!(row.bio.as_deref(), Some("Hello"));
    assert_eq!(row.location.as_deref(), Some("Paris"));
}

/// The form counts characters. The server counted bytes, so a Cyrillic bio
/// of 251 to 500 characters (two bytes each) was refused.
#[tokio::test]
async fn the_bio_limit_counts_characters_not_bytes() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;

    for (letter, count) in [("ж", 300), ("ж", 500), ("語", 500), ("🙂", 500)] {
        let bio = letter.repeat(count);
        let res = put_profile(&server, &user.token, json!({ "bio": bio })).await;
        assert_eq!(
            res.status_code(),
            200,
            "{count} x {letter:?} ({} bytes) must fit: {}",
            bio.len(),
            res.text()
        );
        assert_eq!(stored_user(&db, user.id).await.bio.as_deref(), Some(&*bio));
    }

    let res = put_profile(&server, &user.token, json!({ "bio": "ж".repeat(501) })).await;
    assert_eq!(res.status_code(), 400, "501 characters: {}", res.text());
    assert_eq!(
        res.json::<Value>()["error"],
        "bio: must be at most 500 characters"
    );
    // The refused request changed nothing.
    assert_eq!(
        stored_user(&db, user.id).await.bio.as_deref(),
        Some("🙂".repeat(500).as_str())
    );
}

/// Display name and location stop at 100 characters, website and avatar URL
/// at 2048, each with a `field: detail` message. A request with one bad field
/// saves none of the others.
#[tokio::test]
async fn profile_fields_have_server_side_limits() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;
    let url_of_len = |len: usize| {
        let prefix = "https://iana.org/";
        format!("{prefix}{}", "a".repeat(len - prefix.len()))
    };

    for field in ["display_name", "location"] {
        let ok = put_profile(&server, &user.token, json!({ field: "ж".repeat(100) })).await;
        assert_eq!(ok.status_code(), 200, "{field} at 100: {}", ok.text());

        let res = put_profile(&server, &user.token, json!({ field: "ж".repeat(101) })).await;
        assert_eq!(res.status_code(), 400, "{field} at 101: {}", res.text());
        assert_eq!(
            res.json::<Value>()["error"],
            format!("{field}: must be at most 100 characters")
        );
    }

    for field in ["website", "avatar_url"] {
        let ok = put_profile(&server, &user.token, json!({ field: url_of_len(2048) })).await;
        assert_eq!(ok.status_code(), 200, "{field} at 2048: {}", ok.text());

        let res = put_profile(&server, &user.token, json!({ field: url_of_len(2049) })).await;
        assert_eq!(res.status_code(), 400, "{field} at 2049: {}", res.text());
        assert_eq!(
            res.json::<Value>()["error"],
            format!("{field}: must be at most 2048 characters")
        );
    }

    let before = stored_user(&db, user.id).await;
    let res = put_profile(
        &server,
        &user.token,
        json!({ "display_name": "Changed", "bio": "x".repeat(501) }),
    )
    .await;
    assert_eq!(res.status_code(), 400, "{}", res.text());
    assert_eq!(
        stored_user(&db, user.id).await.display_name,
        before.display_name,
        "a refused request must not save the fields that were fine"
    );
}

// ---------------------------------------------------------------- passkeys

/// Deleting a passkey revokes older tokens (the factor changed), so the
/// response has to carry a new one or Settings logs the caller out.
#[tokio::test]
async fn deleting_a_passkey_hands_back_a_token_that_keeps_the_session() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;
    let doomed = insert_passkey(&db, user.id, None).await;
    let kept = insert_passkey(&db, user.id, None).await;

    let res = server
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "control: session works: {}",
        res.text()
    );

    let res = server
        .post("/auth/passkey/delete")
        .authorization_bearer(&user.token)
        .json(&json!({ "passkey_id": doomed }))
        .await;
    assert_eq!(res.status_code(), 200, "delete passkey: {}", res.text());
    let fresh = res.json::<Value>()["token"]
        .as_str()
        .expect("the delete response carries a token")
        .to_string();

    // What Settings does next: reload with the token it now holds.
    let res = server.get("/auth/me").authorization_bearer(&fresh).await;
    assert_eq!(res.status_code(), 200, "new token: {}", res.text());
    let res = server
        .get("/auth/passkeys")
        .authorization_bearer(&fresh)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let listed: Vec<i64> = res.json::<Value>()["passkeys"]
        .as_array()
        .expect("passkey list")
        .iter()
        .map(|p| p["id"].as_i64().expect("id"))
        .collect();
    assert_eq!(listed, vec![i64::from(kept)]);

    // The delete still signs out every session issued before it.
    let res = server
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(res.status_code(), 401, "the old token is revoked");
}

/// `last_used` went out as `2026-10-01 09:30:00.123`, a zone-less string that
/// browsers read as local time. Every timestamp a passkey response carries is
/// RFC 3339 in UTC.
#[tokio::test]
async fn passkey_timestamps_are_rfc3339_utc() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;
    let used_at = chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
        .unwrap()
        .and_hms_opt(9, 30, 5)
        .unwrap();
    let used = insert_passkey(&db, user.id, Some(used_at)).await;
    let unused = insert_passkey(&db, user.id, None).await;

    let res = server
        .get("/auth/passkeys")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    let rows = body["passkeys"].as_array().expect("passkey list");

    let used_row = rows
        .iter()
        .find(|p| p["id"].as_i64() == Some(i64::from(used)))
        .expect("used passkey");
    assert_eq!(used_row["last_used"], "2026-10-01T09:30:05Z");

    let unused_row = rows
        .iter()
        .find(|p| p["id"].as_i64() == Some(i64::from(unused)))
        .expect("unused passkey");
    assert!(unused_row["last_used"].is_null(), "{unused_row}");

    for row in rows {
        let created = row["created_at"].as_str().expect("created_at");
        assert!(created.ends_with('Z'), "created_at {created}");
        chrono::DateTime::parse_from_rfc3339(created).expect("created_at parses as RFC 3339");
    }
}

// --------------------------------------------- password reset versus change

/// A reset is account recovery: whatever else can sign in as the account (an
/// API key or passkey an intruder added) goes with the old password.
#[tokio::test]
async fn password_reset_revokes_api_keys_and_passkeys() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;
    let key = create_api_key(&server, &user.token).await;
    insert_passkey(&db, user.id, None).await;
    insert_passkey(&db, user.id, None).await;

    // Another account's key and passkey are not touched.
    let bystander = register_verified(&server, &db).await;
    let bystander_key = create_api_key(&server, &bystander.token).await;
    insert_passkey(&db, bystander.id, None).await;

    let res = server.get("/links").authorization_bearer(&key).await;
    assert_eq!(
        res.status_code(),
        200,
        "control: the key works: {}",
        res.text()
    );

    reset_password(&server, &db, user.id).await;

    let res = server.get("/links").authorization_bearer(&key).await;
    assert_eq!(res.status_code(), 401, "the old API key must be revoked");
    assert_eq!(api_key_count(&db, user.id).await, 0);
    assert_eq!(passkey_count(&db, user.id).await, 0);
    let res = server
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(res.status_code(), 401, "the old token is revoked too");

    let res = server
        .get("/links")
        .authorization_bearer(&bystander_key)
        .await;
    assert_eq!(res.status_code(), 200, "another account's key survives");
    assert_eq!(passkey_count(&db, bystander.id).await, 1);

    // The recovered account can sign in with the new password.
    let res = server
        .post("/auth/login")
        .json(&json!({ "email": user.email, "password": "newpassword123" }))
        .await;
    assert_eq!(res.status_code(), 200, "login after reset: {}", res.text());
}

/// A change is made by someone signed in who knows the old password: other
/// sessions are signed out, but API keys and passkeys stay, and the caller
/// keeps a working session.
#[tokio::test]
async fn password_change_signs_out_other_sessions_but_keeps_keys_and_passkeys() {
    let (server, db) = spawn_real_app().await;
    let user = register_verified(&server, &db).await;
    let key = create_api_key(&server, &user.token).await;
    insert_passkey(&db, user.id, None).await;

    let body = change_password(&server, &user.token).await;
    let fresh = body["token"].as_str().expect("fresh token");

    let res = server.get("/auth/me").authorization_bearer(fresh).await;
    assert_eq!(res.status_code(), 200, "the caller stays signed in");
    let res = server
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(res.status_code(), 401, "other sessions are signed out");

    let res = server.get("/links").authorization_bearer(&key).await;
    assert_eq!(
        res.status_code(),
        200,
        "the API key is kept: {}",
        res.text()
    );
    assert_eq!(api_key_count(&db, user.id).await, 1);
    assert_eq!(passkey_count(&db, user.id).await, 1);
}

/// The notice says what was revoked, and it differs by case: a reset removes
/// sessions, API keys and passkeys; a change signs out the other sessions and
/// keeps the keys and passkeys.
#[tokio::test]
async fn the_password_notice_states_what_each_case_revoked() {
    let (server, db) = mail_app().await;
    let stub = smtp_stub();

    let reset_user = register_verified(&server, &db).await;
    reset_password(&server, &db, reset_user.id).await;
    let text = password_notice(stub, &reset_user.email).await;
    assert!(text.contains("was reset on"), "{text}");
    assert!(
        text.contains(
            "Every signed-in session was signed out, and all API keys and passkeys on the \
             account were removed."
        ),
        "{text}"
    );
    assert!(!text.contains("were not changed"), "{text}");

    let change_user = register_verified(&server, &db).await;
    change_password(&server, &change_user.token).await;
    let text = password_notice(stub, &change_user.email).await;
    assert!(text.contains("was changed on"), "{text}");
    assert!(
        text.contains(
            "Your other signed-in sessions were signed out. Your API keys and passkeys were not \
             changed."
        ),
        "{text}"
    );
    assert!(!text.contains("were removed"), "{text}");
}
