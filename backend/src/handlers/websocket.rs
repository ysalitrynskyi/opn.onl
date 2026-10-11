use crate::utils::decode_jwt;
use crate::utils::env::env_parse;
use axum::{
    extract::{
        Query, State,
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::sync::{broadcast, watch};

const DEFAULT_AUTH_REVALIDATE_INTERVAL: Duration = Duration::from_secs(30);

/// Open `/ws` and `/sse` connections one user may hold at once, together
/// (`WS_MAX_CONNECTIONS_PER_USER`). Each connection is a task, a broadcast
/// receiver and a user lookup every revalidation interval; a handful covers
/// several tabs and devices.
const DEFAULT_MAX_CONNECTIONS_PER_USER: usize = 5;

/// Largest message the server accepts on `/ws`. It ignores everything a client
/// sends, so this only has to fit a ping or a subscribe; the library default of
/// 64 MiB let any signed-in user make the server buffer and parse that much per
/// connection.
const MAX_CLIENT_MESSAGE_BYTES: usize = 4 * 1024;

/// WebSocket state for real-time updates
#[derive(Clone)]
pub struct WsState {
    /// Broadcast channel for click events. Connections subscribe to this and
    /// filter by link owner (see `event_is_for`); receivers are dropped
    /// automatically when a connection closes, so there is no per-connection
    /// state to leak.
    pub click_tx: broadcast::Sender<ClickEvent>,
    auth_revalidate_interval: Duration,
    /// 0 means no limit.
    max_connections_per_user: usize,
    open_connections: Arc<Mutex<HashMap<i32, usize>>>,
    /// Flips to `true` when the process starts shutting down. Open `/sse`
    /// streams never finish on their own, and the HTTP server waits for every
    /// response to finish before it exits, so each connection watches this and
    /// ends itself.
    shutting_down: Arc<watch::Sender<bool>>,
}

/// One open `/ws` or `/sse` connection, counted against its user's limit until
/// dropped.
pub struct ConnectionGuard {
    user_id: i32,
    open_connections: Arc<Mutex<HashMap<i32, usize>>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let mut open = self
            .open_connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = open.get_mut(&self.user_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.remove(&self.user_id);
            }
        }
    }
}

impl WsState {
    pub fn new() -> Self {
        Self::with_auth_revalidate_interval(DEFAULT_AUTH_REVALIDATE_INTERVAL)
            .with_max_connections_per_user(env_parse(
                "WS_MAX_CONNECTIONS_PER_USER",
                DEFAULT_MAX_CONNECTIONS_PER_USER,
            ))
    }

    /// Override the revalidation interval, primarily for deterministic transport
    /// tests. Production uses [`Self::new`] and the 30-second default.
    pub fn with_auth_revalidate_interval(auth_revalidate_interval: Duration) -> Self {
        let (click_tx, _) = broadcast::channel(1000);
        Self {
            click_tx,
            auth_revalidate_interval,
            max_connections_per_user: DEFAULT_MAX_CONNECTIONS_PER_USER,
            open_connections: Arc::default(),
            shutting_down: Arc::new(watch::channel(false).0),
        }
    }

    /// Cap the `/ws` and `/sse` connections one user may hold at once; 0 lifts
    /// the cap.
    pub fn with_max_connections_per_user(mut self, max: usize) -> Self {
        self.max_connections_per_user = max;
        self
    }

    /// Count a new connection for `user_id`, or `None` when they are at the
    /// limit. The count drops again when the guard does.
    pub fn try_acquire(&self, user_id: i32) -> Option<ConnectionGuard> {
        let mut open = self
            .open_connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let count = open.get(&user_id).copied().unwrap_or(0);
        if self.max_connections_per_user != 0 && count >= self.max_connections_per_user {
            return None;
        }
        open.insert(user_id, count + 1);
        Some(ConnectionGuard {
            user_id,
            open_connections: self.open_connections.clone(),
        })
    }

    /// Connections `user_id` currently holds open.
    pub fn open_connections(&self, user_id: i32) -> usize {
        self.open_connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&user_id)
            .copied()
            .unwrap_or(0)
    }

    /// Tell every open connection to finish.
    pub fn begin_shutdown(&self) {
        self.shutting_down.send_replace(true);
    }

    /// Wait for `signal`, then end every open connection. Hand this to
    /// `with_graceful_shutdown`: the server waits for each response to finish
    /// before it exits, an open `/sse` stream never does, and a deploy with one
    /// stream attached ran into the orchestrator's kill timeout and exited 137.
    pub async fn shutdown_after(self: Arc<Self>, signal: impl std::future::Future<Output = ()>) {
        signal.await;
        self.begin_shutdown();
    }

    fn shutdown_watch(&self) -> watch::Receiver<bool> {
        self.shutting_down.subscribe()
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

/// Resolves once shutdown has begun. A dropped sender means the state is gone
/// and nothing can be broadcast any more, which also ends the connection.
async fn shutdown_began(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stopping| *stopping).await;
}

fn too_many_connections() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        "Too many open realtime connections for this account. Close one and try again.",
    )
        .into_response()
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
    let Some(connection) = ws_state.try_acquire(user_id) else {
        return too_many_connections();
    };
    let db = state.db.clone();
    ws.max_message_size(MAX_CLIENT_MESSAGE_BYTES)
        .max_frame_size(MAX_CLIENT_MESSAGE_BYTES)
        .on_upgrade(move |socket| {
            handle_socket(socket, ws_state, db, credentials, user_id, connection)
        })
}

async fn handle_socket(
    socket: WebSocket,
    ws_state: Arc<WsState>,
    db: sea_orm::DatabaseConnection,
    credentials: WsCredentials,
    user_id: i32,
    // Held for the life of the socket so the user's connection count drops
    // when it ends.
    _connection: ConnectionGuard,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut shutdown = ws_state.shutdown_watch();

    // Subscribe to the global channel and filter per event. The receiver is
    // dropped when this task ends, so nothing accumulates server-side.
    let mut global_rx = ws_state.click_tx.subscribe();
    let mut org_ids = current_org_ids(&db, user_id).await;
    let mut revalidate = tokio::time::interval(ws_state.auth_revalidate_interval);
    revalidate.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown_began(&mut shutdown) => {
                let _ = sender
                    .send(Message::Close(Some(CloseFrame {
                        code: close_code::AWAY,
                        reason: "server shutting down".into(),
                    })))
                    .await;
                break;
            }
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
                Some(Err(_)) => {
                    // The size limit is the one way a well-behaved client can
                    // trip an error here; a peer that is broken some other way
                    // is not reading the close frame anyway.
                    let _ = sender
                        .send(Message::Close(Some(CloseFrame {
                            code: close_code::SIZE,
                            reason: "messages from clients may not exceed 4 KiB".into(),
                        })))
                        .await;
                    break;
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Everything one `/sse` stream carries between events.
struct SseSession {
    rx: broadcast::Receiver<ClickEvent>,
    revalidate: tokio::time::Interval,
    db: sea_orm::DatabaseConnection,
    credentials: WsCredentials,
    user_id: i32,
    org_ids: HashSet<i32>,
    shutdown: watch::Receiver<bool>,
    _connection: ConnectionGuard,
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
    let Some(connection) = ws_state.try_acquire(user_id) else {
        return too_many_connections();
    };
    let rx = ws_state.click_tx.subscribe();
    let mut revalidate = tokio::time::interval(ws_state.auth_revalidate_interval);
    revalidate.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let db = state.db.clone();
    let org_ids = current_org_ids(&db, user_id).await;

    // Filter events to only include links this user may read, and end the
    // stream when its credential is revoked, the user is soft-deleted or the
    // server is shutting down. The connection guard lives in the state, so the
    // user's count drops when the stream is dropped.
    let session = SseSession {
        rx,
        revalidate,
        db,
        credentials,
        user_id,
        org_ids,
        shutdown: ws_state.shutdown_watch(),
        _connection: connection,
    };
    let stream = stream::unfold(session, |mut session| async move {
        loop {
            tokio::select! {
                _ = shutdown_began(&mut session.shutdown) => return None,
                _ = session.revalidate.tick() => {
                    let current_user = resolve_ws_user(
                        &session.db,
                        session.credentials.token.as_deref(),
                        &session.credentials.headers,
                    )
                    .await;
                    if current_user != Some(session.user_id) {
                        return None;
                    }
                    session.org_ids = current_org_ids(&session.db, session.user_id).await;
                }
                event = session.rx.recv() => match event {
                    Ok(event) => {
                        // Only send events for links this user may read
                        if event_is_for(&event, session.user_id, &session.org_ids) {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            return Some((
                                Ok::<_, std::convert::Infallible>(Event::default().data(json)),
                                session,
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
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
