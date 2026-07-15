use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use webauthn_rs::prelude::*;
use webauthn_rs::Webauthn;
use sea_orm::*;
use url::Url;
use uuid::Uuid;
use chrono::Utc;

use crate::AppState;
use crate::entity::{users, passkeys};
use crate::utils::jwt::create_jwt;

// In-memory store for registration/auth state
// In production, use Redis or database with expiration
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// How long a pending registration/authentication challenge is kept before it expires.
const PASSKEY_STATE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// In-memory map of pending WebAuthn challenges with per-entry expiry. Entries
/// are pruned on insert so abandoned ceremonies can't accumulate unbounded, and
/// expired entries are rejected on lookup.
/// NOTE: process-local - for multi-instance deployments this should move to Redis.
struct ExpiringMap<V> {
    inner: Mutex<HashMap<String, (std::time::Instant, V)>>,
}

impl<V> ExpiringMap<V> {
    fn new() -> Self {
        Self { inner: Mutex::new(HashMap::new()) }
    }

    fn insert(&self, key: String, value: V) {
        let mut map = self.inner.lock().unwrap();
        let now = std::time::Instant::now();
        map.retain(|_, (t, _)| now.duration_since(*t) < PASSKEY_STATE_TTL);
        map.insert(key, (now, value));
    }

    fn remove(&self, key: &str) -> Option<V> {
        let mut map = self.inner.lock().unwrap();
        match map.remove(key) {
            Some((t, v)) if std::time::Instant::now().duration_since(t) < PASSKEY_STATE_TTL => Some(v),
            _ => None,
        }
    }
}

static REG_STATE: Lazy<ExpiringMap<PasskeyRegistration>> = Lazy::new(ExpiringMap::new);
static AUTH_STATE: Lazy<ExpiringMap<PasskeyAuthentication>> = Lazy::new(ExpiringMap::new);

// Helper to get Webauthn instance
fn get_webauthn() -> Webauthn {
    let rp_id = std::env::var("WEBAUTHN_RP_ID")
        .unwrap_or_else(|_| {
            std::env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "localhost".to_string())
                .replace("https://", "")
                .replace("http://", "")
                .split('/')
                .next()
                .unwrap_or("localhost")
                .to_string()
        });
    
    let rp_origin = std::env::var("FRONTEND_URL")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    
    let origin_url = Url::parse(&rp_origin).unwrap_or_else(|e| {
        tracing::error!("Invalid FRONTEND_URL for WebAuthn: {} - using localhost fallback", e);
        Url::parse("http://localhost:5173").expect("Hardcoded URL should always parse")
    });
    
    // Extract just the host for rp_id (e.g., "opn.onl" from "https://opn.onl")
    let effective_rp_id = origin_url.host_str().unwrap_or(&rp_id);
    
    WebauthnBuilder::new(effective_rp_id, &origin_url)
        .map_err(|e| {
            tracing::error!("Failed to create WebAuthn builder: {:?}", e);
            e
        })
        .and_then(|builder| {
            builder.build().map_err(|e| {
                tracing::error!("Failed to build WebAuthn: {:?}", e);
                e
            })
        })
        .unwrap_or_else(|_| {
            // Last resort fallback for development only
            tracing::warn!("WebAuthn falling back to localhost - passkeys may not work in production!");
            let fallback_url = Url::parse("http://localhost:5173").expect("Hardcoded URL");
            WebauthnBuilder::new("localhost", &fallback_url)
                .expect("Localhost WebAuthn builder")
                .build()
                .expect("Localhost WebAuthn build")
        })
}

/// Instance kill-switch for passkeys (default ON, like the other ENABLE_* flags).
/// When `false`, the enroll and login ceremonies are refused; management
/// endpoints (list/delete/rename) keep working so users can still clean up.
fn passkeys_enabled() -> bool {
    std::env::var("ENABLE_PASSKEYS")
        .map(|v| v != "false")
        .unwrap_or(true)
}

#[derive(Deserialize)]
pub struct RegisterStartRequest {
    /// Accepted for wire compatibility but IGNORED server-side: the target
    /// account is taken from the caller's authenticated identity, never from
    /// this field (see `register_start`).
    #[allow(dead_code)]
    pub username: String,
}

#[derive(Serialize)]
pub struct RegisterStartResponse {
    pub options: CreationChallengeResponse,
}

#[derive(Deserialize)]
pub struct RegisterFinishRequest {
    /// Accepted for wire compatibility but IGNORED server-side: the credential
    /// is bound to the caller's authenticated identity (see `register_finish`).
    #[allow(dead_code)]
    pub username: String,
    pub credential: RegisterPublicKeyCredential,
}

#[derive(Deserialize)]
pub struct LoginStartRequest {
    pub username: String,
}

#[derive(Serialize)]
pub struct LoginStartResponse {
    pub options: RequestChallengeResponse,
}

#[derive(Deserialize)]
pub struct LoginFinishRequest {
    pub username: String,
    pub credential: PublicKeyCredential,
}

#[derive(Serialize, ToSchema)]
pub struct PasskeyAuthResponse {
    pub token: String,
    pub email_verified: bool,
    pub is_admin: bool,
}

/// Begin passkey enrollment for the authenticated caller. Returns a WebAuthn
/// `CreationChallengeResponse` to pass to the browser's credential API. The
/// request/response bodies are standard WebAuthn ceremony objects and are not
/// expanded into the schema.
#[utoipa::path(
    post,
    path = "/auth/passkey/register/start",
    responses(
        (status = 200, description = "WebAuthn creation challenge"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Passkeys are disabled on this instance"),
        (status = 404, description = "User not found"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn register_start(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(_payload): Json<RegisterStartRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return (StatusCode::FORBIDDEN, "Passkeys are disabled on this instance").into_response();
    }
    // Passkey enrollment MUST be authenticated: a passkey may only be added to
    // the caller's own account. We derive the target account from the caller's
    // authenticated identity, NOT from the client-supplied `username`. This
    // closes the account-takeover hole where anyone who knew a victim's email
    // could enroll their own authenticator onto the victim's account.
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let user = match users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
    {
        Ok(Some(u)) => u,
        _ => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    // Deterministic UUID from ID for demo purposes
    let user_unique_id = Uuid::from_bytes(user.id.to_le_bytes().repeat(4)[0..16].try_into().unwrap());

    // In a real app, you might want to exclude already registered credentials here
    let exclude_credentials: Option<Vec<CredentialID>> = None;

    let webauthn = get_webauthn();
    let (ccr, reg_state) = match webauthn.start_passkey_registration(
        user_unique_id,
        &user.email,
        &user.email,
        exclude_credentials,
    ) {
        Ok(res) => res,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to start registration").into_response(),
    };

    // Key the pending-challenge store by the authenticated user id so the finish
    // step can only complete for the same account that started the ceremony.
    REG_STATE.insert(user.id.to_string(), reg_state);

    (StatusCode::OK, Json(RegisterStartResponse { options: ccr })).into_response()
}

/// Complete passkey enrollment for the authenticated caller. Rejects a
/// credential already registered to any account (409).
#[utoipa::path(
    post,
    path = "/auth/passkey/register/finish",
    responses(
        (status = 200, description = "Passkey registered"),
        (status = 400, description = "Invalid or expired registration ceremony"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Passkeys are disabled on this instance"),
        (status = 404, description = "User not found"),
        (status = 409, description = "This passkey is already registered"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn register_finish(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<RegisterFinishRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return (StatusCode::FORBIDDEN, "Passkeys are disabled on this instance").into_response();
    }
    // Same authenticated-identity rule as register_start: the credential is bound
    // to the CALLER's account, never to a client-supplied username. The pending
    // challenge is looked up by the authenticated user id.
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let reg_state = match REG_STATE.remove(&user_id.to_string()) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "Registration state not found").into_response(),
    };

    let webauthn = get_webauthn();
    let passkey = match webauthn.finish_passkey_registration(&payload.credential, &reg_state) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to finish registration").into_response(),
    };

    let user = match users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
    {
        Ok(Some(user)) => user,
        _ => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    // Save passkey to DB - serialize the passkey for storage
    let passkey_json = serde_json::to_string(&passkey).unwrap_or_default();
    let cred_id_str = format!("{:?}", passkey.cred_id());

    // A credential id is globally unique to one authenticator credential.
    // Reject a re-registration of an already-known credential instead of
    // silently creating a duplicate row (the insert result was previously
    // discarded, so a duplicate — or, with the new UNIQUE index, a constraint
    // violation — went unnoticed).
    let already_registered = passkeys::Entity::find()
        .filter(passkeys::Column::CredId.eq(&cred_id_str))
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .is_some();
    if already_registered {
        return (StatusCode::CONFLICT, "This passkey is already registered").into_response();
    }

    let passkey_model = passkeys::ActiveModel {
        user_id: Set(user.id),
        cred_id: Set(cred_id_str),
        cred_public_key: Set(passkey_json),
        counter: Set(0),
        ..Default::default()
    };

    match passkeys::Entity::insert(passkey_model).exec(&state.db).await {
        Ok(_) => (StatusCode::OK, "Passkey registered").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to register passkey").into_response(),
    }
}

/// Begin passkey login. Returns a WebAuthn `RequestChallengeResponse`.
#[utoipa::path(
    post,
    path = "/auth/passkey/login/start",
    responses(
        (status = 200, description = "WebAuthn assertion challenge"),
        (status = 400, description = "User has no registered passkeys"),
        (status = 403, description = "Passkeys are disabled on this instance"),
        (status = 404, description = "User not found"),
    ),
    tag = "Authentication"
)]
pub async fn login_start(
    State(state): State<AppState>,
    Json(payload): Json<LoginStartRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return (StatusCode::FORBIDDEN, "Passkeys are disabled on this instance").into_response();
    }
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.username))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    let user = match user {
        Some(u) => u,
        None => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    // Fetch user's passkeys from DB
    let db_passkeys = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user.id))
        .all(&state.db)
        .await
        .unwrap_or(vec![]);

    if db_passkeys.is_empty() {
        return (StatusCode::BAD_REQUEST, "No passkeys registered for this user").into_response();
    }

    // Convert DB models to webauthn-rs Passkey structs by deserializing
    let allow_credentials: Vec<Passkey> = db_passkeys.iter().filter_map(|pk| {
        serde_json::from_str(&pk.cred_public_key).ok()
    }).collect();

    if allow_credentials.is_empty() {
        return (StatusCode::BAD_REQUEST, "Failed to parse stored passkeys").into_response();
    }

    let webauthn = get_webauthn();
    let (rcr, auth_state) = match webauthn.start_passkey_authentication(&allow_credentials) {
        Ok(res) => res,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to start authentication").into_response(),
    };

    AUTH_STATE.insert(payload.username.clone(), auth_state);

    (StatusCode::OK, Json(LoginStartResponse { options: rcr })).into_response()
}

/// Complete passkey login and issue a JWT on success.
#[utoipa::path(
    post,
    path = "/auth/passkey/login/finish",
    responses(
        (status = 200, description = "Authenticated; JWT issued", body = PasskeyAuthResponse),
        (status = 400, description = "Invalid or expired assertion"),
        (status = 401, description = "Assertion did not verify"),
        (status = 403, description = "Passkeys are disabled on this instance"),
        (status = 404, description = "User not found"),
    ),
    tag = "Authentication"
)]
pub async fn login_finish(
    State(state): State<AppState>,
    Json(payload): Json<LoginFinishRequest>,
) -> impl IntoResponse {
    if !passkeys_enabled() {
        return (StatusCode::FORBIDDEN, "Passkeys are disabled on this instance").into_response();
    }
    let auth_state = match AUTH_STATE.remove(&payload.username) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "Authentication state not found").into_response(),
    };

    let webauthn = get_webauthn();
    let auth_result = match webauthn.finish_passkey_authentication(&payload.credential, &auth_state) {
        Ok(res) => res,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Authentication failed").into_response(),
    };

    // Auth successful. We need to update the counter in the DB.
    let cred_id_str = format!("{:?}", auth_result.cred_id());

    let passkey_db = passkeys::Entity::find()
        .filter(passkeys::Column::CredId.eq(&cred_id_str))
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(pk) = passkey_db {
        // Persist the updated signature counter back into the stored credential
        // blob (the value the verifier actually reads) so WebAuthn clone/replay
        // detection works on subsequent logins - not just the separate counter column.
        let updated_blob = serde_json::from_str::<Passkey>(&pk.cred_public_key)
            .ok()
            .and_then(|mut passkey| {
                passkey.update_credential(&auth_result);
                serde_json::to_string(&passkey).ok()
            });

        let mut active_pk: passkeys::ActiveModel = pk.into();
        if let Some(blob) = updated_blob {
            active_pk.cred_public_key = Set(blob);
        }
        active_pk.counter = Set(auth_result.counter() as i32);
        active_pk.last_used = Set(Some(Utc::now().naive_utc()));
        let _ = active_pk.update(&state.db).await;
    }

    // Fetch user to issue token
    let user = match users::Entity::find()
        .filter(users::Column::Email.eq(&payload.username))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
    {
        Ok(Some(user)) => user,
        _ => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    let token = match create_jwt(user.id, &user.email, user.token_version) {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token").into_response(),
    };

    (StatusCode::OK, Json(PasskeyAuthResponse {
        token,
        email_verified: user.email_verified,
        is_admin: user.is_admin,
    })).into_response()
}

#[derive(Serialize, ToSchema)]
pub struct PasskeyInfo {
    pub id: i32,
    pub name: String,
    pub created_at: String,
    pub last_used: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PasskeyListResponse {
    pub passkeys: Vec<PasskeyInfo>,
}

/// List user's passkeys
#[utoipa::path(
    get,
    path = "/auth/passkeys",
    responses(
        (status = 200, description = "The caller's registered passkeys", body = PasskeyListResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn list_passkeys(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    let user_passkeys = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .all(&state.db)
        .await
        .unwrap_or_default();

    let passkey_list: Vec<PasskeyInfo> = user_passkeys.into_iter().map(|pk| PasskeyInfo {
        id: pk.id,
        name: pk.name.unwrap_or_else(|| format!("Passkey {}", pk.id)),
        created_at: pk.created_at.to_string(),
        last_used: pk.last_used.map(|lu| lu.to_string()),
    }).collect();

    (StatusCode::OK, Json(PasskeyListResponse { passkeys: passkey_list })).into_response()
}

#[derive(Deserialize)]
pub struct DeletePasskeyRequest {
    pub passkey_id: i32,
}

/// Delete a passkey
#[utoipa::path(
    post,
    path = "/auth/passkey/delete",
    responses(
        (status = 200, description = "Passkey deleted"),
        (status = 400, description = "Cannot delete the account's only login method"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Passkey not found"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn delete_passkey(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<DeletePasskeyRequest>,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    // Verify the passkey belongs to the user
    let passkey = passkeys::Entity::find_by_id(payload.passkey_id)
        .filter(passkeys::Column::UserId.eq(user_id))
        .one(&state.db)
        .await
        .unwrap_or(None);

    if passkey.is_none() {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Passkey not found"}))).into_response();
    }

    // Don't allow deleting last passkey if user has no password
    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        if user.password_hash.is_empty() {
            let passkey_count = passkeys::Entity::find()
                .filter(passkeys::Column::UserId.eq(user_id))
                .count(&state.db)
                .await
                .unwrap_or(0);

            if passkey_count <= 1 {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                    "error": "Cannot delete the last passkey when no password is set"
                }))).into_response();
            }
        }
    }

    // Delete the passkey
    let result = passkeys::Entity::delete_by_id(payload.passkey_id)
        .exec(&state.db)
        .await;

    match result {
        Ok(_) => {
            // Revoking an authentication factor must invalidate existing sessions.
            // Bump token_version so JWTs minted before the revoke stop being
            // accepted, matching the documented invariant (a password change /
            // reset / account-delete / passkey-revoke all bump the version).
            if let Some(u) = users::Entity::find_by_id(user_id).one(&state.db).await.unwrap_or(None) {
                let next = u.token_version + 1;
                let mut active: users::ActiveModel = u.into();
                active.token_version = Set(next);
                let _ = active.update(&state.db).await;
            }
            (StatusCode::OK, Json(serde_json::json!({"message": "Passkey deleted successfully"}))).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to delete passkey"}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct RenamePasskeyRequest {
    pub passkey_id: i32,
    pub name: String,
}

/// Rename a passkey
#[utoipa::path(
    post,
    path = "/auth/passkey/rename",
    responses(
        (status = 200, description = "Passkey renamed"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Passkey not found"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn rename_passkey(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<RenamePasskeyRequest>,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    // Verify the passkey belongs to the user
    let passkey = passkeys::Entity::find_by_id(payload.passkey_id)
        .filter(passkeys::Column::UserId.eq(user_id))
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(pk) = passkey {
        let mut active_pk: passkeys::ActiveModel = pk.into();
        active_pk.name = Set(Some(payload.name.clone()));
        
        match active_pk.update(&state.db).await {
            Ok(_) => (StatusCode::OK, Json(serde_json::json!({"message": "Passkey renamed successfully"}))).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to rename passkey"}))).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Passkey not found"}))).into_response()
    }
}
