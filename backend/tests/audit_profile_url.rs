//! Profile website / avatar_url must reject javascript:/data: (stored XSS).
//! Also short-link create must reject localhost-style hosts.

mod common;

use common::{spawn_real_app, unique_email};
use serde_json::{json, Value};

async fn register(server: &axum_test::TestServer, email: &str) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register failed: {}", res.text());
    let body: Value = res.json();
    body["token"].as_str().expect("token").to_string()
}

#[tokio::test]
async fn profile_rejects_javascript_website() {
    let (server, _db) = spawn_real_app().await;
    let token = register(&server, &unique_email()).await;

    let res = server
        .put("/auth/profile")
        .authorization_bearer(&token)
        .json(&json!({ "website": "javascript:alert(document.domain)" }))
        .await;
    assert_eq!(res.status_code(), 400, "body: {}", res.text());
    assert!(
        res.text().to_lowercase().contains("http"),
        "error should mention http(s): {}",
        res.text()
    );
}

#[tokio::test]
async fn profile_rejects_data_and_file_schemes() {
    let (server, _db) = spawn_real_app().await;
    let token = register(&server, &unique_email()).await;

    for bad in [
        "data:text/html,hi",
        "file:///etc/passwd",
        "ftp://evil.example/x",
    ] {
        let res = server
            .put("/auth/profile")
            .authorization_bearer(&token)
            .json(&json!({ "website": bad }))
            .await;
        assert_eq!(res.status_code(), 400, "{bad} => {}", res.text());
    }
}

#[tokio::test]
async fn profile_rejects_javascript_avatar_url() {
    let (server, _db) = spawn_real_app().await;
    let token = register(&server, &unique_email()).await;

    let res = server
        .put("/auth/profile")
        .authorization_bearer(&token)
        .json(&json!({ "avatar_url": "javascript:alert(1)" }))
        .await;
    assert_eq!(res.status_code(), 400, "body: {}", res.text());
}

#[tokio::test]
async fn profile_accepts_https_website() {
    let (server, _db) = spawn_real_app().await;
    let token = register(&server, &unique_email()).await;

    let res = server
        .put("/auth/profile")
        .authorization_bearer(&token)
        .json(&json!({ "website": "https://iana.org/me" }))
        .await;
    assert_eq!(res.status_code(), 200, "body: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["website"], "https://iana.org/me");
}

#[tokio::test]
async fn create_link_rejects_localhost_destination() {
    let (server, _db) = spawn_real_app().await;

    let res = server
        .post("/links")
        .json(&json!({ "original_url": "http://localhost/secret" }))
        .await;
    assert_eq!(res.status_code(), 400, "body: {}", res.text());
    assert!(
        res.text().to_lowercase().contains("local")
            || res.text().to_lowercase().contains("internal"),
        "expected local/internal error: {}",
        res.text()
    );
}
