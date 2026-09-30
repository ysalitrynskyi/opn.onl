//! Dashboard `clicks_today` is a UTC calendar-day bucket, not the caller's
//! local day. Real router + real Postgres via `common::spawn_real_app`.

mod common;

use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_email};
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
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

/// A click one second before UTC midnight must not count as "today"; a click
/// one second after must. This is the published meaning of `clicks_today`.
#[tokio::test]
async fn clicks_today_buckets_from_utc_midnight() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/dash-today" }))
        .await;
    assert_eq!(
        created.status_code(),
        201,
        "create link: {}",
        created.text()
    );
    let link_id = created.json::<Value>()["id"].as_i64().unwrap() as i32;

    let today_start = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let before = today_start - Duration::seconds(1);
    let after = today_start + Duration::seconds(1);

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO click_events (link_id, created_at) VALUES ($1, $2), ($1, $3)",
        [link_id.into(), before.into(), after.into()],
    ))
    .await
    .expect("insert click fixtures");

    let res = server
        .get("/analytics/dashboard")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "dashboard: {}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["clicks_today"].as_i64(),
        Some(1),
        "clicks_today must count only events on or after 00:00:00 UTC, got {body}"
    );
}

/// Callers have no timezone query param; the spec must say the bucket is UTC
/// so a local-day reading is not implied by the field name alone.
#[tokio::test]
async fn openapi_documents_clicks_today_as_utc() {
    let (server, _db) = spawn_real_app().await;
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    let clicks_today =
        &spec["components"]["schemas"]["DashboardStats"]["properties"]["clicks_today"];
    let description = clicks_today["description"].as_str().unwrap_or("");
    assert!(
        description.to_ascii_lowercase().contains("utc"),
        "DashboardStats.clicks_today must be documented as UTC, got {clicks_today}"
    );
}
