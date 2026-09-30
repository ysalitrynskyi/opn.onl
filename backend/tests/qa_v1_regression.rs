//! Regressions from the 2026-09-30 QA pass that were still true on main.
//! Real router + real Postgres. Destinations that need a public DNS answer
//! use iana.org; rebinding names use 127.0.0.1.nip.io.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{links, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id)
}

fn assert_no_secret(body: &str, secret: &str) {
    assert!(!body.contains(secret), "response echoed {secret:?}: {body}");
    assert!(
        !body.contains("String("),
        "response included validator debug output: {body}"
    );
}

#[tokio::test]
async fn create_rejects_control_characters_and_private_dns() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let crlf = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/a\r\nX-Injected: 1" }))
        .await;
    assert_eq!(crlf.status_code(), 400, "crlf: {}", crlf.text());
    assert!(
        crlf.text().contains("control characters"),
        "crlf reason: {}",
        crlf.text()
    );

    let rebinding = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "http://127.0.0.1.nip.io/qa" }))
        .await;
    assert_eq!(rebinding.status_code(), 400, "nip.io: {}", rebinding.text());
    assert!(
        rebinding.text().contains("disallowed") || rebinding.text().contains("private"),
        "nip.io reason: {}",
        rebinding.text()
    );

    let ok = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/qa-dns-ok" }))
        .await;
    assert_eq!(ok.status_code(), 201, "public dest: {}", ok.text());

    let bulk = server
        .post("/links/bulk")
        .authorization_bearer(&token)
        .json(&json!({
            "urls": [
                "http://127.0.0.1.nip.io/bulk",
                "https://iana.org/qa-bulk-ok"
            ]
        }))
        .await;
    assert_eq!(bulk.status_code(), 200, "bulk: {}", bulk.text());
    let bulk_body: Value = bulk.json();
    let errors = bulk_body["errors"].as_array().unwrap();
    assert!(
        errors
            .iter()
            .any(|e| e.as_str().unwrap_or("").contains("nip.io")),
        "bulk should report the private host: {bulk_body}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.as_str().unwrap_or("").contains("iana.org")),
        "public bulk url should be stored: {bulk_body}"
    );
}

#[tokio::test]
async fn stored_private_destination_cannot_be_cloned_or_routed() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/qa-clone-base" }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    let id = created.json::<Value>()["id"].as_i64().unwrap() as i32;

    let model = links::Entity::find_by_id(id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    let mut active: links::ActiveModel = model.into();
    active.original_url = Set("http://127.0.0.1.nip.io/cloned".to_string());
    active.update(&db).await.expect("rewrite destination");

    let cloned = server
        .post(&format!("/links/{id}/clone"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(cloned.status_code(), 400, "clone: {}", cloned.text());
    assert!(
        cloned.text().contains("disallowed") || cloned.text().contains("private"),
        "clone reason: {}",
        cloned.text()
    );

    // Restore a public URL so the row is a valid base, then refuse a private rule.
    let model = links::Entity::find_by_id(id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    let mut active: links::ActiveModel = model.into();
    active.original_url = Set("https://iana.org/qa-clone-base".to_string());
    active.update(&db).await.expect("restore destination");

    let rules = server
        .put(&format!("/links/{id}/rules"))
        .authorization_bearer(&token)
        .json(&json!({
            "rules": [{ "destination_url": "http://127.0.0.1.nip.io/rule" }]
        }))
        .await;
    assert_eq!(rules.status_code(), 400, "rules: {}", rules.text());
    assert!(
        rules.text().contains("disallowed") || rules.text().contains("private"),
        "rules reason: {}",
        rules.text()
    );
}

#[tokio::test]
async fn list_search_matches_title_and_alias_is_not_editable() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/qa-title-search",
            "title": "QaLifecycleTitle"
        }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    let created: Value = created.json();
    let id = created["id"].as_i64().unwrap();
    let code = created["code"].as_str().unwrap().to_string();

    let found = server
        .get("/links")
        .add_query_param("search", "QaLifecycleTitle")
        .authorization_bearer(&token)
        .await;
    assert_eq!(found.status_code(), 200, "search: {}", found.text());
    let rows: Vec<Value> = found.json();
    assert!(
        rows.iter().any(|row| row["id"].as_i64() == Some(id)),
        "title search missed the link: {rows:?}"
    );

    let same = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "custom_alias": code }))
        .await;
    assert_eq!(same.status_code(), 200, "same alias: {}", same.text());

    let changed = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "custom_alias": unique_code() }))
        .await;
    assert_eq!(changed.status_code(), 400, "new alias: {}", changed.text());
    assert!(
        changed.text().contains("cannot be changed"),
        "alias reason: {}",
        changed.text()
    );

    let listed = server.get("/links").authorization_bearer(&token).await;
    let rows: Vec<Value> = listed.json();
    let row = rows
        .iter()
        .find(|row| row["id"].as_i64() == Some(id))
        .unwrap();
    assert_eq!(row["code"].as_str(), Some(code.as_str()));
}

#[tokio::test]
async fn health_check_reports_the_specific_url_error() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let reserved = server
        .post("/links/health-check")
        .authorization_bearer(&token)
        .json(&json!({ "url": "https://example.com/health" }))
        .await;
    assert_eq!(reserved.status_code(), 400, "reserved: {}", reserved.text());
    let reserved: Value = reserved.json();
    let message = reserved["error"].as_str().unwrap();
    assert!(
        message.contains("reserved example domains"),
        "reserved message: {message}"
    );
    assert_ne!(message, "Invalid URL format");

    let local = server
        .post("/links/health-check")
        .authorization_bearer(&token)
        .json(&json!({ "url": "http://localhost/health" }))
        .await;
    assert_eq!(local.status_code(), 400, "local: {}", local.text());
    let local: Value = local.json();
    let message = local["error"].as_str().unwrap();
    assert!(
        message.contains("local") || message.contains("internal"),
        "local message: {message}"
    );
}

#[tokio::test]
async fn change_password_rejects_the_same_password_without_revoking_the_session() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    let wrong = server
        .post("/auth/change-password")
        .authorization_bearer(&token)
        .json(&json!({
            "current_password": "not-the-password",
            "new_password": "password456"
        }))
        .await;
    assert_eq!(wrong.status_code(), 400, "wrong: {}", wrong.text());
    assert!(
        wrong.text().contains("incorrect"),
        "wrong: {}",
        wrong.text()
    );
    assert!(
        !wrong.text().contains("different"),
        "wrong password must not look like a same-password refusal: {}",
        wrong.text()
    );

    let leaked = "ZzLeak7";
    let short = server
        .post("/auth/change-password")
        .authorization_bearer(&token)
        .json(&json!({
            "current_password": "password123",
            "new_password": leaked
        }))
        .await;
    assert_eq!(short.status_code(), 400, "short: {}", short.text());
    assert_no_secret(&short.text(), leaked);

    let same = server
        .post("/auth/change-password")
        .authorization_bearer(&token)
        .json(&json!({
            "current_password": "password123",
            "new_password": "password123"
        }))
        .await;
    assert_eq!(same.status_code(), 400, "same: {}", same.text());
    assert!(same.text().contains("different"), "same: {}", same.text());
    assert_no_secret(&same.text(), "password123");

    let row = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .expect("db")
        .expect("user");
    assert_eq!(
        row.token_version, 0,
        "same-password change revoked sessions"
    );

    let me = server.get("/auth/me").authorization_bearer(&token).await;
    assert_eq!(me.status_code(), 200, "old token: {}", me.text());
}

#[tokio::test]
async fn validation_errors_do_not_echo_the_submitted_value() {
    let (server, _db) = spawn_real_app().await;

    let leaked_email = "leak-token-xyz";
    let register = server
        .post("/auth/register")
        .json(&json!({ "email": leaked_email, "password": "password123" }))
        .await;
    assert_eq!(register.status_code(), 400, "register: {}", register.text());
    assert_no_secret(&register.text(), leaked_email);
    assert!(
        register.text().contains("email"),
        "register should name the field: {}",
        register.text()
    );

    let contact = server
        .post("/contact")
        .json(&json!({
            "name": "Ada",
            "email": leaked_email,
            "subject": "A real subject",
            "message": "This message is long enough to pass."
        }))
        .await;
    assert_eq!(contact.status_code(), 400, "contact: {}", contact.text());
    assert_no_secret(&contact.text(), leaked_email);
}

#[tokio::test]
async fn avatar_proxy_fetches_only_a_live_bio_avatar() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;
    let avatar = "http://127.0.0.1.nip.io/qa-avatar.png";

    let blocked = server
        .get("/api/bio/avatar")
        .add_query_param("url", avatar)
        .await;
    assert_eq!(
        blocked.status_code(),
        404,
        "unlisted avatar must not be fetched: {}",
        blocked.text()
    );

    let profile = server
        .put("/auth/profile")
        .authorization_bearer(&token)
        .json(&json!({ "avatar_url": avatar }))
        .await;
    assert_eq!(profile.status_code(), 200, "profile: {}", profile.text());

    let username = format!("qa{}", unique_code().to_lowercase());
    let enabled = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({ "bio_username": username, "bio_enabled": true }))
        .await;
    assert_eq!(enabled.status_code(), 200, "enable bio: {}", enabled.text());

    let allowed = server
        .get("/api/bio/avatar")
        .add_query_param("url", avatar)
        .await;
    assert_ne!(
        allowed.status_code(),
        404,
        "a live bio avatar must get past the allowlist: {}",
        allowed.text()
    );
    assert!(
        allowed.status_code().is_server_error() || allowed.status_code().as_u16() == 415,
        "nip.io avatar is not a public image, got {}: {}",
        allowed.status_code(),
        allowed.text()
    );

    let disabled = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({ "bio_enabled": false }))
        .await;
    assert_eq!(disabled.status_code(), 200, "disable: {}", disabled.text());

    let hidden = server
        .get("/api/bio/avatar")
        .add_query_param("url", avatar)
        .await;
    assert_eq!(
        hidden.status_code(),
        404,
        "disabled bio must not proxy: {}",
        hidden.text()
    );
}
