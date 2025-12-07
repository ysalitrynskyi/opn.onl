use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
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

static REG_STATE: Lazy<Mutex<HashMap<String, PasskeyRegistration>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static AUTH_STATE: Lazy<Mutex<HashMap<String, PasskeyAuthentication>>> = Lazy::new(|| Mutex::new(HashMap::new()));

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
    
    let origin_url = match Url::parse(&rp_origin) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Invalid FRONTEND_URL for WebAuthn: {}", e);
            // Fall back to localhost for development
            Url::parse("http://localhost:5173").unwrap()
        }
    };
    
    match WebauthnBuilder::new(&rp_id, &origin_url) {
        Ok(builder) => match builder.build() {
            Ok(webauthn) => webauthn,
            Err(e) => {
                tracing::error!("Failed to build WebAuthn: {:?}", e);
                // Fallback to localhost
                let fallback_url = Url::parse("http://localhost:5173").unwrap();
                WebauthnBuilder::new("localhost", &fallback_url)
                    .unwrap()
                    .build()
                    .unwrap()
            }
        },
        Err(e) => {
            tracing::error!("Failed to create WebAuthn builder: {:?}", e);
            // Fallback to localhost
            let fallback_url = Url::parse("http://localhost:5173").unwrap();
            WebauthnBuilder::new("localhost", &fallback_url)
                .unwrap()
                .build()
                .unwrap()
        }
    }
}

#[derive(Deserialize)]
pub struct RegisterStartRequest {
    pub username: String,
}

#[derive(Serialize)]
pub struct RegisterStartResponse {
    pub options: CreationChallengeResponse,
}

#[derive(Deserialize)]
pub struct RegisterFinishRequest {
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

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
}

pub async fn register_start(
    State(state): State<AppState>,
    Json(payload): Json<RegisterStartRequest>,
) -> impl IntoResponse {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.username))
        .one(&state.db)
        .await
        .unwrap_or(None);

    let user = match user {
        Some(u) => u,
        None => return (StatusCode::NOT_FOUND, "User not found").into_response(),
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

    REG_STATE.lock().unwrap().insert(payload.username.clone(), reg_state);

    (StatusCode::OK, Json(RegisterStartResponse { options: ccr })).into_response()
}

pub async fn register_finish(
    State(state): State<AppState>,
    Json(payload): Json<RegisterFinishRequest>,
) -> impl IntoResponse {
    let reg_state = match REG_STATE.lock().unwrap().remove(&payload.username) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, "Registration state not found").into_response(),
    };

    let webauthn = get_webauthn();
    let passkey = match webauthn.finish_passkey_registration(&payload.credential, &reg_state) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to finish registration").into_response(),
    };

    let user = match users::Entity::find()
        .filter(users::Column::Email.eq(&payload.username))
        .one(&state.db)
        .await 
    {
        Ok(Some(user)) => user,
        _ => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    // Save passkey to DB - serialize the passkey for storage
    let passkey_json = serde_json::to_string(&passkey).unwrap_or_default();
    let cred_id_str = format!("{:?}", passkey.cred_id());

    let passkey_model = passkeys::ActiveModel {
        user_id: Set(user.id),
        cred_id: Set(cred_id_str),
        cred_public_key: Set(passkey_json),
        counter: Set(0),
        ..Default::default()
    };

    let _ = passkeys::Entity::insert(passkey_model).exec(&state.db).await;

    (StatusCode::OK, "Passkey registered").into_response()
}

pub async fn login_start(
    State(state): State<AppState>,
    Json(payload): Json<LoginStartRequest>,
) -> impl IntoResponse {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.username))
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

    AUTH_STATE.lock().unwrap().insert(payload.username.clone(), auth_state);

    (StatusCode::OK, Json(LoginStartResponse { options: rcr })).into_response()
}

pub async fn login_finish(
    State(state): State<AppState>,
    Json(payload): Json<LoginFinishRequest>,
) -> impl IntoResponse {
    let auth_state = match AUTH_STATE.lock().unwrap().remove(&payload.username) {
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
        let mut active_pk: passkeys::ActiveModel = pk.into();
        active_pk.counter = Set(auth_result.counter() as i32);
        active_pk.last_used = Set(Some(Utc::now().naive_utc()));
        let _ = active_pk.update(&state.db).await;
    }

    // Fetch user to issue token
    let user = match users::Entity::find()
        .filter(users::Column::Email.eq(&payload.username))
        .one(&state.db)
        .await 
    {
        Ok(Some(user)) => user,
        _ => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    let token = match create_jwt(user.id, &user.email) {
        Ok(t) => t,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token").into_response(),
    };

    (StatusCode::OK, Json(AuthResponse { token })).into_response()
}
