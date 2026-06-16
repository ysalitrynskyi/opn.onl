use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::entity::api_keys;
use crate::handlers::links::{get_user_id_from_header, hash_api_key};
use crate::AppState;

const MAX_API_KEYS: u64 = 20;

fn api_keys_enabled() -> bool {
    std::env::var("ENABLE_API_KEYS")
        .map(|v| v != "false")
        .unwrap_or(true)
}

#[derive(Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub id: i32,
    pub name: String,
    /// The full secret key — shown ONCE at creation and never again.
    pub key: String,
    pub key_prefix: String,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct ApiKeyInfo {
    pub id: i32,
    pub name: String,
    pub key_prefix: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

/// Create a new personal API key. The full key is returned once.
pub async fn create_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    if !api_keys_enabled() {
        return (StatusCode::FORBIDDEN, "API keys are disabled on this instance").into_response();
    }
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let count = api_keys::Entity::find()
        .filter(api_keys::Column::UserId.eq(user_id))
        .count(&state.db)
        .await
        .unwrap_or(0);
    if count >= MAX_API_KEYS {
        return (
            StatusCode::BAD_REQUEST,
            format!("You can have at most {} API keys", MAX_API_KEYS),
        )
            .into_response();
    }

    let name: String = match payload.name {
        Some(n) if !n.trim().is_empty() => n.trim().chars().take(60).collect(),
        _ => "API key".to_string(),
    };

    // opn_<40 random alphanumerics> — ~238 bits of entropy.
    let random: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect();
    let key = format!("opn_{}", random);
    let key_prefix: String = key.chars().take(12).collect();
    let key_hash = hash_api_key(&key);

    let am = api_keys::ActiveModel {
        user_id: Set(user_id),
        name: Set(name.clone()),
        key_hash: Set(key_hash),
        key_prefix: Set(key_prefix.clone()),
        ..Default::default()
    };
    match am.insert(&state.db).await {
        Ok(rec) => (
            StatusCode::CREATED,
            Json(CreateApiKeyResponse {
                id: rec.id,
                name,
                key,
                key_prefix,
                created_at: rec.created_at.to_string(),
            }),
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create API key").into_response(),
    }
}

/// List the caller's API keys (never returns the secret).
pub async fn list_api_keys(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };
    let keys = api_keys::Entity::find()
        .filter(api_keys::Column::UserId.eq(user_id))
        .order_by_desc(api_keys::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();
    let out: Vec<ApiKeyInfo> = keys
        .into_iter()
        .map(|k| ApiKeyInfo {
            id: k.id,
            name: k.name,
            key_prefix: k.key_prefix,
            last_used_at: k.last_used_at.map(|d| d.to_string()),
            created_at: k.created_at.to_string(),
        })
        .collect();
    (StatusCode::OK, Json(out)).into_response()
}

/// Revoke (delete) one of the caller's API keys.
pub async fn delete_api_key(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };
    let res = api_keys::Entity::delete_many()
        .filter(api_keys::Column::Id.eq(id))
        .filter(api_keys::Column::UserId.eq(user_id))
        .exec(&state.db)
        .await;
    match res {
        Ok(r) if r.rows_affected > 0 => (StatusCode::OK, "API key revoked").into_response(),
        Ok(_) => (StatusCode::NOT_FOUND, "API key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to revoke API key").into_response(),
    }
}
