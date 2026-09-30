//! GET /analytics/dashboard must aggregate the last 30 days of clicks in SQL
//! without changing the published numbers. Real router + real Postgres.

mod common;

use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseBackend, DatabaseConnection,
    Statement,
};
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

async fn insert_link(db: &DatabaseConnection, user_id: i32, url: &str) -> i32 {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set(url.to_string()),
        user_id: Set(Some(user_id)),
        click_count: Set(0),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
    .id
}

async fn insert_click(
    db: &DatabaseConnection,
    link_id: i32,
    created_at: chrono::NaiveDateTime,
    country: Option<&str>,
    browser: Option<&str>,
) {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO click_events (link_id, created_at, country, browser) VALUES ($1, $2, $3, $4)",
        [
            link_id.into(),
            created_at.into(),
            country.into(),
            browser.into(),
        ],
    ))
    .await
    .expect("insert click");
}

fn country_count(body: &Value, name: &str) -> i64 {
    body["top_countries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["country"].as_str() == Some(name))
        .and_then(|c| c["count"].as_i64())
        .unwrap_or(0)
}

fn browser_count(body: &Value, name: &str) -> i64 {
    body["top_browsers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["browser"].as_str() == Some(name))
        .and_then(|b| b["count"].as_i64())
        .unwrap_or(0)
}

fn day_count(body: &Value, date: &str) -> i64 {
    body["clicks_by_day"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["date"].as_str() == Some(date))
        .and_then(|d| d["count"].as_i64())
        .unwrap_or(0)
}

#[tokio::test]
async fn dashboard_aggregates_window_geo_and_browser_without_loading_every_row() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (_other_token, other_id) = register_verified(&server, &db).await;

    let live = insert_link(&db, user_id, "https://iana.org/live").await;
    let gone = insert_link(&db, user_id, "https://iana.org/gone").await;
    let foreign = insert_link(&db, other_id, "https://iana.org/foreign").await;

    let now = Utc::now().naive_utc();
    let today_start = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let today = today_start + Duration::hours(2);
    let this_week = now - Duration::days(3);
    let this_month = now - Duration::days(20);
    let too_old = now - Duration::days(40);
    let today_key = today_start.format("%Y-%m-%d").to_string();
    let week_key = this_week.date().format("%Y-%m-%d").to_string();
    let month_key = this_month.date().format("%Y-%m-%d").to_string();

    // Live link: 1 today (DE/Firefox), 1 this week (US/Chrome), 1 this month (null/null),
    // plus one older than 30 days that must not count.
    insert_click(&db, live, today, Some("DE"), Some("Firefox")).await;
    insert_click(&db, live, this_week, Some("US"), Some("Chrome")).await;
    insert_click(&db, live, this_month, None, None).await;
    insert_click(&db, live, too_old, Some("FR"), Some("Safari")).await;
    // Soft-deleted link still has a recent click — must not count.
    insert_click(&db, gone, today, Some("DE"), Some("Firefox")).await;
    // Another user's click must not count.
    insert_click(&db, foreign, today, Some("US"), Some("Chrome")).await;

    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [gone] }))
        .await;
    assert_eq!(del.status_code(), 200, "soft-delete: {}", del.text());

    let res = server
        .get("/analytics/dashboard")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "dashboard: {}", res.text());
    let body: Value = res.json();

    assert_eq!(body["total_links"].as_i64(), Some(1), "only the live link");
    assert_eq!(body["clicks_today"].as_i64(), Some(1), "got {body}");
    assert_eq!(body["clicks_this_week"].as_i64(), Some(2), "got {body}");
    assert_eq!(body["clicks_this_month"].as_i64(), Some(3), "got {body}");

    assert_eq!(country_count(&body, "DE"), 1);
    assert_eq!(country_count(&body, "US"), 1);
    assert_eq!(country_count(&body, "Unknown"), 1);
    assert_eq!(
        country_count(&body, "FR"),
        0,
        "40-day-old click must drop out"
    );

    assert_eq!(browser_count(&body, "Firefox"), 1);
    assert_eq!(browser_count(&body, "Chrome"), 1);
    assert_eq!(browser_count(&body, "Unknown"), 1);
    assert_eq!(browser_count(&body, "Safari"), 0);

    assert_eq!(day_count(&body, &today_key), 1);
    assert_eq!(day_count(&body, &week_key), 1);
    assert_eq!(day_count(&body, &month_key), 1);

    let countries = body["top_countries"].as_array().unwrap();
    let total_pct: f64 = countries
        .iter()
        .map(|c| c["percentage"].as_f64().unwrap_or(0.0))
        .sum();
    assert!(
        (total_pct - 100.0).abs() < 0.01,
        "country percentages must sum to 100, got {total_pct} from {countries:?}"
    );
}

#[tokio::test]
async fn dashboard_with_no_links_returns_zero_aggregates() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let res = server
        .get("/analytics/dashboard")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "empty dashboard: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["total_links"].as_i64(), Some(0));
    assert_eq!(body["total_clicks"].as_i64(), Some(0));
    assert_eq!(body["clicks_today"].as_i64(), Some(0));
    assert_eq!(body["clicks_this_week"].as_i64(), Some(0));
    assert_eq!(body["clicks_this_month"].as_i64(), Some(0));
    assert_eq!(body["top_countries"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(body["top_browsers"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(body["clicks_by_day"].as_array().map(|a| a.len()), Some(0));
}
