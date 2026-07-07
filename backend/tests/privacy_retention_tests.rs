//! Analytics-PII lifecycle tests: the retention sweep must anonymize
//! per-visitor identifiers (ip_address, user_agent) on old click events while
//! leaving fresh events and aggregate analytics columns untouched.

mod common;

use opn_onl_backend::utils::privacy::scrub_expired_click_pii;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde_json::json;

#[tokio::test]
async fn retention_sweep_anonymizes_only_expired_click_identifiers() {
    let (server, db) = common::spawn_real_app().await;

    // Real user + link through the actual handlers (satisfies FKs).
    let email = common::unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register failed: {}", res.text());
    let user_id = res.json::<serde_json::Value>()["user_id"].as_i64().unwrap() as i32;
    common::mark_email_verified(&db, user_id).await;

    let token = res.json::<serde_json::Value>()["token"].as_str().unwrap().to_string();
    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://example.com/retention" }))
        .await;
    assert!(res.status_code().is_success(), "create link failed: {}", res.text());
    let link_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

    // One click event past the retention window, one fresh.
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO click_events (link_id, created_at, ip_address, user_agent, referer, country, city) VALUES \
         ($1, NOW() - make_interval(days => 400), '203.0.113.0', 'old-agent', 'https://old.example', 'US', 'New York'), \
         ($1, NOW(), '198.51.100.0', 'new-agent', 'https://new.example', 'DE', 'Berlin')",
        [(link_id as i32).into()],
    ))
    .await
    .expect("failed to insert click fixtures");

    let affected = scrub_expired_click_pii(&db, 396).await.expect("sweep failed");
    assert_eq!(affected, 1, "exactly the expired event should be anonymized");

    let rows = db
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT ip_address, user_agent, country, city FROM click_events \
             WHERE link_id = $1 ORDER BY created_at ASC",
            [(link_id as i32).into()],
        ))
        .await
        .expect("failed to read back click events");
    assert_eq!(rows.len(), 2);

    // Expired event: identifiers gone, aggregate analytics preserved.
    let old_ip: Option<String> = rows[0].try_get("", "ip_address").unwrap();
    let old_ua: Option<String> = rows[0].try_get("", "user_agent").unwrap();
    let old_country: Option<String> = rows[0].try_get("", "country").unwrap();
    let old_city: Option<String> = rows[0].try_get("", "city").unwrap();
    assert_eq!(old_ip, None);
    assert_eq!(old_ua, None);
    assert_eq!(old_country.as_deref(), Some("US"));
    assert_eq!(old_city.as_deref(), Some("New York"));

    // Fresh event: fully intact.
    let new_ip: Option<String> = rows[1].try_get("", "ip_address").unwrap();
    let new_ua: Option<String> = rows[1].try_get("", "user_agent").unwrap();
    assert_eq!(new_ip.as_deref(), Some("198.51.100.0"));
    assert_eq!(new_ua.as_deref(), Some("new-agent"));

    // Idempotent: nothing new to anonymize on a second run.
    let affected = scrub_expired_click_pii(&db, 396).await.expect("second sweep failed");
    assert_eq!(affected, 0);
}
