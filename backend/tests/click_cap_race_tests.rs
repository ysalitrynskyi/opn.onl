//! Regression tests for the max_clicks / burn-after-reading overshoot race.
//!
//! `redirect_link` used to enforce the click cap as check-then-act against the
//! in-memory click buffer, so N concurrent requests could all pass the guard
//! before any click was recorded — a "one-time" burn link could be opened by
//! every concurrent request (observed 20/20 redirects pre-fix). The fix makes
//! capped links consume their click with a single atomic conditional UPDATE
//! (`click_count < max_clicks`), so at most `max_clicks` requests can ever win.
//!
//! These tests drive the real router in-process via `common::spawn_real_app()`
//! (`opn_onl_backend::build_router` against a real Postgres). Concurrent
//! requests are issued with `futures::future::join_all` so the race is
//! exercised in CI with no external server and no env gate. (`JoinSet::spawn`
//! cannot carry `TestServer::get()`: axum-test's `AutoFuture` is `!Send`.)

mod common;

use std::future::IntoFuture;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{json, Value};

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({
            "email": unique_email(),
            "password": "password123",
        }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().expect("user_id") as i32;
    mark_email_verified(db, user_id).await;
    body["token"].as_str().expect("token").to_string()
}

async fn create_link(server: &axum_test::TestServer, token: &str, mut payload: Value) -> String {
    payload["custom_alias"] = json!(unique_code());
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["code"]
        .as_str()
        .expect("code")
        .to_string()
}

struct Slam {
    redirects: usize,
    gone: usize,
    other: usize,
}

/// Fire `n` GET /{code} requests concurrently. Each `TestServer::get` clones
/// the inner transport handle; `join_all` polls them together so they
/// interleave at the DB await (sequential `.await` would not reproduce the race).
async fn slam(server: &axum_test::TestServer, code: &str, n: usize) -> Slam {
    let path = format!("/{code}");
    let responses =
        futures::future::join_all((0..n).map(|_| server.get(&path).into_future())).await;

    let mut slam = Slam {
        redirects: 0,
        gone: 0,
        other: 0,
    };
    for res in responses {
        match res.status_code().as_u16() {
            301 | 302 | 307 | 308 => slam.redirects += 1,
            410 => slam.gone += 1,
            _ => slam.other += 1,
        }
    }
    slam
}

async fn persisted_link(db: &DatabaseConnection, code: &str) -> links::Model {
    links::Entity::find()
        .filter(links::Column::Code.eq(code))
        .one(db)
        .await
        .expect("db error")
        .expect("link not found")
}

/// A link with max_clicks = 1 hit by N concurrent redirects yields exactly one
/// success and N-1 refusals, and click_count ends at 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn max_clicks_one_is_exactly_once_under_concurrency() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({
            "original_url": "https://iana.org/capped-once",
            "max_clicks": 1,
        }),
    )
    .await;

    const N: usize = 20;
    let slam = slam(&server, &code, N).await;

    assert_eq!(
        slam.redirects, 1,
        "max_clicks=1 link {code} served {} redirects to {N} concurrent requests \
         (gone={} other={})",
        slam.redirects, slam.gone, slam.other
    );
    assert_eq!(
        slam.gone,
        N - 1,
        "the other {} concurrent requests must be refused with 410 (other={})",
        N - 1,
        slam.other
    );
    assert_eq!(slam.redirects + slam.gone + slam.other, N);

    let stored = persisted_link(&db, &code).await;
    assert_eq!(
        stored.click_count, 1,
        "atomic consume must leave click_count at exactly 1"
    );

    let follow_up = server.get(&format!("/{code}")).await.status_code().as_u16();
    assert_eq!(follow_up, 410, "exhausted cap must stay 410 after the race");
}

/// A burn-after-reading link hit concurrently is consumed exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn burn_link_is_exactly_once_under_concurrency() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({
            "original_url": "https://iana.org/burn-race-secret",
            "burn_after_reading": true,
        }),
    )
    .await;

    const N: usize = 20;
    let slam = slam(&server, &code, N).await;

    assert_eq!(
        slam.redirects, 1,
        "burn link {code} served {} redirects to {N} concurrent requests \
         (gone={} other={}); a one-time link must be opened exactly once",
        slam.redirects, slam.gone, slam.other
    );
    assert_eq!(
        slam.gone,
        N - 1,
        "concurrent losers must get 410 Gone (other={})",
        slam.other
    );
    assert_eq!(slam.redirects + slam.gone + slam.other, N);

    let stored = persisted_link(&db, &code).await;
    assert_eq!(
        stored.click_count, 1,
        "burn consume must leave click_count at 1"
    );
    assert!(
        stored.burned_at.is_some(),
        "the winning click must stamp burned_at"
    );

    let follow_up = server.get(&format!("/{code}")).await.status_code().as_u16();
    assert_eq!(follow_up, 410, "burned link must stay 410 after the race");
}

/// A link with max_clicks = 5 hit by 20 concurrent requests yields exactly 5
/// successes, and the persisted click_count settles at 5.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn capped_link_never_overshoots_under_concurrency() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let code = create_link(
        &server,
        &token,
        json!({
            "original_url": "https://iana.org/capped-race",
            "max_clicks": 5,
        }),
    )
    .await;

    const MAX: usize = 5;
    const N: usize = 20;
    let slam = slam(&server, &code, N).await;

    assert_eq!(
        slam.redirects, MAX,
        "max_clicks={MAX} link {code} served {} redirects to {N} concurrent requests \
         (gone={} other={})",
        slam.redirects, slam.gone, slam.other
    );
    assert_eq!(
        slam.gone,
        N - MAX,
        "the remaining requests must be 410 (other={})",
        slam.other
    );
    assert_eq!(slam.redirects + slam.gone + slam.other, N);

    let stored = persisted_link(&db, &code).await;
    assert_eq!(
        stored.click_count, MAX as i64,
        "persisted click_count must settle at exactly max_clicks"
    );
}
