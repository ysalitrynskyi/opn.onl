//! Regression tests for the privacy findings in docs/CODE_AUDIT_2026-07.md.
//! Real router + real Postgres via `common::spawn_real_app`.
//!
//! - Right to erasure: account deletion purges per-visitor click PII.
//! - Referer host-truncation is unit-tested in
//!   `utils::privacy::tests::referer_reduced_to_host_only`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::click_events;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde_json::{json, Value};

/// The account-deletion erasure step must null per-visitor identifiers
/// (ip/user-agent/referer) on the user's link click events while keeping the
/// aggregate dimensions (country, …).
#[tokio::test]
async fn purge_click_pii_erases_identifiers_keeps_aggregates() {
    let (server, db) = spawn_real_app().await;

    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let token = body["token"].as_str().unwrap().to_string();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(&db, user_id).await;

    let link = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://example.com/x" }))
        .await;
    assert_eq!(link.status_code(), 201, "create link: {}", link.text());
    let link_id = link.json::<Value>()["id"].as_i64().unwrap() as i32;

    let click = click_events::ActiveModel {
        link_id: Set(link_id),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ip_address: Set(Some("203.0.113.0".to_string())),
        user_agent: Set(Some("Mozilla/5.0 test".to_string())),
        referer: Set(Some("example.org".to_string())),
        country: Set(Some("US".to_string())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert click event");

    let purged = opn_onl_backend::utils::privacy::purge_click_pii_for_user(&db, user_id)
        .await
        .expect("purge");
    assert!(
        purged >= 1,
        "expected to purge >= 1 click event, got {purged}"
    );

    let after = click_events::Entity::find_by_id(click.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        after.ip_address.is_none() && after.user_agent.is_none() && after.referer.is_none(),
        "visitor PII must be nulled after erasure"
    );
    assert_eq!(
        after.country.as_deref(),
        Some("US"),
        "aggregate dimensions must be preserved"
    );
}
