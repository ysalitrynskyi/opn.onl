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

use crate::entity::{users, links, blocked_links, blocked_domains};
use crate::utils::decode_jwt;
use crate::AppState;

#[derive(Serialize, ToSchema)]
pub struct AdminResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize, ToSchema)]
#[allow(dead_code)]
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
    let admin_id = match require_admin(&state, &headers).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    // SECURITY: Prevent admin from deleting themselves
    if admin_id == user_id {
        return (StatusCode::FORBIDDEN, Json(AdminResponse {
            success: false,
            message: "Cannot delete your own account".to_string(),
        })).into_response();
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
    let admin_id = match require_admin(&state, &headers).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    // SECURITY: Prevent admin from hard deleting themselves
    if admin_id == user_id {
        return (StatusCode::FORBIDDEN, Json(AdminResponse {
            success: false,
            message: "Cannot permanently delete your own account".to_string(),
        })).into_response();
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

/// Remove admin status from a user (admin only)
#[utoipa::path(
    post,
    path = "/admin/users/{user_id}/remove-admin",
    params(
        ("user_id" = i32, Path, description = "User ID to remove admin from")
    ),
    responses(
        (status = 200, description = "Admin status removed", body = AdminResponse),
        (status = 403, description = "Admin access required or cannot demote self"),
        (status = 404, description = "User not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn remove_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> impl IntoResponse {
    let admin_id = match require_admin(&state, &headers).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    // SECURITY: Prevent admin from demoting themselves
    if admin_id == user_id {
        return (StatusCode::FORBIDDEN, Json(AdminResponse {
            success: false,
            message: "Cannot remove your own admin status".to_string(),
        })).into_response();
    }

    let user = users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        if !user.is_admin {
            return (StatusCode::BAD_REQUEST, Json(AdminResponse {
                success: false,
                message: "User is not an admin".to_string(),
            })).into_response();
        }

        let mut active_user: users::ActiveModel = user.into();
        active_user.is_admin = Set(false);
        
        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to update user".to_string(),
            })).into_response();
        }

        return (StatusCode::OK, Json(AdminResponse {
            success: true,
            message: format!("Admin status removed from user {}", user_id),
        })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(AdminResponse {
        success: false,
        message: "User not found".to_string(),
    })).into_response()
}

// ==================== BLOCKED LINKS/DOMAINS ====================

#[derive(Deserialize, ToSchema)]
pub struct BlockLinkRequest {
    pub url: String,
    pub reason: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct BlockDomainRequest {
    pub domain: String,
    pub reason: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct BlockedLinkResponse {
    pub id: i32,
    pub url: String,
    pub reason: Option<String>,
    pub blocked_by: Option<i32>,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct BlockedDomainResponse {
    pub id: i32,
    pub domain: String,
    pub reason: Option<String>,
    pub blocked_by: Option<i32>,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct AdminStatsResponse {
    pub total_users: i64,
    pub active_users: i64,
    pub total_links: i64,
    pub active_links: i64,
    pub total_clicks: i64,
    pub blocked_links_count: i64,
    pub blocked_domains_count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct AdminUserResponse {
    pub id: i32,
    pub email: String,
    pub is_admin: bool,
    pub email_verified: bool,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

/// Get admin dashboard stats
#[utoipa::path(
    get,
    path = "/admin/stats",
    responses(
        (status = 200, description = "Admin stats", body = AdminStatsResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let total_users = users::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let active_users = users::Entity::find()
        .filter(users::Column::DeletedAt.is_null())
        .count(&state.db).await.unwrap_or(0) as i64;
    
    let total_links = links::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let active_links = links::Entity::find()
        .filter(links::Column::DeletedAt.is_null())
        .count(&state.db).await.unwrap_or(0) as i64;
    
    let total_clicks: i64 = links::Entity::find()
        .all(&state.db).await.unwrap_or_default()
        .iter().map(|l| l.click_count as i64).sum();
    
    let blocked_links_count = blocked_links::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let blocked_domains_count = blocked_domains::Entity::find().count(&state.db).await.unwrap_or(0) as i64;

    (StatusCode::OK, Json(AdminStatsResponse {
        total_users,
        active_users,
        total_links,
        active_links,
        total_clicks,
        blocked_links_count,
        blocked_domains_count,
    })).into_response()
}

/// Block a URL (admin only)
#[utoipa::path(
    post,
    path = "/admin/blocked/links",
    request_body = BlockLinkRequest,
    responses(
        (status = 201, description = "URL blocked", body = BlockedLinkResponse),
        (status = 403, description = "Admin access required"),
        (status = 409, description = "URL already blocked"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn block_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BlockLinkRequest>,
) -> impl IntoResponse {
    let admin_id = match require_admin(&state, &headers).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    let existing = blocked_links::Entity::find()
        .filter(blocked_links::Column::Url.eq(&payload.url))
        .one(&state.db)
        .await
        .ok()
        .flatten();
    
    if existing.is_some() {
        return (StatusCode::CONFLICT, Json(AdminResponse {
            success: false,
            message: "URL is already blocked".to_string(),
        })).into_response();
    }

    let blocked = blocked_links::ActiveModel {
        url: Set(payload.url.clone()),
        reason: Set(payload.reason.clone()),
        blocked_by: Set(Some(admin_id)),
        ..Default::default()
    };

    match blocked.insert(&state.db).await {
        Ok(result) => {
            (StatusCode::CREATED, Json(BlockedLinkResponse {
                id: result.id,
                url: result.url,
                reason: result.reason,
                blocked_by: result.blocked_by,
                created_at: result.created_at.to_string(),
            })).into_response()
        }
        Err(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to block URL".to_string(),
            })).into_response()
        }
    }
}

/// Get all blocked links (admin only)
#[utoipa::path(
    get,
    path = "/admin/blocked/links",
    responses(
        (status = 200, description = "List of blocked links", body = Vec<BlockedLinkResponse>),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_blocked_links(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let blocked = blocked_links::Entity::find()
        .order_by_desc(blocked_links::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let responses: Vec<BlockedLinkResponse> = blocked.into_iter().map(|b| BlockedLinkResponse {
        id: b.id,
        url: b.url,
        reason: b.reason,
        blocked_by: b.blocked_by,
        created_at: b.created_at.to_string(),
    }).collect();

    (StatusCode::OK, Json(responses)).into_response()
}

/// Unblock a URL (admin only)
#[utoipa::path(
    delete,
    path = "/admin/blocked/links/{id}",
    params(
        ("id" = i32, Path, description = "Blocked link ID")
    ),
    responses(
        (status = 200, description = "URL unblocked", body = AdminResponse),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Blocked URL not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn unblock_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let result = blocked_links::Entity::delete_by_id(id)
        .exec(&state.db)
        .await;

    match result {
        Ok(res) if res.rows_affected > 0 => {
            (StatusCode::OK, Json(AdminResponse {
                success: true,
                message: "URL unblocked".to_string(),
            })).into_response()
        }
        _ => {
            (StatusCode::NOT_FOUND, Json(AdminResponse {
                success: false,
                message: "Blocked URL not found".to_string(),
            })).into_response()
        }
    }
}

/// Block a domain (admin only)
#[utoipa::path(
    post,
    path = "/admin/blocked/domains",
    request_body = BlockDomainRequest,
    responses(
        (status = 201, description = "Domain blocked", body = BlockedDomainResponse),
        (status = 403, description = "Admin access required"),
        (status = 409, description = "Domain already blocked"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn block_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BlockDomainRequest>,
) -> impl IntoResponse {
    let admin_id = match require_admin(&state, &headers).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    let domain = payload.domain
        .replace("https://", "")
        .replace("http://", "")
        .trim_end_matches('/')
        .to_lowercase();

    let existing = blocked_domains::Entity::find()
        .filter(blocked_domains::Column::Domain.eq(&domain))
        .one(&state.db)
        .await
        .ok()
        .flatten();
    
    if existing.is_some() {
        return (StatusCode::CONFLICT, Json(AdminResponse {
            success: false,
            message: "Domain is already blocked".to_string(),
        })).into_response();
    }

    let blocked = blocked_domains::ActiveModel {
        domain: Set(domain.clone()),
        reason: Set(payload.reason.clone()),
        blocked_by: Set(Some(admin_id)),
        ..Default::default()
    };

    match blocked.insert(&state.db).await {
        Ok(result) => {
            (StatusCode::CREATED, Json(BlockedDomainResponse {
                id: result.id,
                domain: result.domain,
                reason: result.reason,
                blocked_by: result.blocked_by,
                created_at: result.created_at.to_string(),
            })).into_response()
        }
        Err(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to block domain".to_string(),
            })).into_response()
        }
    }
}

/// Get all blocked domains (admin only)
#[utoipa::path(
    get,
    path = "/admin/blocked/domains",
    responses(
        (status = 200, description = "List of blocked domains", body = Vec<BlockedDomainResponse>),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_blocked_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let blocked = blocked_domains::Entity::find()
        .order_by_desc(blocked_domains::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let responses: Vec<BlockedDomainResponse> = blocked.into_iter().map(|b| BlockedDomainResponse {
        id: b.id,
        domain: b.domain,
        reason: b.reason,
        blocked_by: b.blocked_by,
        created_at: b.created_at.to_string(),
    }).collect();

    (StatusCode::OK, Json(responses)).into_response()
}

/// Unblock a domain (admin only)
#[utoipa::path(
    delete,
    path = "/admin/blocked/domains/{id}",
    params(
        ("id" = i32, Path, description = "Blocked domain ID")
    ),
    responses(
        (status = 200, description = "Domain unblocked", body = AdminResponse),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Blocked domain not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn unblock_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let result = blocked_domains::Entity::delete_by_id(id)
        .exec(&state.db)
        .await;

    match result {
        Ok(res) if res.rows_affected > 0 => {
            (StatusCode::OK, Json(AdminResponse {
                success: true,
                message: "Domain unblocked".to_string(),
            })).into_response()
        }
        _ => {
            (StatusCode::NOT_FOUND, Json(AdminResponse {
                success: false,
                message: "Blocked domain not found".to_string(),
            })).into_response()
        }
    }
}

/// Get all users (admin only)
#[utoipa::path(
    get,
    path = "/admin/users",
    responses(
        (status = 200, description = "List of users", body = Vec<AdminUserResponse>),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_all_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let users_list = users::Entity::find()
        .order_by_desc(users::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let responses: Vec<AdminUserResponse> = users_list.into_iter().map(|u| AdminUserResponse {
        id: u.id,
        email: u.email,
        is_admin: u.is_admin,
        email_verified: u.email_verified,
        created_at: u.created_at.to_string(),
        deleted_at: u.deleted_at.map(|d| d.to_string()),
    }).collect();

    (StatusCode::OK, Json(responses)).into_response()
}
