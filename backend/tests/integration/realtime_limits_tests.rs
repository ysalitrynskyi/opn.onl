//! Limits on the real-time transports (`/ws`, `/sse`).
//!
//! The server ignores everything a client sends on `/ws`, yet accepted
//! messages of up to 64 MiB and parsed each one. Any signed-in user could open
//! as many sockets and streams as they liked. And an open `/sse` stream kept
//! the HTTP server from shutting down, so a deploy waited out the
//! orchestrator's kill timeout and the process exited 137.

use crate::common;

use std::sync::Arc;
use std::time::Duration;

use axum_test::WsMessage;
use common::{spawn_real_app_ws, spawn_real_app_ws_with_state, unique_email};
use futures_util::StreamExt;
use opn_onl_backend::handlers::websocket::{ClickEvent, WsState};
use serde_json::{Value, json};

/// Register a user and return `(jwt, user_id)`.
async fn register(server: &axum_test::TestServer) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().unwrap().to_string(),
        body["user_id"].as_i64().unwrap() as i32,
    )
}

fn click_for(user_id: i32, code: &str) -> ClickEvent {
    ClickEvent {
        link_id: 1,
        link_code: code.to_string(),
        user_id: Some(user_id),
        org_id: None,
        click_count: 1,
        country: None,
        city: None,
        device: None,
        browser: None,
        timestamp: "2026-10-11T00:00:00Z".to_string(),
    }
}

async fn open_ws(server: &axum_test::TestServer, token: &str) -> axum_test::TestWebSocket {
    server
        .get_websocket("/ws")
        .add_query_param("token", token)
        .await
        .into_websocket()
        .await
}

async fn open_sse(server: &axum_test::TestServer, token: &str) -> reqwest::Response {
    let url = server
        .server_address()
        .expect("HTTP transport address")
        .join("sse")
        .unwrap();
    reqwest::Client::new()
        .get(url)
        .query(&[("token", token)])
        .send()
        .await
        .expect("open SSE stream")
}

async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..300 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

fn close_code(message: WsMessage) -> u16 {
    match message {
        WsMessage::Close(Some(frame)) => u16::from(frame.code),
        other => panic!("expected a close frame with a code, got {other:?}"),
    }
}

#[tokio::test]
async fn ws_closes_a_connection_that_sends_an_oversized_message() {
    let (server, _db, ws) = spawn_real_app_ws().await;
    let (token, user_id) = register(&server).await;
    let mut socket = open_ws(&server, &token).await;
    // Events reach only receivers that exist when they are sent.
    wait_until("the socket to subscribe", || {
        ws.click_tx.receiver_count() >= 1
    })
    .await;

    // Control: a message right at the limit is accepted (and ignored), and the
    // socket carries on delivering this user's events.
    socket.send_text("x".repeat(4 * 1024)).await;
    ws.broadcast_click(click_for(user_id, "STILL-OPEN"));
    let msg: Value = socket.receive_json().await;
    assert_eq!(msg["link_code"], "STILL-OPEN", "{msg}");

    // One byte over: the server closes with "message too big" instead of
    // buffering and parsing it.
    socket.send_text("x".repeat(4 * 1024 + 1)).await;
    let closed = tokio::time::timeout(Duration::from_secs(2), socket.receive_message())
        .await
        .expect("the server kept a connection open after an oversized message");
    assert_eq!(close_code(closed), 1009);
    wait_until("the connection to be released", || {
        ws.open_connections(user_id) == 0
    })
    .await;
}

#[tokio::test]
async fn a_user_cannot_hold_more_connections_than_the_limit() {
    let ws_state = Arc::new(WsState::new().with_max_connections_per_user(2));
    let (server, _db, ws) = spawn_real_app_ws_with_state(ws_state).await;
    let (token, user_id) = register(&server).await;
    let (other_token, other_id) = register(&server).await;

    let first = open_ws(&server, &token).await;
    let _second = open_ws(&server, &token).await;
    wait_until("both sockets to be counted", || {
        ws.open_connections(user_id) == 2
    })
    .await;

    // A third socket is refused before the upgrade...
    let refused = server
        .get_websocket("/ws")
        .add_query_param("token", &token)
        .expect_failure()
        .await;
    assert_eq!(refused.status_code(), 429, "{}", refused.text());

    // ...and so is an SSE stream: the limit covers both transports together.
    let refused_sse = open_sse(&server, &token).await;
    assert_eq!(refused_sse.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);

    // Control: another account is unaffected.
    let _others = open_ws(&server, &other_token).await;
    wait_until("the other user's socket", || {
        ws.open_connections(other_id) == 1
    })
    .await;

    // Closing one frees a slot.
    first.close().await;
    wait_until("the closed socket to be released", || {
        ws.open_connections(user_id) == 1
    })
    .await;
    let _replacement = open_ws(&server, &token).await;
    wait_until("the replacement to be counted", || {
        ws.open_connections(user_id) == 2
    })
    .await;
}

#[tokio::test]
async fn sse_streams_and_sockets_share_one_limit_and_release_it_on_disconnect() {
    let ws_state = Arc::new(WsState::new().with_max_connections_per_user(2));
    let (server, _db, ws) = spawn_real_app_ws_with_state(ws_state).await;
    let (token, user_id) = register(&server).await;

    let stream = open_sse(&server, &token).await;
    assert_eq!(stream.status(), reqwest::StatusCode::OK);
    let _socket = open_ws(&server, &token).await;
    wait_until("both connections to be counted", || {
        ws.open_connections(user_id) == 2
    })
    .await;

    let refused = server
        .get_websocket("/ws")
        .add_query_param("token", &token)
        .expect_failure()
        .await;
    assert_eq!(refused.status_code(), 429);

    // The stream ends when the client goes away, and its slot with it.
    drop(stream);
    wait_until("the dropped stream to be released", || {
        ws.open_connections(user_id) == 1
    })
    .await;
    let _again = open_ws(&server, &token).await;
}

#[tokio::test]
async fn a_limit_of_zero_means_no_limit() {
    let ws_state = Arc::new(WsState::new().with_max_connections_per_user(0));
    let (server, _db, ws) = spawn_real_app_ws_with_state(ws_state).await;
    let (token, user_id) = register(&server).await;

    let mut sockets = Vec::new();
    for _ in 0..8 {
        sockets.push(open_ws(&server, &token).await);
    }
    wait_until("all sockets to be counted", || {
        ws.open_connections(user_id) == 8
    })
    .await;
}

#[tokio::test]
async fn open_streams_end_when_shutdown_begins() {
    let (server, _db, ws) = spawn_real_app_ws().await;
    let (token, user_id) = register(&server).await;

    let stream = open_sse(&server, &token).await;
    assert_eq!(stream.status(), reqwest::StatusCode::OK);
    let mut body = stream.bytes_stream();
    let mut socket = open_ws(&server, &token).await;
    wait_until("both connections to be counted", || {
        ws.open_connections(user_id) == 2
    })
    .await;

    ws.begin_shutdown();

    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(chunk) = body.next().await {
            chunk.expect("read SSE body");
        }
    })
    .await
    .expect("the SSE stream stayed open after shutdown began");

    let closed = tokio::time::timeout(Duration::from_secs(2), socket.receive_message())
        .await
        .expect("the WebSocket stayed open after shutdown began");
    assert_eq!(close_code(closed), 1001);
}

/// The scenario behind exit 137: the real HTTP server, an SSE client attached,
/// and a shutdown signal. `serve` has to return on its own.
#[tokio::test]
async fn graceful_shutdown_completes_with_an_sse_stream_attached() {
    use opn_onl_backend::{AppState, build_router};

    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = common::setup_test_db().await;
    let ws = Arc::new(WsState::new());
    let mut state = AppState::for_tests(db.clone()).await;
    state.ws_state = Some(ws.clone());
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(ws.clone().shutdown_after(async {
            let _ = stop_rx.await;
        }))
        .await
        .unwrap();
    });

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/auth/register"))
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::CREATED);
    let token = res.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let stream = client
        .get(format!("http://{addr}/sse"))
        .query(&[("token", token.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), reqwest::StatusCode::OK);
    let mut body = stream.bytes_stream();

    stop_tx.send(()).unwrap();

    // Keep reading so the client does not hold the connection open itself.
    let drain = tokio::spawn(async move { while body.next().await.is_some() {} });
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .expect("the server did not shut down with an SSE stream attached")
        .unwrap();
    drain.await.unwrap();
}
