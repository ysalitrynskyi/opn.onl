//! Real-router coverage for hostless URLs, live schedule/expiry windows,
//! empty bulk create, and `/links/health-check`. Replaces theatre in
//! `link_features_tests.rs` that is not already covered by
//! `link_create_lifecycle_tests.rs` / `new_features_tests.rs` /
//! `click_cap_race_tests.rs`.

mod common;

use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
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

#[tokio::test]
async fn create_rejects_empty_and_hostless_urls() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    for url in ["", "https://", "http://", "//iana.org"] {
        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": url }))
            .await;
        assert_eq!(res.status_code(), 400, "{url:?}: {}", res.text());
    }
}

#[tokio::test]
async fn past_starts_at_still_redirects() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/started-{}", unique_code());
    let starts = (Utc::now() - Duration::hours(1)).to_rfc3339();
    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": dest, "starts_at": starts }))
        .await;
    assert_eq!(res.status_code(), 201, "create: {}", res.text());
    let code = res.json::<Value>()["code"].as_str().unwrap().to_string();

    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(
        redirect.status_code(),
        307,
        "past starts_at must be live: {}",
        redirect.text()
    );
    assert_eq!(
        redirect
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        dest
    );
}

#[tokio::test]
async fn future_expires_at_still_redirects() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/unexpired-{}", unique_code());
    let expires = (Utc::now() + Duration::hours(1)).to_rfc3339();
    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": dest, "expires_at": expires }))
        .await;
    assert_eq!(res.status_code(), 201, "create: {}", res.text());
    let code = res.json::<Value>()["code"].as_str().unwrap().to_string();

    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(
        redirect.status_code(),
        307,
        "future expires_at must be live: {}",
        redirect.text()
    );
    assert_eq!(
        redirect
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        dest
    );
}

#[tokio::test]
async fn bulk_create_empty_urls_returns_no_links() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let res = server
        .post("/links/bulk")
        .authorization_bearer(&token)
        .json(&json!({ "urls": [] }))
        .await;
    assert_eq!(res.status_code(), 200, "empty bulk: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["links"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(body["errors"].as_array().map(|a| a.len()), Some(0));
}

#[tokio::test]
async fn health_check_requires_auth_and_rejects_invalid_urls() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let unauth = server
        .post("/links/health-check")
        .json(&json!({ "url": "https://iana.org/" }))
        .await;
    assert_eq!(unauth.status_code(), 401, "health-check requires auth");

    for url in ["ftp://iana.org/file", "javascript:alert(1)", "not a url"] {
        let res = server
            .post("/links/health-check")
            .authorization_bearer(&token)
            .json(&json!({ "url": url }))
            .await;
        assert_eq!(res.status_code(), 400, "{url}: {}", res.text());
        let body: Value = res.json();
        assert_eq!(body["reachable"], false);
        assert!(
            body["error"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("invalid"),
            "{url} should be invalid: {body}"
        );
    }
}

/// starts_at in the past + expires_at in the future + max_clicks under the cap
/// must still 307. Each constraint is covered alone; this pins the conjunction
/// the scheduling theatre claimed to test.
#[tokio::test]
async fn combined_schedule_window_and_cap_is_live() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let dest = format!("https://iana.org/window-{}", unique_code());
    let starts = (Utc::now() - Duration::hours(1)).to_rfc3339();
    let expires = (Utc::now() + Duration::hours(1)).to_rfc3339();
    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": dest,
            "starts_at": starts,
            "expires_at": expires,
            "max_clicks": 50
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create: {}", res.text());
    let body: Value = res.json();
    let code = body["code"].as_str().unwrap();
    assert_eq!(body["max_clicks"], 50);

    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(
        redirect.status_code(),
        307,
        "in-window capped link must redirect: {}",
        redirect.text()
    );
    assert_eq!(
        redirect
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        dest
    );
}
