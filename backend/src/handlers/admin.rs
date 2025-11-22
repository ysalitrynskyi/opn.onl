use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use sea_orm::*;
use sea_orm::sea_query::Expr;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::entity::{users, links};
use crate::utils::decode_jwt;
use crate::AppState;

#[derive(Serialize, ToSchema)]
pub struct AdminResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteUserRequest {
    pub user_id: i32,
    pub hard_delete: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct BackupResponse {
    pub success: bool,
    pub filename: Option<String>,
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct BackupListResponse {
    pub backups: Vec<String>,
}

/// Check if user is admin
async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<i32, (StatusCode, Json<AdminResponse>)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let token = auth_header.ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(AdminResponse {
            success: false,
            message: "Unauthorized".to_string(),
        }))
    })?;

    let claims = decode_jwt(token).map_err(|_| {
        (StatusCode::UNAUTHORIZED, Json(AdminResponse {
            success: false,
            message: "Invalid token".to_string(),
        }))
    })?;

    // Check if user is admin
    let user = users::Entity::find_by_id(claims.user_id)
        .one(&state.db)
        .await
        .map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Database error".to_string(),
            }))
        })?
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(AdminResponse {
                success: false,
                message: "User not found".to_string(),
            }))
        })?;

    if !user.is_admin {
        return Err((StatusCode::FORBIDDEN, Json(AdminResponse {
            success: false,
            message: "Admin access required".to_string(),
        })));
    }

    Ok(claims.user_id)
}

/// Soft delete a user (admin only)
#[utoipa::path(
    delete,
    path = "/admin/users/{user_id}",
    params(
        ("user_id" = i32, Path, description = "User ID to delete")
    ),
    responses(
        (status = 200, description = "User deleted successfully", body = AdminResponse),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        if user.deleted_at.is_some() {
            return (StatusCode::BAD_REQUEST, Json(AdminResponse {
                success: false,
                message: "User already deleted".to_string(),
            })).into_response();
        }

        // Soft delete user
        let mut active_user: users::ActiveModel = user.into();
        active_user.deleted_at = Set(Some(Utc::now().naive_utc()));
        
        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to delete user".to_string(),
            })).into_response();
        }

        // Soft delete all user's links
        links::Entity::update_many()
            .col_expr(links::Column::DeletedAt, Expr::value(Utc::now().naive_utc()))
            .filter(links::Column::UserId.eq(user_id))
            .filter(links::Column::DeletedAt.is_null())
            .exec(&state.db)
            .await
            .ok();

        return (StatusCode::OK, Json(AdminResponse {
            success: true,
            message: format!("User {} soft deleted", user_id),
        })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(AdminResponse {
        success: false,
        message: "User not found".to_string(),
    })).into_response()
}

/// Hard delete a user (admin only, use with caution)
#[utoipa::path(
    delete,
    path = "/admin/users/{user_id}/hard",
    params(
        ("user_id" = i32, Path, description = "User ID to permanently delete")
    ),
    responses(
        (status = 200, description = "User permanently deleted", body = AdminResponse),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn hard_delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    // First delete all user's links and associated data
    // This uses cascade delete for click_events and link_tags
    links::Entity::delete_many()
        .filter(links::Column::UserId.eq(user_id))
        .exec(&state.db)
        .await
        .ok();

    // Then delete the user
    let result = users::Entity::delete_by_id(user_id)
        .exec(&state.db)
        .await;

    match result {
        Ok(res) if res.rows_affected > 0 => {
            (StatusCode::OK, Json(AdminResponse {
                success: true,
                message: format!("User {} permanently deleted with all associated data", user_id),
            })).into_response()
        }
        _ => {
            (StatusCode::NOT_FOUND, Json(AdminResponse {
                success: false,
                message: "User not found".to_string(),
            })).into_response()
        }
    }
}

/// Restore a soft-deleted user (admin only)
#[utoipa::path(
    post,
    path = "/admin/users/{user_id}/restore",
    params(
        ("user_id" = i32, Path, description = "User ID to restore")
    ),
    responses(
        (status = 200, description = "User restored successfully", body = AdminResponse),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn restore_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        if user.deleted_at.is_none() {
            return (StatusCode::BAD_REQUEST, Json(AdminResponse {
                success: false,
                message: "User is not deleted".to_string(),
            })).into_response();
        }

        // Restore user
        let mut active_user: users::ActiveModel = user.into();
        active_user.deleted_at = Set(None);
        
        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to restore user".to_string(),
            })).into_response();
        }

        // Restore all user's links
        links::Entity::update_many()
            .col_expr(links::Column::DeletedAt, Expr::value(Option::<chrono::NaiveDateTime>::None))
            .filter(links::Column::UserId.eq(user_id))
            .exec(&state.db)
            .await
            .ok();

        return (StatusCode::OK, Json(AdminResponse {
            success: true,
            message: format!("User {} restored", user_id),
        })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(AdminResponse {
        success: false,
        message: "User not found".to_string(),
    })).into_response()
}

/// Create a database backup (admin only)
#[utoipa::path(
    post,
    path = "/admin/backup",
    responses(
        (status = 200, description = "Backup created successfully", body = BackupResponse),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Backup failed"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn create_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    if !state.backup.is_configured() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(BackupResponse {
            success: false,
            filename: None,
            message: "Backup service not configured".to_string(),
        })).into_response();
    }

    match state.backup.create_backup().await {
        Ok(filename) => {
            (StatusCode::OK, Json(BackupResponse {
                success: true,
                filename: Some(filename),
                message: "Backup created successfully".to_string(),
            })).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(BackupResponse {
                success: false,
                filename: None,
                message: format!("Backup failed: {}", e),
            })).into_response()
        }
    }
}

/// List available backups (admin only)
#[utoipa::path(
    get,
    path = "/admin/backup",
    responses(
        (status = 200, description = "List of backups", body = BackupListResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn list_backups(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    if !state.backup.is_configured() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(AdminResponse {
            success: false,
            message: "Backup service not configured".to_string(),
        })).into_response();
    }

    match state.backup.list_backups().await {
        Ok(backups) => {
            (StatusCode::OK, Json(BackupListResponse { backups })).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: format!("Failed to list backups: {}", e),
            })).into_response()
        }
    }
}

/// Clean up old backups (admin only)
#[utoipa::path(
    delete,
    path = "/admin/backup/cleanup/{keep_count}",
    params(
        ("keep_count" = usize, Path, description = "Number of recent backups to keep")
    ),
    responses(
        (status = 200, description = "Old backups cleaned up", body = AdminResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn cleanup_backups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(keep_count): Path<usize>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    if !state.backup.is_configured() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(AdminResponse {
            success: false,
            message: "Backup service not configured".to_string(),
        })).into_response();
    }

    match state.backup.cleanup_old_backups(keep_count).await {
        Ok(deleted) => {
            (StatusCode::OK, Json(AdminResponse {
                success: true,
                message: format!("Cleaned up {} old backups", deleted),
            })).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: format!("Cleanup failed: {}", e),
            })).into_response()
        }
    }
}

/// Make a user an admin (admin only)
#[utoipa::path(
    post,
    path = "/admin/users/{user_id}/make-admin",
    params(
        ("user_id" = i32, Path, description = "User ID to make admin")
    ),
    responses(
        (status = 200, description = "User is now admin", body = AdminResponse),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn make_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let user = users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        let mut active_user: users::ActiveModel = user.into();
        active_user.is_admin = Set(true);
        
        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to update user".to_string(),
            })).into_response();
        }

        return (StatusCode::OK, Json(AdminResponse {
            success: true,
            message: format!("User {} is now an admin", user_id),
        })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(AdminResponse {
        success: false,
        message: "User not found".to_string(),
    })).into_response()
}

