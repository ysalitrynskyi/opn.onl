//! Integration tests for the real-time analytics transports (`/ws`, `/sse`).
//!
//! This file previously defined LOCAL copies of the event structs and only
//! tested that those copies round-tripped through serde — it exercised none of
//! the production handlers, so it passed no matter what `ws_handler` /
//! `sse_handler` did. It now drives the real router over an HTTP transport with
//! a real `WsState`, asserting the three properties that actually matter:
//!   * both transports reject unauthenticated / bad / revoked tokens
//!     (the `token_version` revocation the audit added), and
//!   * a connected `/ws` subscriber receives broadcast click events for its own
//!     links but NOT another user's (the per-user filter in `handle_socket`).

mod common;

use common::{mark_email_verified, spawn_real_app_ws, unique_email};
use opn_onl_backend::handlers::websocket::{ClickEvent, WsState};
use serde_json::{json, Value};
use std::time::Duration;

/// Register a user and return `(jwt, user_id)`.
async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().unwrap().to_string(),
        body["user_id"].as_i64().unwrap() as i32,
    )
}

/// A click event addressed to `user_id`, tagged with `code` so the receiving
/// side can tell whose event it got.
fn click_for(user_id: i32, code: &str) -> ClickEvent {
    ClickEvent {
        link_id: 1,
        link_code: code.to_string(),
        user_id: Some(user_id),
        click_count: 7,
        country: Some("US".to_string()),
        city: Some("NYC".to_string()),
        device: Some("Desktop".to_string()),
        browser: Some("Firefox".to_string()),
        timestamp: "2026-07-11T00:00:00Z".to_string(),
    }
}

/// Wait until the handler has actually subscribed to the broadcast channel.
/// `broadcast` only delivers to receivers that existed at send time, so
/// broadcasting before the socket's `subscribe()` runs would race; gating on
/// `receiver_count` makes the delivery test deterministic.
async fn wait_for_subscriber(ws: &WsState) {
    for _ in 0..200 {
        if ws.click_tx.receiver_count() >= 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("socket never subscribed to the broadcast channel");
}

#[tokio::test]
async fn ws_rejects_missing_and_bad_tokens() {
    let (server, _db, _ws) = spawn_real_app_ws().await;

    // No token at all.
    let res = server.get_websocket("/ws").expect_failure().await;
    assert_eq!(res.status_code(), 401, "missing token must be 401");

    // A syntactically-present but invalid token.
    let res = server
        .get_websocket("/ws")
        .add_query_param("token", "not.a.jwt")
        .expect_failure()
        .await;
    assert_eq!(res.status_code(), 401, "garbage token must be 401");
}

#[tokio::test]
async fn ws_rejects_token_revoked_by_version_bump() {
    use opn_onl_backend::entity::users;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

    let (server, db, _ws) = spawn_real_app_ws().await;
    let (token, user_id) = register(&server, &unique_email()).await;

    // Simulate a password change/reset: bump token_version so the issued JWT is
    // now stale. The WS handshake must honor that, not just the signature.
    let user = users::Entity::find_by_id(user_id).one(&db).await.unwrap().unwrap();
    let current = user.token_version;
    let mut active: users::ActiveModel = user.into();
    active.token_version = Set(current + 1);
    active.update(&db).await.unwrap();

    let res = server
        .get_websocket("/ws")
        .add_query_param("token", &token)
        .expect_failure()
        .await;
    assert_eq!(res.status_code(), 401, "revoked token must be 401 on /ws");
}

#[tokio::test]
async fn ws_delivers_owner_click_and_filters_other_users() {
    let (server, db, ws) = spawn_real_app_ws().await;
    let (token, user_id) = register(&server, &unique_email()).await;
    mark_email_verified(&db, user_id).await;

    let mut socket = server
        .get_websocket("/ws")
        .add_query_param("token", &token)
        .await
        .into_websocket()
        .await;

    wait_for_subscriber(&ws).await;

    // Broadcast an event for a DIFFERENT user first, then one for the connected
    // user. The socket must skip the first and deliver the second — if the
    // per-user filter regressed, the first message received would be the other
    // user's event and the code assertion below would fail.
    let other_user = user_id + 100_000;
    ws.broadcast_click(click_for(other_user, "OTHER-USER-EVENT"));
    ws.broadcast_click(click_for(user_id, "MY-EVENT"));

    let msg: Value = socket.receive_json().await;
    assert_eq!(msg["type"], "click", "expected a click frame: {msg}");
    assert_eq!(
        msg["link_code"], "MY-EVENT",
        "socket must receive its own event, never another user's: {msg}"
    );
    assert_eq!(msg["user_id"], user_id);

    socket.close().await;
}

#[tokio::test]
async fn sse_rejects_missing_and_revoked_tokens() {
    use opn_onl_backend::entity::users;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

    let (server, db, _ws) = spawn_real_app_ws().await;

    // No token → 401 before the stream opens.
    let res = server.get("/sse").expect_failure().await;
    assert_eq!(res.status_code(), 401, "missing token must be 401 on /sse");

    // Revoked token → 401 (same DB-backed check as /ws and the HTTP API).
    let (token, user_id) = register(&server, &unique_email()).await;
    let user = users::Entity::find_by_id(user_id).one(&db).await.unwrap().unwrap();
    let current = user.token_version;
    let mut active: users::ActiveModel = user.into();
    active.token_version = Set(current + 1);
    active.update(&db).await.unwrap();

    let res = server
        .get("/sse")
        .add_query_param("token", &token)
        .expect_failure()
        .await;
    assert_eq!(res.status_code(), 401, "revoked token must be 401 on /sse");
}
