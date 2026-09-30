pub mod admin;
pub mod analytics;
pub mod api_keys;
pub mod auth;
pub mod bio;
pub mod contact;
pub mod folders;
pub mod links;
pub mod organizations;
pub mod passkeys;
pub mod tags;
pub mod websocket;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// An error in the shape every JSON endpoint returns: `{"error": "..."}`.
/// Some handlers answered with a bare text body instead, so a client had to
/// guess per endpoint whether to parse JSON.
pub fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}
