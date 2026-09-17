//! Rate-limit path classification and click-buffer behaviour against the real
//! router and a real Postgres database.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use serde_json::{json, Value};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &sea_orm::DatabaseConnection,
) -> String {
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().expect("user_id") as i32;
    mark_email_verified(db, user_id).await;
    body["token"].as_str().expect("token").to_string()
}

#[tokio::test]
async fn custom_alias_starting_with_auth_uses_redirect_limit_not_login_limit() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let alias = format!("auth-sale-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/auth-sale",
            "custom_alias": alias,
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create: {}", res.text());

    // The auth limiter is 10/min. A short-link whose code happens to start
    // with "auth" must stay on the redirect bucket (100/sec), so eleven
    // clicks in one window must not 429.
    for i in 0..11 {
        let res = server.get(&format!("/{alias}")).await;
        assert_eq!(
            res.status_code(),
            307,
            "click {i} of /{alias} must not inherit the /auth login limit: {}",
            res.text()
        );
    }
}

fn rate_limit_remaining(res: &axum_test::TestResponse) -> i64 {
    res.headers()
        .get("x-ratelimit-remaining")
        .expect("X-RateLimit-Remaining")
        .to_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn post_pin_does_not_consume_link_creation_budget() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/pin-limit" }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    assert_eq!(
        rate_limit_remaining(&created),
        99,
        "create spends one slot of the hourly bucket"
    );
    let id = created.json::<Value>()["id"].as_i64().expect("id");

    let pin = server
        .post(&format!("/links/{id}/pin"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(pin.status_code(), 200, "pin: {}", pin.text());
    assert_eq!(
        rate_limit_remaining(&pin),
        99,
        "pin must use the general bucket, not the hourly create budget"
    );
}
