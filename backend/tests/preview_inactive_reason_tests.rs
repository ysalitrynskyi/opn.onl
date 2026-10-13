//! `GET /{code}/preview` says why a link is not live, so the preview page can
//! tell a link that has not started yet from one that is expired, used up or
//! blocked. Real router + real Postgres.

mod common;

use axum_test::TestServer;
use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::blocked_domains;
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet, ActiveValue::Set, DatabaseConnection};
use serde_json::{Value, json};

async fn register_verified(server: &TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    mark_email_verified(db, body["user_id"].as_i64().expect("user_id") as i32).await;
    body["token"].as_str().expect("token").to_string()
}

async fn create_link(server: &TestServer, token: &str, mut payload: Value) -> String {
    payload["custom_alias"] = json!(unique_code());
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["code"]
        .as_str()
        .expect("code")
        .to_string()
}

async fn preview(server: &TestServer, code: &str) -> Value {
    let res = server.get(&format!("/{code}/preview")).await;
    assert_eq!(res.status_code(), 200, "preview: {}", res.text());
    res.json()
}

async fn block_domain(db: &DatabaseConnection, host: &str) {
    blocked_domains::ActiveModel {
        id: NotSet,
        domain: Set(host.to_string()),
        reason: Set(Some("test".to_string())),
        blocked_by: Set(None),
        created_at: Set(Utc::now().naive_utc()),
    }
    .insert(db)
    .await
    .expect("block domain");
}

fn unique_host() -> String {
    format!("blk-{}.iana.org", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn a_live_link_has_no_inactive_reason() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({ "original_url": "https://iana.org/reason-live" }),
    )
    .await;

    let body = preview(&server, &code).await;
    assert_eq!(body["is_active"], true, "{body}");
    assert!(body["inactive_reason"].is_null(), "{body}");
}

#[tokio::test]
async fn a_scheduled_link_is_not_started_rather_than_expired() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({
            "original_url": "https://iana.org/reason-scheduled",
            "starts_at": (Utc::now() + Duration::hours(2)).to_rfc3339(),
        }),
    )
    .await;

    let body = preview(&server, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_eq!(body["is_expired"], false, "{body}");
    assert_eq!(body["inactive_reason"], "not_started", "{body}");
}

#[tokio::test]
async fn an_expired_link_is_expired() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({
            "original_url": "https://iana.org/reason-expired",
            "expires_at": (Utc::now() - Duration::hours(1)).to_rfc3339(),
        }),
    )
    .await;

    let body = preview(&server, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_eq!(body["is_expired"], true, "{body}");
    assert_eq!(body["inactive_reason"], "expired", "{body}");
}

#[tokio::test]
async fn a_link_that_hit_its_click_cap_is_used_up() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({ "original_url": "https://iana.org/reason-capped", "max_clicks": 1 }),
    )
    .await;

    // Control: with clicks left there is nothing to explain.
    assert!(preview(&server, &code).await["inactive_reason"].is_null());

    assert_eq!(server.get(&format!("/{code}")).await.status_code(), 307);
    assert_eq!(server.get(&format!("/{code}")).await.status_code(), 410);

    let body = preview(&server, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_eq!(body["burn_after_reading"], false, "{body}");
    assert_eq!(body["inactive_reason"], "used_up", "{body}");
}

#[tokio::test]
async fn an_opened_one_time_link_is_used_up() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({ "original_url": "https://iana.org/reason-burned", "burn_after_reading": true }),
    )
    .await;
    assert!(preview(&server, &code).await["inactive_reason"].is_null());

    assert_eq!(server.get(&format!("/{code}")).await.status_code(), 307);

    let body = preview(&server, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_eq!(body["burn_after_reading"], true, "{body}");
    assert_eq!(body["inactive_reason"], "used_up", "{body}");
}

#[tokio::test]
async fn a_link_to_a_blocked_domain_is_blocked() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let host = unique_host();
    let code = create_link(
        &server,
        &token,
        json!({ "original_url": format!("https://{host}/reason-blocked") }),
    )
    .await;
    assert!(preview(&server, &code).await["inactive_reason"].is_null());

    block_domain(&db, &host).await;

    let body = preview(&server, &code).await;
    assert_eq!(body["is_active"], false, "{body}");
    assert_eq!(body["inactive_reason"], "blocked", "{body}");
}

/// A visitor to `GET /{code}` is told the link's own state before the
/// blocklist, so the preview agrees with what the visit will say.
#[tokio::test]
async fn the_links_own_state_is_reported_before_the_blocklist() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let host = unique_host();
    let code = create_link(
        &server,
        &token,
        json!({
            "original_url": format!("https://{host}/reason-both"),
            "starts_at": (Utc::now() + Duration::hours(2)).to_rfc3339(),
        }),
    )
    .await;
    block_domain(&db, &host).await;

    let body = preview(&server, &code).await;
    assert_eq!(body["inactive_reason"], "not_started", "{body}");
}
