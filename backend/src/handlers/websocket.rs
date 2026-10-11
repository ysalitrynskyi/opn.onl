use crate::utils::decode_jwt;
use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::broadcast;

const DEFAULT_AUTH_REVALIDATE_INTERVAL: Duration = Duration::from_secs(30);

/// WebSocket state for real-time updates
#[derive(Clone)]
pub struct WsState {
    /// Broadcast channel for click events. Connections subscribe to this and
    /// filter by link owner (see `event_is_for`); receivers are dropped
    /// automatically when a connection closes, so there is no per-connection
    /// state to leak.
    pub click_tx: broadcast::Sender<ClickEvent>,
    auth_revalidate_interval: Duration,
}

impl WsState {
    pub fn new() -> Self {
        Self::with_auth_revalidate_interval(DEFAULT_AUTH_REVALIDATE_INTERVAL)
    }

    /// Override the revalidation interval, primarily for deterministic transport
    /// tests. Production uses [`Self::new`] and the 30-second default.
    pub fn with_auth_revalidate_interval(auth_revalidate_interval: Duration) -> Self {
        let (click_tx, _) = broadcast::channel(1000);
        Self {
            click_tx,
            auth_revalidate_interval,
        }
    }

    /// Broadcast a click event. Connections subscribe to `click_tx` and filter
    /// on their side; there is no per-connection state to clean up.
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
    /// Set for an organization link. Such an event is for the organization's
    /// current members, not for whoever happened to create the link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<i32>,
    pub click_count: i64,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub timestamp: String,
}

/// Whether `event` is for subscriber `user_id`, who currently belongs to
/// `org_ids`. A personal link's clicks go to its creator; an organization
/// link's go to every current member (the Read rule in `link_access`) and the
/// creator gets nothing extra once they have left.
fn event_is_for(event: &ClickEvent, user_id: i32, org_ids: &HashSet<i32>) -> bool {
    match event.org_id {
        Some(org_id) => org_ids.contains(&org_id),
        None => event.user_id == Some(user_id),
    }
}

/// The organizations `user_id` belongs to right now. Read once per connection
/// and again on every revalidation, so a removed member stops receiving an
/// organization's clicks within one revalidation interval without a query per
/// event. A failed lookup yields an empty set: organization events pause
/// until the next successful refresh rather than reaching a stale member.
async fn current_org_ids(db: &sea_orm::DatabaseConnection, user_id: i32) -> HashSet<i32> {
    use crate::entity::org_members;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    match org_members::Entity::find()
        .filter(org_members::Column::UserId.eq(user_id))
        .all(db)
        .await
    {
        Ok(rows) => rows.into_iter().map(|member| member.org_id).collect(),
        Err(error) => {
            tracing::warn!(
                "Failed to load organizations for realtime stream: {}",
                error
            );
            HashSet::new()
        }
    }
}

/// WebSocket message types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "subscribe")]
    Subscribe {
        link_id: Option<i32>,
        user_id: Option<i32>,
    },
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

#[derive(Clone)]
struct WsCredentials {
    token: Option<String>,
    headers: HeaderMap,
}

/// Resolve a `?token=` query JWT to a user id WITH the same DB-backed revocation
/// check the HTTP API uses: the user must exist, not be soft-deleted, and the
/// token's `token_version` must match. Previously the WS/SSE handshake only
/// decoded the JWT signature, so a token revoked by password change/reset kept a
/// live analytics subscription until natural expiry.
async fn resolve_ws_token(db: &sea_orm::DatabaseConnection, token: &str) -> Option<i32> {
    use crate::entity::users;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let claims = decode_jwt(token).ok()?;
    let user = users::Entity::find_by_id(claims.user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(db)
        .await
        .ok()??;
    if user.token_version == claims.token_version {
        Some(user.id)
    } else {
        None
    }
}

/// Resolve the subscriber's user id from the `?token=` query param first, then
/// the `Authorization` header (which also honors API keys) — both DB-checked.
async fn resolve_ws_user(
    db: &sea_orm::DatabaseConnection,
    token: Option<&str>,
    headers: &HeaderMap,
) -> Option<i32> {
    if let Some(t) = token
        && let Some(id) = resolve_ws_token(db, t).await
    {
        return Some(id);
    }
    crate::handlers::links::get_user_id_from_header(db, headers).await
}

/// WebSocket handler for real-time analytics
/// Requires authentication via query parameter: /ws?token=<jwt_token>
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Query(query): Query<WsAuthQuery>,
) -> Response {
    let credentials = WsCredentials {
        token: query.token,
        headers,
    };

    // Resolve + DB-verify the subscriber (query token or Authorization header).
    let user_id = match resolve_ws_user(
        &state.db,
        credentials.token.as_deref(),
        &credentials.headers,
    )
    .await
    {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                "Authentication required. Use /ws?token=<jwt_token>",
            )
                .into_response();
        }
    };

    let ws_state = state
        .ws_state
        .clone()
        .unwrap_or_else(|| Arc::new(WsState::new()));
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, ws_state, db, credentials, user_id))
}

async fn handle_socket(
    socket: WebSocket,
    ws_state: Arc<WsState>,
    db: sea_orm::DatabaseConnection,
    credentials: WsCredentials,
    user_id: i32,
) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the global channel and filter per event. The receiver is
    // dropped when this task ends, so nothing accumulates server-side.
    let mut global_rx = ws_state.click_tx.subscribe();
    let mut org_ids = current_org_ids(&db, user_id).await;
    let mut revalidate = tokio::time::interval(ws_state.auth_revalidate_interval);
    revalidate.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = revalidate.tick() => {
                let current_user = resolve_ws_user(
                    &db,
                    credentials.token.as_deref(),
                    &credentials.headers,
                )
                .await;
                if current_user != Some(user_id) {
                    let _ = sender.send(Message::Close(None)).await;
                    break;
                }
                org_ids = current_org_ids(&db, user_id).await;
            }
            event = global_rx.recv() => match event {
                Ok(event) => {
                    // Only forward events for links this user may read
                    if event_is_for(&event, user_id, &org_ids) {
                        let msg = WsMessage::Click(event);
                        let json = serde_json::to_string(&msg).unwrap_or_default();
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = receiver.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
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
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
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

    let credentials = WsCredentials {
        token: query.token,
        headers,
    };

    // Resolve + DB-verify the subscriber (query token or Authorization header).
    let user_id = match resolve_ws_user(
        &state.db,
        credentials.token.as_deref(),
        &credentials.headers,
    )
    .await
    {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                "Authentication required. Use /sse?token=<jwt_token>",
            )
                .into_response();
        }
    };

    let ws_state = state
        .ws_state
        .clone()
        .unwrap_or_else(|| Arc::new(WsState::new()));
    let rx = ws_state.click_tx.subscribe();
    let mut revalidate = tokio::time::interval(ws_state.auth_revalidate_interval);
    revalidate.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let db = state.db.clone();
    let org_ids = current_org_ids(&db, user_id).await;

    // Filter events to only include links this user may read, and end the
    // stream when its credential is revoked or the user is soft-deleted.
    let stream = stream::unfold(
        (rx, revalidate, db, credentials, user_id, org_ids),
        |(mut rx, mut revalidate, db, credentials, uid, mut org_ids)| async move {
            loop {
                tokio::select! {
                    _ = revalidate.tick() => {
                        let current_user = resolve_ws_user(
                            &db,
                            credentials.token.as_deref(),
                            &credentials.headers,
                        )
                        .await;
                        if current_user != Some(uid) {
                            return None;
                        }
                        org_ids = current_org_ids(&db, uid).await;
                    }
                    event = rx.recv() => match event {
                        Ok(event) => {
                            // Only send events for links this user may read
                            if event_is_for(&event, uid, &org_ids) {
                                let json = serde_json::to_string(&event).unwrap_or_default();
                                return Some((
                                    Ok::<_, std::convert::Infallible>(Event::default().data(json)),
                                    (rx, revalidate, db, credentials, uid, org_ids),
                                ));
                            }
                            // Skip events for other links
                            continue;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                }
            }
        },
    );

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
