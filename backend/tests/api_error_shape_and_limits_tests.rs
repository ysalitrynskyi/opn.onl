//! Follow-ups to the 2026-09-30 QA pass: every JSON endpoint answers errors as
//! `{"error": "..."}`, and free-text fields have limits. Real router + real
//! Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    mark_email_verified(db, body["user_id"].as_i64().unwrap() as i32).await;
    body["token"].as_str().unwrap().to_string()
}

/// The error body is JSON with a non-empty `error` string that contains
/// `needle`, and nothing else a client would have to special-case.
fn assert_json_error(res: &axum_test::TestResponse, status: u16, needle: &str) {
    assert_eq!(res.status_code(), status, "{}", res.text());
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "error must be JSON, got {content_type:?}: {}",
        res.text()
    );
    let body: Value = res.json();
    let error = body["error"].as_str().unwrap_or_default();
    assert!(
        error.contains(needle),
        "expected {needle:?} in the error, got {body}"
    );
}

#[tokio::test]
async fn errors_that_used_to_be_plain_text_are_json() {
    let (server, _db) = spawn_real_app().await;

    // No session at all.
    for path in ["/auth/api-keys", "/links/1/rules", "/links/export"] {
        let res = server.get(path).await;
        assert_json_error(&res, 401, "Unauthorized");
    }
    let res = server
        .put("/auth/bio")
        .json(&json!({ "bio_username": "someone" }))
        .await;
    assert_json_error(&res, 401, "Unauthorized");
}

// Each test spawns its own router, so each stays under the 10 requests per
// second a single client may make.
#[tokio::test]
async fn signed_in_errors_that_used_to_be_plain_text_are_json() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    // Bio: an invalid username is a 400 with the reason.
    let res = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({ "bio_username": "x" }))
        .await;
    assert_json_error(&res, 400, "Username must be");

    // Routing rules: a private destination is refused with the reason.
    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/error-shape" }))
        .await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    let id = created.json::<Value>()["id"].as_i64().unwrap();
    let res = server
        .put(&format!("/links/{id}/rules"))
        .authorization_bearer(&token)
        .json(&json!({ "rules": [{ "destination_url": "http://127.0.0.1/rule" }] }))
        .await;
    assert_json_error(&res, 400, "not allowed");

    // API keys: revoking a key that does not exist.
    let res = server
        .delete("/auth/api-keys/2147483647")
        .authorization_bearer(&token)
        .await;
    assert!(
        res.status_code() == 404 || res.status_code() == 403,
        "{}",
        res.text()
    );
    let body: Value = res.json();
    assert!(body["error"].is_string(), "{body}");
}

#[tokio::test]
async fn names_colors_and_slugs_are_validated() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    for (path, body, needle) in [
        (
            "/tags",
            json!({ "name": "   " }),
            "Tag name must not be empty",
        ),
        (
            "/tags",
            json!({ "name": "ok", "color": "red; background:url(x)" }),
            "hex color",
        ),
        (
            "/folders",
            json!({ "name": "x".repeat(101) }),
            "at most 100",
        ),
        (
            "/folders",
            json!({ "name": "ok", "color": "#12" }),
            "hex color",
        ),
        (
            "/orgs",
            json!({ "name": "Acme", "slug": "Acme Corp" }),
            "Slug must be",
        ),
        (
            "/orgs",
            json!({ "name": "", "slug": "acme-ok" }),
            "must not be empty",
        ),
    ] {
        let res = server
            .post(path)
            .authorization_bearer(&token)
            .json(&body)
            .await;
        assert_json_error(&res, 400, needle);
    }
}

#[tokio::test]
async fn valid_values_pass_and_link_text_is_capped() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    // Valid values still work, and an empty colour stores no colour.
    let tag = server
        .post("/tags")
        .authorization_bearer(&token)
        .json(&json!({ "name": "launch", "color": "" }))
        .await;
    assert_eq!(tag.status_code(), 201, "{}", tag.text());
    assert!(tag.json::<Value>()["color"].is_null(), "{}", tag.text());
    let folder = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "Q4", "color": "#2563eb" }))
        .await;
    assert_eq!(folder.status_code(), 201, "{}", folder.text());

    let long_title = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/t", "title": "t".repeat(501) }))
        .await;
    assert_json_error(&long_title, 400, "Title must be at most 500");

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/n", "title": "t".repeat(500) }))
        .await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    let id = created.json::<Value>()["id"].as_i64().unwrap();
    let long_notes = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "notes": "n".repeat(5001) }))
        .await;
    assert_json_error(&long_notes, 400, "Notes must be at most 5000");

    let theme = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({ "bio_theme": "<script>" }))
        .await;
    assert_json_error(&theme, 400, "Theme must be");
}
