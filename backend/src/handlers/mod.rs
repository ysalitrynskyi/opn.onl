pub mod admin;
pub mod analytics;
pub mod api_keys;
pub mod auth;
pub mod bio;
pub mod contact;
pub mod folders;
pub mod link_access;
pub mod links;
pub mod organizations;
pub mod passkeys;
pub mod reports;
pub mod tags;
pub mod websocket;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::AppState;
use crate::entity::users;

/// An error in the shape every JSON endpoint returns: `{"error": "..."}`.
/// Some handlers answered with a bare text body instead, so a client had to
/// guess per endpoint whether to parse JSON.
pub fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// Whether `user` may use a feature that otherwise requires a verified email.
///
/// The stored `email_verified` flag is left as it is. Every guard (link
/// create, API keys, passkey enrollment) must call this rather than read the
/// column alone, so `REQUIRE_EMAIL_VERIFICATION=false` — the switch for an
/// instance that has no SMTP — cannot be forgotten by the next check.
pub fn verification_satisfied(state: &AppState, user: &users::Model) -> bool {
    user.email_verified || !state.require_email_verification
}
