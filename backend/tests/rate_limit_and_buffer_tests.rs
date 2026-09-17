//! Rate-limit path classification and click-buffer behaviour against the real
//! router and a real Postgres database.

mod common;

use common::{mark_email_verified, setup_test_db, spawn_real_app, unique_email};
use opn_onl_backend::utils::click_buffer::ClickData;
use opn_onl_backend::utils::ClickBuffer;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter,
    Statement, TransactionTrait,
};
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

#[tokio::test]
async fn post_clone_consumes_link_creation_budget_but_pin_does_not() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/clone-limit" }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    assert_eq!(
        rate_limit_remaining(&created),
        99,
        "create spends one slot of the hourly bucket"
    );
    let id = created.json::<Value>()["id"].as_i64().expect("id");

    let cloned = server
        .post(&format!("/links/{id}/clone"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(cloned.status_code(), 201, "clone: {}", cloned.text());
    assert_eq!(
        rate_limit_remaining(&cloned),
        98,
        "clone creates a link and must spend the hourly create budget"
    );

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

/// `POST /links/bulk` used to spend 1 middleware token plus 1 per URL in the
/// handler, so a 100/hour budget admitted at most 99 URLs. Charge only in the
/// handler: N URLs spend N tokens, matching `POST /links`.
#[tokio::test]
async fn bulk_create_charges_one_create_token_per_url() {
    use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
    use std::sync::Arc;

    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let mut limiters = RateLimiters::new();
    limiters.link_creation = Arc::new(RateLimiter::new(RateLimitConfig::new(3, 3600)));
    state.rate_limiters = Arc::new(limiters);
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("test server");

    let token = register_verified(&server, &db).await;
    let res = server
        .post("/links/bulk")
        .authorization_bearer(&token)
        .json(&json!({
            "urls": [
                "https://iana.org/bulk-a",
                "https://iana.org/bulk-b",
                "https://iana.org/bulk-c",
            ]
        }))
        .await;
    assert_eq!(res.status_code(), 200, "bulk: {}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["links"].as_array().map(|a| a.len()),
        Some(3),
        "a 3-token create budget must admit 3 bulk URLs: {body}"
    );
    assert_eq!(
        body["errors"].as_array().map(|a| a.len()),
        Some(0),
        "no URL should be rate-limited: {body}"
    );
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
        created_at: None,
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

#[tokio::test]
async fn click_buffer_flush_error_does_not_busy_loop() {
    let mut opts = sea_orm::ConnectOptions::new(
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
    );
    opts.acquire_timeout(std::time::Duration::from_millis(50));
    opts.connect_timeout(std::time::Duration::from_secs(2));
    let fail_db = sea_orm::Database::connect(opts).await.expect("connect");
    fail_db.close_by_ref().await.expect("close pool");

    let buffer = std::sync::Arc::new(ClickBuffer::with_limits(1, 20, 60));
    for _ in 0..5 {
        buffer.add_click(click(1));
    }
    let handle = buffer.clone().start_flush_task(fail_db);
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    let attempts = buffer.flush_attempt_count();
    handle.abort();
    let _ = handle.await;
    assert!(
        attempts <= 2,
        "flush failure must back off rather than notify-spin, got {attempts}"
    );
    assert_eq!(
        buffer.queued_event_count(),
        5,
        "failed flush must requeue rather than drop"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn click_buffer_shutdown_joins_in_flight_flush() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let link_id = create_link_id(&server, &token).await;

    // Hold FOR UPDATE on this link so the in-flight flush blocks at its
    // parent lock_shared, after taking events out of the queue. Row-level
    // so parallel tests on other links are unaffected.
    let lock_db = sea_orm::Database::connect(
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
    )
    .await
    .expect("lock conn");
    let txn = lock_db.begin().await.expect("begin");
    txn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT id FROM links WHERE id = $1 FOR UPDATE",
        [link_id.into()],
    ))
    .await
    .expect("lock link");

    let buffer = std::sync::Arc::new(ClickBuffer::with_limits(1, 20, 60));
    for _ in 0..5 {
        buffer.add_click(click(link_id));
    }
    let handle = buffer.clone().start_flush_task(db.clone());

    let started = std::time::Instant::now();
    while buffer.queued_event_count() != 0 {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "background flush never took the queued events"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let shutting_down = {
        let buffer = buffer.clone();
        let db = db.clone();
        tokio::spawn(async move {
            buffer.request_stop();
            let _ = handle.await;
            buffer.flush(&db).await;
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    txn.rollback().await.expect("release link lock");
    tokio::time::timeout(std::time::Duration::from_secs(5), shutting_down)
        .await
        .expect("shutdown timed out")
        .expect("shutdown task");

    assert_eq!(buffer.queued_event_count(), 0);
    let persisted = opn_onl_backend::entity::click_events::Entity::find()
        .filter(opn_onl_backend::entity::click_events::Column::LinkId.eq(link_id))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(
        persisted, 5,
        "in-flight flush must persist rather than be dropped on shutdown"
    );
}

#[tokio::test]
async fn click_buffer_persists_click_time_not_flush_time() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let link_id = create_link_id(&server, &token).await;

    let clicked_at = chrono::Utc::now().naive_utc() - chrono::Duration::minutes(10);
    let mut data = click(link_id);
    data.created_at = Some(clicked_at);

    let buffer = ClickBuffer::with_limits(10, 10, 60);
    buffer.add_click(data);
    buffer.flush(&db).await;

    let stored = opn_onl_backend::entity::click_events::Entity::find()
        .filter(opn_onl_backend::entity::click_events::Column::LinkId.eq(link_id))
        .one(&db)
        .await
        .unwrap()
        .expect("click row");
    let delta = (stored.created_at - clicked_at).num_seconds().abs();
    assert!(
        delta <= 1,
        "created_at must be the click time ({clicked_at}), not flush time, got {}",
        stored.created_at
    );
    let lag = (chrono::Utc::now().naive_utc() - stored.created_at).num_minutes();
    assert!(
        lag >= 9,
        "stored created_at should be ~10 minutes ago, lag={lag} min"
    );
}
