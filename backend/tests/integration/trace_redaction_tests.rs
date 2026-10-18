//! The HTTP trace layer logs `request.uri()`. Subscriber auth for `/ws` and
//! `/sse` is `?token=<jwt>`, so the span must not record the raw token.

use crate::common;

use opn_onl_backend::redact_request_uri;
use serde_json::{Value, json};

#[test]
fn redacts_token_query_on_ws_and_sse() {
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";

    let ws = redact_request_uri(&format!("/ws?token={jwt}"));
    assert_eq!(ws, "/ws?token=REDACTED");
    assert!(!ws.contains(jwt));

    let sse = redact_request_uri(&format!("/sse?foo=1&token={jwt}&bar=2"));
    assert_eq!(sse, "/sse?foo=1&token=REDACTED&bar=2");
    assert!(!sse.contains(jwt));
}

#[test]
fn redacts_token_case_insensitively_and_leaves_other_uris_alone() {
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
    assert_eq!(
        redact_request_uri(&format!("/ws?TOKEN={jwt}")),
        "/ws?TOKEN=REDACTED"
    );
    assert_eq!(redact_request_uri("/ws"), "/ws");
    assert_eq!(
        redact_request_uri("/links/1/stats?days=90"),
        "/links/1/stats?days=90"
    );
}

/// A password-protected link continues at `/<code>?unlock=<token>`; the token
/// opens the link for two minutes, so it must not reach a log line either.
#[test]
fn redacts_link_unlock_tokens() {
    let unlock = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.unlock.signature";
    let uri = redact_request_uri(&format!("/abc123?unlock={unlock}"));
    assert_eq!(uri, "/abc123?unlock=REDACTED");
    assert!(!uri.contains(unlock));
    assert_eq!(
        redact_request_uri(&format!("/abc123?confirm=1&Unlock={unlock}")),
        "/abc123?confirm=1&Unlock=REDACTED"
    );
}

/// Query-string JWT auth must still work; we only redact it from traces.
#[tokio::test]
async fn ws_query_token_still_authenticates() {
    let (server, _db, _ws) = common::spawn_real_app_ws().await;
    let res = server
        .post("/auth/register")
        .json(&json!({
            "email": common::unique_email(),
            "password": "password123"
        }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let token = res.json::<Value>()["token"].as_str().unwrap().to_string();

    let socket = server
        .get_websocket("/ws")
        .add_query_param("token", &token)
        .await
        .into_websocket()
        .await;
    socket.close().await;
}
