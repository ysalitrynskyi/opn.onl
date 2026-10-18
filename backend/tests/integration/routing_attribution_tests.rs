//! Clicks by destination. A routed redirect records the URL it sent the
//! visitor to, and `GET /links/{id}/stats` reports `clicks_by_destination`, so
//! an A/B split or a routing rule has a result. Routing rule ids change on
//! every save, so the URL itself is what a click keeps.

use crate::common;

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::header;
use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::click_events;
use opn_onl_backend::utils::ClickBuffer;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

const IPHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const LINK_URL: &str = "https://iana.org/attribution-link";
const MOBILE: &str = "https://iana.org/attribution-mobile";
const GERMAN: &str = "https://iana.org/attribution-de";
const VARIANT_A: &str = "https://iana.org/attribution-a";
const VARIANT_B: &str = "https://iana.org/attribution-b";

/// The real router and the click buffer behind it, so a test flushes the
/// clicks it made instead of waiting for a background task.
async fn spawn_app() -> (axum_test::TestServer, DatabaseConnection, Arc<ClickBuffer>) {
    // Pins the environment and migrates the database; its server is unused.
    let (_, db) = spawn_real_app().await;
    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let clicks = state.click_buffer.clone();
    let server =
        axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db, clicks)
}

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

/// `(id, code)` of a fresh link to `LINK_URL`.
async fn create_link(server: &axum_test::TestServer, token: &str) -> (i32, String) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": LINK_URL, "custom_alias": unique_code() }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

async fn put_rules(server: &axum_test::TestServer, token: &str, id: i32, rules: Value) {
    let res = server
        .put(&format!("/links/{id}/rules"))
        .authorization_bearer(token)
        .json(&json!({ "rules": rules }))
        .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());
}

/// Follow the short link as a visitor and return where it sent them.
async fn visit(
    server: &axum_test::TestServer,
    path: &str,
    user_agent: &str,
    accept_language: Option<&str>,
) -> String {
    let mut req = server.get(path).add_header(header::USER_AGENT, user_agent);
    if let Some(language) = accept_language {
        req = req.add_header(header::ACCEPT_LANGUAGE, language);
    }
    let res = req.await;
    assert_eq!(res.status_code(), 307, "redirect: {}", res.text());
    res.header(header::LOCATION)
        .to_str()
        .expect("utf-8 location")
        .to_string()
}

async fn flush(clicks: &ClickBuffer, db: &DatabaseConnection) {
    assert!(
        !clicks.flush(db).await,
        "the clicks must be persisted, not requeued"
    );
}

async fn stored_clicks(db: &DatabaseConnection, link_id: i32) -> Vec<click_events::Model> {
    click_events::Entity::find()
        .filter(click_events::Column::LinkId.eq(link_id))
        .all(db)
        .await
        .expect("click rows")
}

async fn destinations(
    server: &axum_test::TestServer,
    token: &str,
    link_id: i32,
    days: Option<&str>,
) -> Vec<(String, i64)> {
    let mut req = server
        .get(&format!("/links/{link_id}/stats"))
        .authorization_bearer(token);
    if let Some(days) = days {
        req = req.add_query_param("days", days);
    }
    let res = req.await;
    assert_eq!(res.status_code(), 200, "stats: {}", res.text());
    let stats: Value = res.json();
    stats["clicks_by_destination"]
        .as_array()
        .unwrap_or_else(|| panic!("clicks_by_destination in {stats}"))
        .iter()
        .map(|row| {
            (
                row["destination"]
                    .as_str()
                    .expect("destination")
                    .to_string(),
                row["clicks"].as_i64().expect("clicks"),
            )
        })
        .collect()
}

fn owned(rows: &[(&str, i64)]) -> Vec<(String, i64)> {
    rows.iter()
        .map(|(destination, clicks)| (destination.to_string(), *clicks))
        .collect()
}

/// Each click is counted under the rule that routed it, and a click no rule
/// matched under the link's own URL.
#[tokio::test]
async fn conditional_rules_attribute_each_click_to_its_destination() {
    let (server, db, clicks) = spawn_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;
    put_rules(
        &server,
        &token,
        id,
        json!([
            { "match_device": "Mobile", "destination_url": MOBILE },
            { "match_lang": "de", "destination_url": GERMAN },
        ]),
    )
    .await;

    let path = format!("/{code}");
    for _ in 0..3 {
        assert_eq!(visit(&server, &path, IPHONE_UA, None).await, MOBILE);
    }
    for _ in 0..2 {
        assert_eq!(
            visit(&server, &path, DESKTOP_UA, Some("de-DE,de;q=0.9")).await,
            GERMAN
        );
    }
    assert_eq!(
        visit(&server, &path, DESKTOP_UA, Some("en-US")).await,
        LINK_URL
    );
    flush(&clicks, &db).await;

    let mut recorded: HashMap<Option<String>, i64> = HashMap::new();
    for click in stored_clicks(&db, id).await {
        *recorded.entry(click.destination).or_insert(0) += 1;
    }
    assert_eq!(
        recorded,
        HashMap::from([
            (Some(MOBILE.to_string()), 3),
            (Some(GERMAN.to_string()), 2),
            // Routed, so the fall-through is recorded too.
            (Some(LINK_URL.to_string()), 1),
        ])
    );

    assert_eq!(
        destinations(&server, &token, id, None).await,
        owned(&[(MOBILE, 3), (GERMAN, 2), (LINK_URL, 1)])
    );
}

/// A weighted A/B split: the stats say exactly where the visitors were sent.
#[tokio::test]
async fn ab_split_counts_match_where_visitors_were_sent() {
    let (server, db, clicks) = spawn_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;
    put_rules(
        &server,
        &token,
        id,
        json!([
            { "destination_url": VARIANT_A, "weight": 2, "priority": 0 },
            { "destination_url": VARIANT_B, "weight": 1, "priority": 0 },
        ]),
    )
    .await;

    let path = format!("/{code}");
    let mut sent: HashMap<String, i64> = HashMap::new();
    for _ in 0..60 {
        *sent
            .entry(visit(&server, &path, DESKTOP_UA, None).await)
            .or_insert(0) += 1;
    }
    // 60 draws all landing on one side: odds below 2^-35.
    assert_eq!(sent.len(), 2, "both variants must be served: {sent:?}");
    flush(&clicks, &db).await;

    let reported: HashMap<String, i64> = destinations(&server, &token, id, None)
        .await
        .into_iter()
        .collect();
    assert_eq!(reported, sent);
}

/// Without routing rules every click goes to the link's URL. Nothing is
/// stored for it, and the stats report that one destination.
#[tokio::test]
async fn a_link_without_rules_reports_one_destination() {
    let (server, db, clicks) = spawn_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;

    for path in [format!("/{code}"), format!("/{code}?utm_source=visitor")] {
        assert_eq!(visit(&server, &path, DESKTOP_UA, None).await, LINK_URL);
    }
    flush(&clicks, &db).await;

    let rows = stored_clicks(&db, id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows.iter().all(|click| click.destination.is_none()),
        "a link without rules stores no destination: {rows:?}"
    );
    assert_eq!(
        destinations(&server, &token, id, None).await,
        owned(&[(LINK_URL, 2)])
    );
}

/// The destination is the owner's configured URL, query string and all. What
/// the visitor appended to the short link is neither forwarded nor stored.
#[tokio::test]
async fn the_visitors_query_string_is_never_stored() {
    const OWNER_URL: &str = "https://iana.org/attribution-owner?variant=b";

    let (server, db, clicks) = spawn_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;
    put_rules(
        &server,
        &token,
        id,
        json!([{ "destination_url": OWNER_URL }]),
    )
    .await;

    let sent = visit(
        &server,
        &format!("/{code}?utm_source=newsletter&email=visitor%40example.com"),
        DESKTOP_UA,
        None,
    )
    .await;
    assert_eq!(sent, OWNER_URL);
    flush(&clicks, &db).await;

    let rows = stored_clicks(&db, id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].destination.as_deref(), Some(OWNER_URL), "{rows:?}");
    let stored = format!("{rows:?}");
    assert!(
        !stored.contains("newsletter") && !stored.contains("visitor"),
        "the visitor's query string reached click_events: {stored}"
    );
}

/// A click stored without a destination counts toward the link's current URL.
/// A routed click keeps the URL it was sent to, even when that was the link's
/// own URL and the owner changes it afterwards.
#[tokio::test]
async fn clicks_without_a_destination_follow_the_link_url() {
    const NEW_URL: &str = "https://iana.org/attribution-moved";

    let (server, db, clicks) = spawn_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;
    let path = format!("/{code}");

    assert_eq!(visit(&server, &path, DESKTOP_UA, None).await, LINK_URL);
    put_rules(
        &server,
        &token,
        id,
        json!([{ "match_device": "Mobile", "destination_url": MOBILE }]),
    )
    .await;
    assert_eq!(visit(&server, &path, DESKTOP_UA, None).await, LINK_URL);
    assert_eq!(visit(&server, &path, IPHONE_UA, None).await, MOBILE);
    flush(&clicks, &db).await;

    let res = server
        .put(&format!("/links/{id}"))
        .authorization_bearer(&token)
        .json(&json!({ "original_url": NEW_URL }))
        .await;
    assert_eq!(res.status_code(), 200, "update link: {}", res.text());

    assert_eq!(
        destinations(&server, &token, id, None).await,
        owned(&[(LINK_URL, 1), (MOBILE, 1), (NEW_URL, 1)])
    );
}

/// The breakdown covers the requested window, like every other breakdown.
#[tokio::test]
async fn destinations_follow_the_stats_window() {
    const OLD: &str = "https://iana.org/attribution-old";

    let (server, db, _clicks) = spawn_app().await;
    let token = register_verified(&server, &db).await;
    let (id, _code) = create_link(&server, &token).await;
    for (days_ago, destination) in [(40, Some(OLD)), (1, Some(MOBILE)), (1, None)] {
        click_events::ActiveModel {
            link_id: Set(id),
            created_at: Set(Utc::now().naive_utc() - Duration::days(days_ago)),
            destination: Set(destination.map(str::to_string)),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert click event");
    }

    assert_eq!(
        destinations(&server, &token, id, None).await,
        owned(&[(LINK_URL, 1), (MOBILE, 1)])
    );
    assert_eq!(
        destinations(&server, &token, id, Some("60")).await,
        owned(&[(LINK_URL, 1), (MOBILE, 1), (OLD, 1)])
    );
}

/// Generated clients read the breakdown from the published document.
#[tokio::test]
async fn clicks_by_destination_is_documented() {
    let (server, _db, _clicks) = spawn_app().await;
    let res = server.get("/api-docs/openapi.json").await;
    assert_eq!(res.status_code(), 200);
    let spec: Value = res.json();
    let schemas = &spec["components"]["schemas"];

    let field = &schemas["LinkStatsResponse"]["properties"]["clicks_by_destination"];
    assert_eq!(
        field["items"]["$ref"], "#/components/schemas/DestinationStats",
        "{field}"
    );
    let row = &schemas["DestinationStats"];
    for name in ["destination", "clicks"] {
        assert!(row["properties"].get(name).is_some(), "{row}");
    }
}
