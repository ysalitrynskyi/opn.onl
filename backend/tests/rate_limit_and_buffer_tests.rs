//! Rate-limit path classification and click-buffer behaviour against the real
//! router and a real Postgres database.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::utils::click_buffer::ClickData;
use opn_onl_backend::utils::ClickBuffer;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
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

async fn create_link_id(server: &axum_test::TestServer, token: &str) -> i32 {
    let created = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": "https://iana.org/click-buffer" }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    created.json::<Value>()["id"].as_i64().expect("id") as i32
}

#[tokio::test]
async fn click_buffer_hard_cap_drops_events_past_max_queued() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let link_id = create_link_id(&server, &token).await;

    // Flush threshold 2, hard cap 5. Twenty clicks must not grow past 5.
    let buffer = ClickBuffer::with_limits(2, 5, 60);
    for _ in 0..20 {
        buffer.add_click(click(link_id));
    }
    assert_eq!(
        buffer.queued_event_count(),
        5,
        "queue must stop growing at the hard cap"
    );
    assert_eq!(
        buffer.pending_count(link_id),
        5,
        "aggregate counter must not count dropped clicks"
    );

    buffer.flush(&db).await;
    let persisted = opn_onl_backend::entity::click_events::Entity::find()
        .filter(opn_onl_backend::entity::click_events::Column::LinkId.eq(link_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(persisted.len(), 5, "only the capped events are flushed");
}

#[tokio::test]
async fn click_buffer_inserts_past_postgres_bind_limit_in_chunks() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let link_id = create_link_id(&server, &token).await;

    // Postgres rejects a statement with more than 65535 bind parameters.
    // One click row currently binds 12 columns, so ~5462 rows in one
    // insert_many fail forever even after the database recovers.
    const PAST_BIND_LIMIT: usize = 5500;
    let buffer = ClickBuffer::with_limits(PAST_BIND_LIMIT, PAST_BIND_LIMIT, 60);
    for _ in 0..PAST_BIND_LIMIT {
        buffer.add_click(click(link_id));
    }

    buffer.flush(&db).await;
    assert_eq!(
        buffer.queued_event_count(),
        0,
        "chunked insert must drain the queue rather than requeue a forever-failing batch"
    );

    let persisted = opn_onl_backend::entity::click_events::Entity::find()
        .filter(opn_onl_backend::entity::click_events::Column::LinkId.eq(link_id))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(
        persisted, PAST_BIND_LIMIT as u64,
        "every buffered click must persist across insert chunks"
    );
}
