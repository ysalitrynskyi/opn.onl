use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State, Query,
    },
    http::{HeaderMap, StatusCode},
    response::{Response, IntoResponse},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use crate::utils::decode_jwt;

/// WebSocket state for real-time updates
#[derive(Clone)]
pub struct WsState {
    /// Broadcast channel for click events. Connections subscribe to this and
    /// filter by user_id; receivers are dropped automatically when a connection
    /// closes, so there is no per-connection state to leak.
    pub click_tx: broadcast::Sender<ClickEvent>,
}

impl WsState {
    pub fn new() -> Self {
        let (click_tx, _) = broadcast::channel(1000);
        Self { click_tx }
    }

    /// Broadcast a click event. Connections subscribe to `click_tx` and filter
    /// by user_id on their side; there is no per-connection state to clean up.
    pub fn broadcast_click(&self, event: ClickEvent) {
        let _ = self.click_tx.send(event);
    }
}

impl Default for WsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Click event for real-time updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickEvent {
    pub link_id: i32,
    pub link_code: String,
    pub user_id: Option<i32>,
    pub click_count: i32,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub timestamp: String,
}

/// WebSocket message types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "subscribe")]
    Subscribe { link_id: Option<i32>, user_id: Option<i32> },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { link_id: Option<i32> },
    #[serde(rename = "click")]
    Click(ClickEvent),
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Query params for WebSocket/SSE authentication
#[derive(Debug, Deserialize)]
pub struct WsAuthQuery {
    pub token: Option<String>,
}

/// Extract user_id from token
fn get_user_id_from_token(token: &str) -> Option<i32> {
    decode_jwt(token).ok().map(|claims| claims.user_id)
}

/// Extract user_id from headers
fn get_user_id_from_header(headers: &HeaderMap) -> Option<i32> {
    let auth_header = headers.get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    decode_jwt(token).ok().map(|claims| claims.user_id)
}

/// WebSocket handler for real-time analytics
/// Requires authentication via query parameter: /ws?token=<jwt_token>
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Query(query): Query<WsAuthQuery>,
) -> Response {
    // Try to get user_id from query token or header
    let user_id = query.token
        .as_ref()
        .and_then(|t| get_user_id_from_token(t))
        .or_else(|| get_user_id_from_header(&headers));
    
    // Require authentication
    let user_id = match user_id {
        Some(id) => id,
        None => {
            return (StatusCode::UNAUTHORIZED, "Authentication required. Use /ws?token=<jwt_token>").into_response();
        }
    };
    
    let ws_state = state.ws_state.clone().unwrap_or_else(|| Arc::new(WsState::new()));
    ws.on_upgrade(move |socket| handle_socket(socket, ws_state, user_id))
}

async fn handle_socket(socket: WebSocket, ws_state: Arc<WsState>, user_id: i32) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the global channel and filter by user_id. The receiver is
    // dropped when this task ends, so nothing accumulates server-side.
    let mut global_rx = ws_state.click_tx.subscribe();

    // Spawn task to forward click events to client
    let send_task = tokio::spawn(async move {
        loop {
            match global_rx.recv().await {
                Ok(event) => {
                    // Only forward events for this user's links
                    if event.user_id == Some(user_id) {
                        let msg = WsMessage::Click(event);
                        let json = serde_json::to_string(&msg).unwrap_or_default();
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    
    // Handle incoming messages
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        match ws_msg {
                            WsMessage::Ping => {
                                // Pong is handled by axum automatically
                            }
                            WsMessage::Subscribe { .. } => {
                                // Subscription is now handled automatically based on auth
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
}

/// Handler for SSE (Server-Sent Events) alternative
/// This can be used if WebSocket isn't available
/// Requires authentication via query parameter: /sse?token=<jwt_token>
pub async fn sse_handler(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Query(query): Query<WsAuthQuery>,
) -> Response {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures::stream;
    
    // Try to get user_id from query token or header
    let user_id = query.token
        .as_ref()
        .and_then(|t| get_user_id_from_token(t))
        .or_else(|| get_user_id_from_header(&headers));
    
    // Require authentication
    let user_id = match user_id {
        Some(id) => id,
        None => {
            return (StatusCode::UNAUTHORIZED, "Authentication required. Use /sse?token=<jwt_token>").into_response();
        }
    };
    
    let ws_state = state.ws_state.clone().unwrap_or_else(|| Arc::new(WsState::new()));
    let rx = ws_state.click_tx.subscribe();
    
    // Filter events to only include this user's links
    let stream = stream::unfold((rx, user_id), |(mut rx, uid)| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Only send events for this user's links
                    if event.user_id == Some(uid) {
                        let json = serde_json::to_string(&event).unwrap_or_default();
                        return Some((Ok::<_, std::convert::Infallible>(Event::default().data(json)), (rx, uid)));
                    }
                    // Skip events for other users
                    continue;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

