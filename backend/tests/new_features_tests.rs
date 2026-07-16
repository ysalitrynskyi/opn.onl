//! Integration tests for the "new feature" link handlers: clone, pin,
//! code-availability, and UTM building. Real router + real Postgres via
//! `common::spawn_real_app`.
//!
//! This file previously asserted on local literals (e.g. `assert!(pinned ||
//! !pinned)`), which exercised none of the production handlers. It now drives
//! the real endpoints so the tests fail if the handlers regress.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_link(server: &axum_test::TestServer, token: &str, body: Value) -> Value {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json()
}

#[tokio::test]
async fn clone_creates_new_code_and_resets_state() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let original = create_link(
        &server,
        &token,
        json!({ "original_url": "https://example.com/page", "title": "My Link" }),
    )
    .await;
    let original_id = original["id"].as_i64().unwrap();
    let original_code = original["code"].as_str().unwrap().to_string();

    // Pin the original so we can prove the clone does NOT inherit the pin.
    let pin = server
        .post(&format!("/links/{original_id}/pin"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(pin.status_code(), 200, "pin original: {}", pin.text());

    let clone = server
        .post(&format!("/links/{original_id}/clone"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(clone.status_code(), 201, "clone: {}", clone.text());
    let clone_body: Value = clone.json();
    let clone_id = clone_body["id"].as_i64().unwrap();

    // New, distinct short code; same destination.
    assert_ne!(clone_body["code"].as_str().unwrap(), original_code);
    assert_eq!(
        clone_body["original_url"].as_str().unwrap(),
        "https://example.com/page"
    );

    // Inspect the cloned link in the listing: "(copy)" title, unpinned, 0 clicks.
    let list: Value = server
        .get("/links")
        .authorization_bearer(&token)
        .await
        .json();
    let cloned = list
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"].as_i64() == Some(clone_id))
        .expect("cloned link in listing");
    assert_eq!(cloned["title"].as_str(), Some("My Link (copy)"));
    assert_eq!(
        cloned["is_pinned"].as_bool(),
        Some(false),
        "clone must not inherit pin"
    );
    assert_eq!(
        cloned["click_count"].as_i64(),
        Some(0),
        "clone must start at 0 clicks"
    );
}

#[tokio::test]
async fn toggle_pin_flips_state() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let link = create_link(
        &server,
        &token,
        json!({ "original_url": "https://example.com/p" }),
    )
    .await;
    let id = link["id"].as_i64().unwrap();

    let first: Value = server
        .post(&format!("/links/{id}/pin"))
        .authorization_bearer(&token)
        .await
        .json();
    assert_eq!(
        first["is_pinned"].as_bool(),
        Some(true),
        "first toggle pins"
    );

    let second: Value = server
        .post(&format!("/links/{id}/pin"))
        .authorization_bearer(&token)
        .await
        .json();
    assert_eq!(
        second["is_pinned"].as_bool(),
        Some(false),
        "second toggle unpins"
    );
}

#[tokio::test]
async fn check_code_availability_reflects_taken_codes() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let alias = unique_code();

    // Unused, valid alias is available.
    let before: Value = server
        .get(&format!("/links/check-code?code={alias}"))
        .await
        .json();
    assert_eq!(
        before["available"].as_bool(),
        Some(true),
        "unused alias should be available"
    );

    // Take it, then it must report unavailable.
    create_link(
        &server,
        &token,
        json!({ "original_url": "https://example.com/x", "custom_alias": alias }),
    )
    .await;

    let after: Value = server
        .get(&format!("/links/check-code?code={alias}"))
        .await
        .json();
    assert_eq!(
        after["available"].as_bool(),
        Some(false),
        "taken alias should be unavailable"
    );
}

#[tokio::test]
async fn check_code_rejects_invalid_alias() {
    let (server, _db) = spawn_real_app().await;
    // A slash is not a valid alias character; the handler must run validate_alias
    // and report it unavailable rather than accepting it.
    let res: Value = server
        .get("/links/check-code?code=bad%2Fslash")
        .await
        .json();
    assert_eq!(
        res["available"].as_bool(),
        Some(false),
        "invalid alias must be unavailable"
    );
}

#[tokio::test]
async fn build_utm_url_appends_parameters() {
    let (server, _db) = spawn_real_app().await;
    let res = server
        .post("/links/build-utm")
        .json(&json!({
            "url": "https://example.com/landing",
            "utm_source": "newsletter",
            "utm_medium": "email",
            "utm_campaign": "spring_sale"
        }))
        .await;
    assert_eq!(res.status_code(), 200, "build-utm: {}", res.text());
    let built = res.json::<Value>()["url_with_utm"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        built.contains("utm_source=newsletter"),
        "missing utm_source in {built}"
    );
    assert!(
        built.contains("utm_medium=email"),
        "missing utm_medium in {built}"
    );
    assert!(
        built.contains("utm_campaign=spring_sale"),
        "missing utm_campaign in {built}"
    );
}

#[tokio::test]
async fn build_utm_url_rejects_invalid_url() {
    let (server, _db) = spawn_real_app().await;
    let res = server
        .post("/links/build-utm")
        .json(&json!({ "url": "not a url", "utm_source": "x" }))
        .await;
    assert_eq!(res.status_code(), 400, "invalid URL must be rejected");
}
