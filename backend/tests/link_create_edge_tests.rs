//! Link-create edge cases against the real router: empty / non-http URLs,
//! the 2048-char sanitizer cap, unicode path+title, generated short codes.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use serde_json::{json, Value};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &sea_orm::DatabaseConnection,
) -> String {
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

async fn create(
    server: &axum_test::TestServer,
    token: &str,
    body: Value,
) -> axum_test::TestResponse {
    server
        .post("/links")
        .authorization_bearer(token)
        .json(&body)
        .await
}

#[tokio::test]
async fn create_link_rejects_empty_and_non_http_urls() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    for (label, body) in [
        ("empty string", json!({ "original_url": "" })),
        ("empty payload", json!({})),
        ("no scheme", json!({ "original_url": "iana.org" })),
        (
            "javascript",
            json!({ "original_url": "javascript:alert(1)" }),
        ),
        ("ftp", json!({ "original_url": "ftp://iana.org/file" })),
    ] {
        let res = create(&server, &token, body).await;
        assert_eq!(
            res.status_code(),
            400,
            "{label} must be rejected: {}",
            res.text()
        );
    }
}

#[tokio::test]
async fn create_link_rejects_url_over_2048_chars() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let url = format!("https://iana.org/{}", "a".repeat(2040));
    assert!(url.len() > 2048);
    let res = create(&server, &token, json!({ "original_url": url })).await;
    assert_eq!(res.status_code(), 400, "over-long: {}", res.text());
    assert!(
        res.text().to_lowercase().contains("too long"),
        "expected length error: {}",
        res.text()
    );
}

#[tokio::test]
async fn create_link_accepts_unicode_path_and_title_and_mints_six_char_code() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let res = create(
        &server,
        &token,
        json!({
            "original_url": "https://iana.org/путь",
            "title": "ссылка 链接"
        }),
    )
    .await;
    assert_eq!(res.status_code(), 201, "unicode create: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["original_url"], "https://iana.org/путь");
    assert_eq!(body["title"], "ссылка 链接");
    let code = body["code"].as_str().expect("code");
    assert_eq!(code.len(), 6, "generated short code is 6 chars: {code}");
    assert!(
        code.chars().all(|c| c.is_ascii_alphanumeric()),
        "generated short code is alphanumeric: {code}"
    );
}
