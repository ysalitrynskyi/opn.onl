//! Behaviour that matters once an instance is large: sweeps work in short
//! batches, a failed lookup is not reported as a missing link, the dashboard
//! does not load whole link rows, and the link list has an index in its order.
//! Real router + real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::utils::account_purge::{
    delete_personal_link_clicks, purge_expired_deleted_links,
};
use opn_onl_backend::utils::privacy::{
    purge_click_pii_for_user_in_batches, scrub_deleted_users_click_pii,
    scrub_expired_click_pii_in_batches,
};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait};
use serde_json::{Value, json};

async fn register(server: &axum_test::TestServer, db: &DatabaseConnection) -> (String, i32) {
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

async fn create_link(server: &axum_test::TestServer, token: &str, extra: Value) -> (i32, String) {
    let mut body = json!({
        "original_url": format!("https://iana.org/scale-{}", unique_code()),
    });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        body.extend(extra.clone());
    }
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let link: Value = res.json();
    (
        link["id"].as_i64().unwrap() as i32,
        link["code"].as_str().unwrap().to_string(),
    )
}

/// `count` click events on `link_id`, `days_ago` old, each with an IP, a user
/// agent and a referer.
async fn insert_clicks(db: &DatabaseConnection, link_id: i32, count: i32, days_ago: i32) {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO click_events (link_id, created_at, ip_address, user_agent, referer, country) \
         SELECT $1, NOW() - make_interval(days => $3), '203.0.113.0', 'agent', 'example.com', 'US' \
         FROM generate_series(1, $2)",
        [link_id.into(), count.into(), days_ago.into()],
    ))
    .await
    .expect("insert clicks");
}

async fn count(db: &DatabaseConnection, sql: &str, value: i32) -> i64 {
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [value.into()],
        ))
        .await
        .expect("count query")
        .expect("one row");
    row.try_get_by_index::<i64>(0).expect("count")
}

#[tokio::test]
async fn retention_sweep_runs_in_batches_and_keeps_fresh_clicks() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register(&server, &db).await;
    let (link_id, _) = create_link(&server, &token, json!({})).await;
    insert_clicks(&db, link_id, 7, 400).await;
    insert_clicks(&db, link_id, 2, 1).await;

    let stats = scrub_expired_click_pii_in_batches(&db, 396, 3)
        .await
        .expect("sweep");
    // Other tests share the table, so only lower bounds are certain: seven
    // rows of this link in batches of three need at least three statements.
    assert!(stats.rows >= 7, "{stats:?}");
    assert!(stats.batches >= 3, "one statement per batch: {stats:?}");

    let old_with_pii = count(
        &db,
        "SELECT COUNT(*) FROM click_events WHERE link_id = $1 \
         AND created_at < NOW() - INTERVAL '396 days' \
         AND (ip_address IS NOT NULL OR user_agent IS NOT NULL)",
        link_id,
    )
    .await;
    assert_eq!(old_with_pii, 0, "expired identifiers are gone");
    let fresh_with_pii = count(
        &db,
        "SELECT COUNT(*) FROM click_events WHERE link_id = $1 \
         AND created_at > NOW() - INTERVAL '2 days' AND ip_address IS NOT NULL",
        link_id,
    )
    .await;
    assert_eq!(fresh_with_pii, 2, "clicks inside the window keep theirs");
}

#[tokio::test]
async fn account_click_scrub_runs_in_batches_on_personal_links_only() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register(&server, &db).await;
    let (link_id, _) = create_link(&server, &token, json!({})).await;
    insert_clicks(&db, link_id, 7, 1).await;

    let stats = purge_click_pii_for_user_in_batches(&db, user_id, 3)
        .await
        .expect("scrub");
    assert_eq!(stats.rows, 7);
    assert_eq!(stats.batches, 3, "3 + 3 + 1");
    let left = count(
        &db,
        "SELECT COUNT(*) FROM click_events WHERE link_id = $1 \
         AND (ip_address IS NOT NULL OR user_agent IS NOT NULL OR referer IS NOT NULL)",
        link_id,
    )
    .await;
    assert_eq!(left, 0);
}

#[tokio::test]
async fn daily_task_scrubs_personal_clicks_of_deleted_accounts() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register(&server, &db).await;
    let (link_id, _) = create_link(&server, &token, json!({})).await;
    insert_clicks(&db, link_id, 3, 1).await;
    // Soft-deleted, with the post-commit scrub never having run (a crash).
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE users SET deleted_at = NOW() WHERE id = $1",
        [user_id.into()],
    ))
    .await
    .expect("soft delete");

    scrub_deleted_users_click_pii(&db, 2)
        .await
        .expect("scrub deleted accounts");
    let left = count(
        &db,
        "SELECT COUNT(*) FROM click_events WHERE link_id = $1 AND ip_address IS NOT NULL",
        link_id,
    )
    .await;
    assert_eq!(left, 0, "the deleted account's visitors are anonymized");
}

#[tokio::test]
async fn self_delete_scrubs_click_identifiers_after_the_commit() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (server, db) = spawn_real_app().await;
    let (token, _) = register(&server, &db).await;
    let (link_id, _) = create_link(&server, &token, json!({})).await;
    insert_clicks(&db, link_id, 4, 1).await;

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "delete account: {}", res.text());
    let left = count(
        &db,
        "SELECT COUNT(*) FROM click_events WHERE link_id = $1 \
         AND (ip_address IS NOT NULL OR user_agent IS NOT NULL OR referer IS NOT NULL)",
        link_id,
    )
    .await;
    assert_eq!(left, 0);
}

#[tokio::test]
async fn personal_link_clicks_are_deleted_ahead_of_the_rows() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register(&server, &db).await;
    let (link_id, _) = create_link(&server, &token, json!({})).await;
    insert_clicks(&db, link_id, 7, 1).await;

    let deleted = delete_personal_link_clicks(&db, user_id, 3)
        .await
        .expect("delete clicks");
    assert_eq!(deleted, 7);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM click_events WHERE link_id = $1",
            link_id
        )
        .await,
        0
    );
}

#[tokio::test]
async fn deleted_link_purge_removes_its_clicks_and_retires_the_code() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register(&server, &db).await;
    let (link_id, code) = create_link(&server, &token, json!({})).await;
    insert_clicks(&db, link_id, 5, 50).await;
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE links SET deleted_at = NOW() - INTERVAL '40 days' WHERE id = $1",
        [link_id.into()],
    ))
    .await
    .expect("expire link");

    purge_expired_deleted_links(&db, 30).await.expect("purge");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM links WHERE id = $1", link_id).await,
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM click_events WHERE link_id = $1",
            link_id
        )
        .await,
        0
    );
    let retired = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*) FROM retired_codes WHERE code = $1",
            [code.into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap();
    assert_eq!(retired, 1, "the code stays reserved");
}

#[tokio::test]
async fn a_failed_link_lookup_is_503_not_link_not_found() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register(&server, &db).await;
    let (_, code) = create_link(&server, &token, json!({})).await;
    assert_eq!(server.get(&format!("/{code}")).await.status_code(), 307);

    // Same router over a pool that is already closed: every query fails.
    let mut opts =
        sea_orm::ConnectOptions::new(std::env::var("DATABASE_URL").expect("DATABASE_URL"));
    opts.max_connections(1);
    opts.acquire_timeout(std::time::Duration::from_secs(5));
    let dead = sea_orm::Database::connect(opts).await.expect("connect");
    dead.close_by_ref().await.expect("close");
    let state = opn_onl_backend::AppState::for_tests(dead).await;
    let broken = axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("server");

    let res = broken.get(&format!("/{code}")).await;
    assert_eq!(res.status_code(), 503, "{}", res.text());
    assert!(!res.text().contains("not found"), "{}", res.text());
}

#[tokio::test]
async fn dashboard_counts_from_columns_and_picks_top_links_in_sql() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register(&server, &db).await;
    let mut ids = Vec::new();
    for _ in 0..12 {
        ids.push(create_link(&server, &token, json!({})).await.0);
    }
    // Distinct counts so the order is certain; one link expired, one used up.
    for (i, id) in ids.iter().enumerate() {
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE links SET click_count = $2 WHERE id = $1",
            [(*id).into(), (i as i64 * 10).into()],
        ))
        .await
        .unwrap();
    }
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE links SET expires_at = NOW() - INTERVAL '1 day' WHERE id = $1",
        [ids[0].into()],
    ))
    .await
    .unwrap();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE links SET max_clicks = 5 WHERE id = $1",
        [ids[1].into()],
    ))
    .await
    .unwrap();

    let res = server
        .get("/analytics/dashboard")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(body["total_links"], 12);
    assert_eq!(body["total_clicks"], (0..12).map(|i| i * 10).sum::<i64>());
    assert_eq!(
        body["active_links"], 10,
        "expired and used-up links are not active"
    );
    let top: Vec<i64> = body["top_links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|link| link["id"].as_i64().unwrap())
        .collect();
    let expected: Vec<i64> = ids.iter().rev().take(10).map(|id| i64::from(*id)).collect();
    assert_eq!(top, expected, "ten busiest first");
}

#[tokio::test]
async fn link_list_order_has_an_index() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register(&server, &db).await;
    for _ in 0..3 {
        create_link(&server, &token, json!({})).await;
    }
    for name in [
        "idx_links_user_list_order_live",
        "idx_links_org_list_order_live",
    ] {
        assert_eq!(
            db.query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT COUNT(*) FROM pg_indexes WHERE indexname = $1",
                [name.into()],
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index::<i64>(0)
            .unwrap(),
            1,
            "{name}"
        );
    }

    // A tiny table makes a sequential scan and a sort cheaper, so take those
    // off the table and check the plan reads the index in list order.
    let txn = db.begin().await.unwrap();
    txn.execute_unprepared("SET LOCAL enable_seqscan = off; SET LOCAL enable_sort = off")
        .await
        .unwrap();
    let plan = txn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "EXPLAIN SELECT id FROM links \
             WHERE user_id = $1 AND deleted_at IS NULL \
             ORDER BY is_pinned DESC, created_at DESC, id DESC LIMIT 5",
            [user_id.into()],
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get_by_index::<String>(0).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    txn.rollback().await.unwrap();
    assert!(plan.contains("idx_links_user_list_order_live"), "{plan}");
    assert!(!plan.contains("Sort"), "{plan}");
}
