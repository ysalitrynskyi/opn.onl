//! Regression coverage for link redirect/cache/tenant isolation hardening.
//! Uses the real router and a real PostgreSQL database.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{blocked_domains, click_events, links};
use opn_onl_backend::utils::click_buffer::ClickData;
use opn_onl_backend::utils::ClickBuffer;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{json, Value};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32) {
    let response = server
        .post("/auth/register")
        .json(&json!({
            "email": unique_email(),
            "password": "password123",
        }))
        .await;
    assert_eq!(response.status_code(), 201, "register: {}", response.text());
    let body: Value = response.json();
    let user_id = body["user_id"].as_i64().expect("user id") as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().expect("token").to_string(), user_id)
}

async fn create_link(server: &axum_test::TestServer, token: &str, payload: Value) -> (i32, String) {
    let response = server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(
        response.status_code(),
        201,
        "create link: {}",
        response.text()
    );
    let body: Value = response.json();
    (
        body["id"].as_i64().expect("link id") as i32,
        body["code"].as_str().expect("link code").to_string(),
    )
}

fn path_and_query(absolute_url: &str) -> String {
    let parsed = url::Url::parse(absolute_url).expect("absolute redirect URL");
    match parsed.query() {
        Some(query) => format!("{}?{}", parsed.path(), query),
        None => parsed.path().to_string(),
    }
}

#[tokio::test]
async fn password_verify_reenters_interstitial_routing_and_blocklist_pipeline() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;
    let routed_host = format!("{}.example", uuid::Uuid::new_v4());
    let routed_destination = format!("https://{routed_host}/routed-secret");
    let original_destination = "https://example.com/original-secret";

    let (link_id, code) = create_link(
        &server,
        &token,
        json!({
            "original_url": original_destination,
            "password": "correct-horse-battery-staple",
            "safe_link_interstitial": true,
        }),
    )
    .await;

    let rules = server
        .put(&format!("/links/{link_id}/rules"))
        .authorization_bearer(&token)
        .json(&json!({
            "rules": [{
                "destination_url": routed_destination,
                "priority": 0,
            }],
        }))
        .await;
    assert_eq!(
        rules.status_code(),
        200,
        "save routing rule: {}",
        rules.text()
    );

    let verified = server
        .post(&format!("/{code}/verify"))
        .json(&json!({ "password": "correct-horse-battery-staple" }))
        .await;
    assert_eq!(
        verified.status_code(),
        200,
        "password verify: {}",
        verified.text()
    );
    let verified_body: Value = verified.json();
    assert!(
        verified_body.get("url").is_none(),
        "verify must never return the raw destination: {verified_body}"
    );
    let redirect_url = verified_body["redirect_url"]
        .as_str()
        .expect("pipeline redirect URL");
    assert!(
        !redirect_url.contains("original-secret") && !redirect_url.contains("routed-secret"),
        "unlock response leaked a destination: {redirect_url}"
    );
    let unlock_path = path_and_query(redirect_url);

    let interstitial = server.get(&unlock_path).await;
    assert_eq!(
        interstitial.status_code(),
        307,
        "unlocked request should enter interstitial: {}",
        interstitial.text()
    );
    let interstitial_location = interstitial
        .headers()
        .get("location")
        .expect("interstitial location")
        .to_str()
        .unwrap();
    assert!(
        interstitial_location.contains(&format!("/r/{code}"))
            && interstitial_location.contains("unlock="),
        "unlock must survive the interstitial hop: {interstitial_location}"
    );

    let separator = if unlock_path.contains('?') { '&' } else { '?' };
    let confirmed_path = format!("{unlock_path}{separator}confirm=1");
    let routed = server.get(&confirmed_path).await;
    assert_eq!(
        routed.status_code(),
        307,
        "routed redirect: {}",
        routed.text()
    );
    assert_eq!(
        routed.headers().get("location").unwrap().to_str().unwrap(),
        routed_destination
    );
    assert_eq!(
        routed
            .headers()
            .get("referrer-policy")
            .unwrap()
            .to_str()
            .unwrap(),
        "no-referrer"
    );

    blocked_domains::ActiveModel {
        domain: Set(routed_host),
        reason: Set(Some("regression test".to_string())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("block routed domain");

    let blocked = server.get(&confirmed_path).await;
    assert_eq!(
        blocked.status_code(),
        410,
        "an unlock token must not bypass a later blocklist decision: {}",
        blocked.text()
    );
}

#[tokio::test]
async fn create_and_bulk_operations_reject_cross_tenant_folder_and_tags() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&server, &db).await;
    let (attacker_token, _) = register_verified(&server, &db).await;

    let folder = server
        .post("/folders")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "owner-folder" }))
        .await;
    assert_eq!(
        folder.status_code(),
        201,
        "create folder: {}",
        folder.text()
    );
    let folder_id = folder.json::<Value>()["id"].as_i64().unwrap();

    let tag = server
        .post("/tags")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "owner-tag" }))
        .await;
    assert_eq!(tag.status_code(), 201, "create tag: {}", tag.text());
    let tag_id = tag.json::<Value>()["id"].as_i64().unwrap();

    let foreign_folder = server
        .post("/links")
        .authorization_bearer(&attacker_token)
        .json(&json!({
            "original_url": "https://example.com/foreign-folder",
            "folder_id": folder_id,
        }))
        .await;
    assert_eq!(
        foreign_folder.status_code(),
        403,
        "cross-tenant folder assignment must fail: {}",
        foreign_folder.text()
    );

    let foreign_tag = server
        .post("/links")
        .authorization_bearer(&attacker_token)
        .json(&json!({
            "original_url": "https://example.com/foreign-tag",
            "tag_ids": [tag_id],
        }))
        .await;
    assert_eq!(
        foreign_tag.status_code(),
        403,
        "cross-tenant tag assignment must fail: {}",
        foreign_tag.text()
    );

    let bulk = server
        .post("/links/bulk")
        .authorization_bearer(&attacker_token)
        .json(&json!({
            "urls": ["https://example.com/foreign-bulk-folder"],
            "folder_id": folder_id,
        }))
        .await;
    assert_eq!(bulk.status_code(), 200, "bulk response: {}", bulk.text());
    let bulk_body: Value = bulk.json();
    assert_eq!(bulk_body["links"].as_array().unwrap().len(), 0);
    assert!(
        !bulk_body["errors"].as_array().unwrap().is_empty(),
        "bulk create must report the rejected foreign folder"
    );

    let (attacker_link_id, _) = create_link(
        &server,
        &attacker_token,
        json!({ "original_url": "https://example.com/attacker-link" }),
    )
    .await;
    let bulk_update = server
        .post("/links/bulk/update")
        .authorization_bearer(&attacker_token)
        .json(&json!({
            "ids": [attacker_link_id],
            "folder_id": folder_id,
        }))
        .await;
    assert_eq!(
        bulk_update.status_code(),
        200,
        "bulk update response: {}",
        bulk_update.text()
    );
    assert_eq!(bulk_update.json::<Value>()["updated"], 0);

    let stored = links::Entity::find_by_id(attacker_link_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.folder_id, None, "foreign folder must not persist");
}

fn click(link_id: i32) -> ClickData {
    ClickData {
        link_id,
        ip_address: None,
        user_agent: None,
        referer: None,
        country: None,
        city: None,
        region: None,
        latitude: None,
        longitude: None,
        device: None,
        browser: None,
        os: None,
    }
}

#[tokio::test]
async fn click_flush_isolates_orphan_without_dropping_valid_link_batch() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;
    let (link_id, _) = create_link(
        &server,
        &token,
        json!({ "original_url": "https://example.com/click-buffer" }),
    )
    .await;

    let buffer = ClickBuffer::new();
    buffer.add_click(click(link_id));
    buffer.add_click(click(i32::MAX));
    buffer.flush(&db).await;

    let persisted_events = click_events::Entity::find()
        .filter(click_events::Column::LinkId.eq(link_id))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(
        persisted_events, 1,
        "an orphan FK must not discard the valid event"
    );

    let stored = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.click_count, 1,
        "valid aggregate count must survive an orphan in the same flush"
    );
}
