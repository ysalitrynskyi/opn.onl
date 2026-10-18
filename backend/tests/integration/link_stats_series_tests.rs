//! The shape of what `GET /links/{id}/stats` and `GET /analytics/dashboard`
//! return for charts: one entry per UTC day with zeros kept, and cities told
//! apart by country. Real router + real Postgres.

use crate::common;

use chrono::{Duration, NaiveDate, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{click_events, links};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseConnection};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    let user_id = body["user_id"].as_i64().expect("user_id") as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().expect("token").to_string(), user_id)
}

async fn insert_link(db: &DatabaseConnection, user_id: i32) -> i32 {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set("https://iana.org/series".to_string()),
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
    days_ago: i64,
    city: Option<&str>,
    country: Option<&str>,
) {
    click_events::ActiveModel {
        link_id: Set(link_id),
        created_at: Set(Utc::now().naive_utc() - Duration::days(days_ago)),
        city: Set(city.map(str::to_string)),
        country: Set(country.map(str::to_string)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert click event");
}

async fn link_stats(server: &axum_test::TestServer, token: &str, link_id: i32, days: i64) -> Value {
    let res = server
        .get(&format!("/links/{link_id}/stats"))
        .authorization_bearer(token)
        .add_query_param("days", days)
        .await;
    assert_eq!(res.status_code(), 200, "stats: {}", res.text());
    res.json()
}

/// The `(date, count)` pairs of a `clicks_by_day` array.
fn series(body: &Value) -> Vec<(NaiveDate, i64)> {
    body["clicks_by_day"]
        .as_array()
        .unwrap_or_else(|| panic!("clicks_by_day in {body}"))
        .iter()
        .map(|d| {
            (
                d["date"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| panic!("a YYYY-MM-DD date in {d}")),
                d["count"].as_i64().expect("count"),
            )
        })
        .collect()
}

/// Every day from the first to the last, none skipped.
fn assert_consecutive(days: &[(NaiveDate, i64)]) {
    for pair in days.windows(2) {
        assert_eq!(
            pair[1].0 - pair[0].0,
            Duration::days(1),
            "{} and {} are not adjacent days: {days:?}",
            pair[0].0,
            pair[1].0
        );
    }
}

fn utc_day(days_ago: i64) -> NaiveDate {
    (Utc::now() - Duration::days(days_ago)).date_naive()
}

fn count_on(days: &[(NaiveDate, i64)], day: NaiveDate) -> Option<i64> {
    days.iter()
        .find(|(d, _)| *d == day)
        .map(|(_, count)| *count)
}

/// A link with a click 5 days ago and one today used to chart two points side
/// by side: the days between were missing, not zero.
#[tokio::test]
async fn link_stats_keeps_days_without_clicks_as_zero() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id).await;
    insert_click(&db, link_id, 5, None, None).await;
    insert_click(&db, link_id, 0, None, None).await;
    insert_click(&db, link_id, 0, None, None).await;

    let body = link_stats(&server, &token, link_id, 7).await;
    let days = series(&body);

    assert_consecutive(&days);
    assert_eq!(
        days.first().map(|(d, _)| *d),
        Some(utc_day(7)),
        "the series opens on the day the window opens: {days:?}"
    );
    assert_eq!(
        days.last().map(|(d, _)| *d),
        Some(utc_day(0)),
        "and runs through today: {days:?}"
    );
    assert_eq!(days.len(), 8, "{days:?}");
    assert_eq!(count_on(&days, utc_day(5)), Some(1), "{days:?}");
    assert_eq!(count_on(&days, utc_day(4)), Some(0), "{days:?}");
    assert_eq!(count_on(&days, utc_day(1)), Some(0), "{days:?}");
    assert_eq!(count_on(&days, utc_day(0)), Some(2), "{days:?}");
    assert_eq!(
        days.iter().map(|(_, count)| count).sum::<i64>(),
        body["total_clicks"].as_i64().expect("total_clicks"),
        "filling days must not add or lose clicks"
    );
}

/// A link nobody has clicked is a flat line over the window, not a missing
/// chart.
#[tokio::test]
async fn link_stats_without_clicks_is_all_zero_days() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id).await;

    let body = link_stats(&server, &token, link_id, 3).await;
    let days = series(&body);

    assert_consecutive(&days);
    assert_eq!(days.len(), 4, "{days:?}");
    assert!(days.iter().all(|(_, count)| *count == 0), "{days:?}");
    assert_eq!(body["total_clicks"], 0);
}

/// When the cap cuts the window short, the series covers the days that were
/// loaded; days before the oldest loaded click are unknown, not zero.
#[tokio::test]
async fn a_truncated_sample_does_not_claim_the_older_days_were_quiet() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id).await;

    // 50,000 clicks now (the whole sample) and one older click the cap drops.
    db.execute_unprepared(&format!(
        "INSERT INTO click_events (link_id, created_at) \
         SELECT {link_id}, NOW() AT TIME ZONE 'UTC' FROM generate_series(1, 50000)"
    ))
    .await
    .expect("insert the sampled clicks");
    insert_click(&db, link_id, 10, None, None).await;

    let body = link_stats(&server, &token, link_id, 30).await;
    assert_eq!(body["truncated"], true, "{body}");
    assert_eq!(body["total_clicks"], 50_001);

    let days = series(&body);
    assert_eq!(days.len(), 1, "only today was loaded: {days:?}");
    assert_eq!(days[0].1, 50_000);
}

/// The answer counts the clicks it describes, even while clicks are being
/// written. The total came from a second query, so a click flush that
/// committed between that count and the read of the clicks themselves made the
/// total include clicks the series, the breakdowns and the recent clicks did
/// not have. Clients that wait for the total to reach a number (the analytics
/// e2e spec does) then read a chart without those clicks.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_total_counts_the_clicks_the_answer_describes_while_clicks_arrive() {
    // A hundred stats requests in a row: more than the per-second and
    // per-minute budgets allow one client.
    let (_, db) = spawn_real_app().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let roomy = || Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)));
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        ..RateLimiters::default()
    });
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("server");
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id).await;

    // One committed click after another for as long as the requests run.
    let writing = Arc::new(AtomicBool::new(true));
    let writer = {
        let (db, writing) = (db.clone(), writing.clone());
        tokio::spawn(async move {
            while writing.load(Ordering::Relaxed) {
                insert_click(&db, link_id, 0, None, None).await;
            }
        })
    };

    let mut mismatches = Vec::new();
    for _ in 0..100 {
        let body = link_stats(&server, &token, link_id, 7).await;
        assert_eq!(body["truncated"], false);
        let total = body["total_clicks"].as_i64().expect("total_clicks");
        let in_series: i64 = series(&body).iter().map(|(_, count)| count).sum();
        if total != in_series {
            mismatches.push((total, in_series));
        }
    }
    writing.store(false, Ordering::Relaxed);
    writer.await.expect("writer");

    assert!(
        mismatches.is_empty(),
        "answers whose total was not the clicks in their series (total, series): {mismatches:?}"
    );
}

/// Paris, France and Paris, Texas are two cities. They were merged under
/// whichever country the first click came from.
#[tokio::test]
async fn link_stats_counts_same_named_cities_in_different_countries_apart() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id).await;
    insert_click(&db, link_id, 0, Some("Paris"), Some("France")).await;
    insert_click(&db, link_id, 0, Some("Paris"), Some("France")).await;
    insert_click(&db, link_id, 0, Some("Paris"), Some("United States")).await;
    insert_click(&db, link_id, 0, Some("London"), Some("United Kingdom")).await;
    // No city resolved: one "Unknown" entry whatever the country.
    insert_click(&db, link_id, 0, None, Some("Germany")).await;
    insert_click(&db, link_id, 0, None, Some("Spain")).await;
    insert_click(&db, link_id, 0, None, None).await;

    let body = link_stats(&server, &token, link_id, 7).await;
    let mut cities: Vec<(String, Option<String>, i64)> = body["clicks_by_city"]
        .as_array()
        .expect("clicks_by_city")
        .iter()
        .map(|c| {
            (
                c["city"].as_str().expect("city").to_string(),
                c["country"].as_str().map(str::to_string),
                c["count"].as_i64().expect("count"),
            )
        })
        .collect();
    cities.sort();

    let entry = |city: &str, country: Option<&str>, count: i64| {
        (city.to_string(), country.map(str::to_string), count)
    };
    assert_eq!(
        cities,
        vec![
            entry("London", Some("United Kingdom"), 1),
            entry("Paris", Some("France"), 2),
            entry("Paris", Some("United States"), 1),
            entry("Unknown", None, 3),
        ]
    );

    let paris_france = body["clicks_by_city"]
        .as_array()
        .and_then(|all| all.iter().find(|c| c["country"] == "France"))
        .expect("Paris, France");
    assert!(
        (paris_france["percentage"].as_f64().expect("percentage") - 200.0 / 7.0).abs() < 0.001,
        "{paris_france}"
    );
}

/// The dashboard's 30-day series is also one entry per day.
#[tokio::test]
async fn dashboard_clicks_by_day_keeps_days_without_clicks_as_zero() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id).await;
    insert_click(&db, link_id, 20, None, None).await;
    insert_click(&db, link_id, 2, None, None).await;
    insert_click(&db, link_id, 2, None, None).await;
    // Outside the 30-day window.
    insert_click(&db, link_id, 40, None, None).await;

    let res = server
        .get("/analytics/dashboard")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "dashboard: {}", res.text());
    let days = series(&res.json());

    assert_consecutive(&days);
    assert_eq!(days.len(), 31, "{days:?}");
    assert_eq!(days.first().map(|(d, _)| *d), Some(utc_day(30)));
    assert_eq!(days.last().map(|(d, _)| *d), Some(utc_day(0)));
    assert_eq!(count_on(&days, utc_day(20)), Some(1), "{days:?}");
    assert_eq!(count_on(&days, utc_day(2)), Some(2), "{days:?}");
    assert_eq!(count_on(&days, utc_day(10)), Some(0), "{days:?}");
    assert_eq!(days.iter().map(|(_, count)| count).sum::<i64>(), 3);
}
