use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use sea_orm::*;
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::sea_query::Expr;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

use crate::entity::{
    api_keys, blocked_domains, blocked_links, click_events, links, org_members, organizations,
    passkeys, users,
};
use crate::utils::decode_jwt;
use crate::AppState;

/// Clamp pagination params: 1-based page, 1..=100 per_page (default 25).
fn clamp_pagination(page: Option<u64>, per_page: Option<u64>) -> (u64, u64) {
    let page = page.unwrap_or(1).max(1);
    let per_page = per_page.unwrap_or(25).clamp(1, 100);
    (page, per_page)
}

/// Escape LIKE/ILIKE wildcards in user-supplied search text and wrap it for a
/// substring match.
fn ilike_pattern(search: &str) -> String {
    let escaped = search
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

/// A `WHERE` fragment matching links whose destination is suspicious — a
/// dangerous file extension at the end of the path, or a raw IPv4/bracketed
/// IPv6 host. Built entirely from the static extension list (no user input), so
/// embedding it as raw SQL is injection-safe. Kept in SQL so the "suspicious
/// only" filter and the suspicious count both stay correct under pagination
/// instead of loading every row to test in Rust.
fn suspicious_sql_condition() -> Condition {
    let alt = crate::utils::url_policy::dangerous_extensions().join("|");
    // Case-insensitive: dangerous extension inside the PATH (after the host's
    // first '/'), at a segment boundary. Anchoring to the path is essential —
    // otherwise a plain `.com` / `.run` domain in the host would match.
    let ext_re = format!(
        "links.original_url ~* '^https?://[^/?#]+/[^?#]*\\.({})($|[?#/])'",
        alt
    );
    // Case-insensitive: host is a bare IPv4 or bracketed IPv6 literal.
    let ip_re = "links.original_url ~* '^https?://(\\[[0-9a-f:]+\\]|[0-9]{1,3}(\\.[0-9]{1,3}){3})([:/?#]|$)'";
    Condition::any()
        .add(Expr::cust(ext_re))
        .add(Expr::cust(ip_re))
}

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

    // Check if user is admin. Exclude soft-deleted users so a deleted admin's
    // still-valid token cannot keep authorizing /admin/* actions.
    let user = users::Entity::find_by_id(claims.user_id)
        .filter(users::Column::DeletedAt.is_null())
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

    // Honor JWT revocation on the admin surface too. A password change/reset
    // bumps the user's token_version to invalidate outstanding JWTs; every other
    // auth path checks this (see get_user_id_from_header). Without it a stolen or
    // pre-reset admin token would keep authorizing /admin/* actions for the full
    // token lifetime even after the admin reset their password to lock it out.
    if user.token_version != claims.token_version {
        return Err((StatusCode::UNAUTHORIZED, Json(AdminResponse {
            success: false,
            message: "Token has been revoked".to_string(),
        })));
    }

    if !user.is_admin {
        return Err((StatusCode::FORBIDDEN, Json(AdminResponse {
            success: false,
            message: "Admin access required".to_string(),
        })));
    }

    Ok(claims.user_id)
}

/// Purge any cached redirect entries pointing at a now-blocked exact URL, so the
/// block takes effect immediately instead of after the cache TTL.
async fn invalidate_cache_for_url(state: &AppState, url: &str) {
    let cache = match &state.redis_cache {
        Some(c) => c,
        None => return,
    };
    let matches = links::Entity::find()
        .filter(links::Column::OriginalUrl.eq(url))
        .filter(links::Column::DeletedAt.is_null())
        .all(&state.db)
        .await
        .unwrap_or_default();
    for l in matches {
        let _ = cache.invalidate_link(&l.code).await;
    }
}

/// Purge cached redirect entries whose target host matches a now-blocked domain
/// (or a subdomain of it), so the block takes effect immediately.
async fn invalidate_cache_for_domain(state: &AppState, domain: &str) {
    let cache = match &state.redis_cache {
        Some(c) => c,
        None => return,
    };
    let all = links::Entity::find()
        .filter(links::Column::DeletedAt.is_null())
        .all(&state.db)
        .await
        .unwrap_or_default();
    for l in all {
        if let Ok(u) = url::Url::parse(&l.original_url) {
            if let Some(h) = u.host_str() {
                let h = h.trim_end_matches('.').to_lowercase();
                if h == domain || h.ends_with(&format!(".{}", domain)) {
                    let _ = cache.invalidate_link(&l.code).await;
                }
            }
        }
    }
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
        (status = 409, description = "User still owns organizations with other members"),
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

        // Refuse to delete an org owner while other members depend on the
        // org; ownership must be transferred (or the org deleted) first.
        match crate::handlers::organizations::split_owned_orgs(&state.db, user_id).await {
            Ok(split) if !split.blocking.is_empty() => {
                let slugs: Vec<&str> = split.blocking.iter().map(|o| o.slug.as_str()).collect();
                return (StatusCode::CONFLICT, Json(AdminResponse {
                    success: false,
                    message: format!(
                        "User {} still owns organizations with other members: {}. Transfer ownership or delete them first.",
                        user_id,
                        slugs.join(", ")
                    ),
                })).into_response();
            }
            Ok(_) => {}
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                    success: false,
                    message: "Failed to delete user".to_string(),
                })).into_response();
            }
        }

        // Soft delete user
        let mut active_user: users::ActiveModel = user.into();
        active_user.deleted_at = Set(Some(Utc::now().naive_utc()));
        
        if active_user.update(&state.db).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to delete user".to_string(),
            })).into_response();
        }

        // Capture codes before the soft-delete so their cached redirects can be
        // dropped afterwards (soft-delete is an UPDATE — nothing else clears the
        // cache, so a deleted user's links would keep redirecting until the TTL).
        let cached_codes = crate::handlers::links::active_link_codes_for_user(&state, user_id).await;

        // Soft delete all user's links
        links::Entity::update_many()
            .col_expr(links::Column::DeletedAt, Expr::value(Utc::now().naive_utc()))
            .filter(links::Column::UserId.eq(user_id))
            .filter(links::Column::DeletedAt.is_null())
            .exec(&state.db)
            .await
            .ok();

        crate::handlers::links::invalidate_cached_codes(&state, &cached_codes).await;

        // Remove the user's passkeys. Soft-delete is an UPDATE so the FK cascade
        // never fires; without this a deleted account could still re-authenticate
        // via WebAuthn and mint a fresh token.
        crate::entity::passkeys::Entity::delete_many()
            .filter(crate::entity::passkeys::Column::UserId.eq(user_id))
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
        (status = 409, description = "User still owns organizations with other members"),
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

    let txn = match state.db.begin().await {
        Ok(txn) => txn,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to delete user".to_string(),
            })).into_response();
        }
    };

    // Orgs the user owns: ones with other members block the deletion
    // (ownership must be transferred first); solo orgs die with the account.
    // Checked inside the transaction so a concurrent invite/transfer can't
    // slip between check and delete; the owner_id ON DELETE RESTRICT FK is
    // the final backstop either way.
    let split = match crate::handlers::organizations::split_owned_orgs(&txn, user_id).await {
        Ok(split) => split,
        Err(_) => {
            let _ = txn.rollback().await;
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to delete user".to_string(),
            })).into_response();
        }
    };

    if !split.blocking.is_empty() {
        let _ = txn.rollback().await;
        let slugs: Vec<&str> = split.blocking.iter().map(|o| o.slug.as_str()).collect();
        return (StatusCode::CONFLICT, Json(AdminResponse {
            success: false,
            message: format!(
                "User {} still owns organizations with other members: {}. Transfer ownership or delete them first.",
                user_id,
                slugs.join(", ")
            ),
        })).into_response();
    }

    let result = async {
        for org in &split.solo {
            crate::handlers::organizations::purge_organization(&txn, org.id).await?;
        }

        // Delete all user's links and associated data
        // (cascade delete handles click_events and link_tags)
        links::Entity::delete_many()
            .filter(links::Column::UserId.eq(user_id))
            .exec(&txn)
            .await?;

        users::Entity::delete_by_id(user_id).exec(&txn).await
    }
    .await;

    match result {
        Ok(res) if res.rows_affected > 0 => {
            if txn.commit().await.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                    success: false,
                    message: "Failed to delete user".to_string(),
                })).into_response();
            }
            (StatusCode::OK, Json(AdminResponse {
                success: true,
                message: format!("User {} permanently deleted with all associated data", user_id),
            })).into_response()
        }
        Ok(_) => {
            let _ = txn.rollback().await;
            (StatusCode::NOT_FOUND, Json(AdminResponse {
                success: false,
                message: "User not found".to_string(),
            })).into_response()
        }
        Err(_) => {
            let _ = txn.rollback().await;
            // Most likely the owner_id RESTRICT FK: the user (re)gained an
            // organization concurrently. Surface it as a conflict rather
            // than a generic failure.
            (StatusCode::CONFLICT, Json(AdminResponse {
                success: false,
                message: format!(
                    "Failed to delete user {}: the user may still own an organization. Transfer ownership and retry.",
                    user_id
                ),
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
        
        if active_user.update(&state.db).await.is_err() {
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
        
        if active_user.update(&state.db).await.is_err() {
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
        
        if active_user.update(&state.db).await.is_err() {
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
    pub verified_users: i64,
    pub admin_users: i64,
    pub total_links: i64,
    pub active_links: i64,
    pub total_clicks: i64,
    pub total_orgs: i64,
    pub users_today: i64,
    pub links_today: i64,
    pub clicks_today: i64,
    pub blocked_links_count: i64,
    pub blocked_domains_count: i64,
    /// Live (non-deleted) links whose destination trips the abuse heuristic.
    pub suspicious_links_count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct AdminUserResponse {
    pub id: i32,
    pub email: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
    pub email_verified: bool,
    pub created_at: String,
    pub deleted_at: Option<String>,
    pub bio_username: Option<String>,
    pub bio_enabled: bool,
    pub links_count: i64,
    pub total_clicks: i64,
    pub api_keys_count: i64,
    pub passkeys_count: i64,
    pub orgs_owned: i64,
}

#[derive(Serialize, ToSchema)]
pub struct AdminUsersListResponse {
    pub users: Vec<AdminUserResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

#[derive(Deserialize, IntoParams)]
pub struct AdminUsersQuery {
    /// 1-based page number (default 1)
    pub page: Option<u64>,
    /// Items per page, 1-100 (default 25)
    pub per_page: Option<u64>,
    /// Substring match on email, display name, or bio username
    pub search: Option<String>,
    /// Filter: all (default) | active | deleted | admins | unverified
    pub status: Option<String>,
    /// Sort order for created_at: desc (default) | asc
    pub order: Option<String>,
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

    let day_ago = (Utc::now() - Duration::hours(24)).naive_utc();

    let total_users = users::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let active_users = users::Entity::find()
        .filter(users::Column::DeletedAt.is_null())
        .count(&state.db).await.unwrap_or(0) as i64;
    let verified_users = users::Entity::find()
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::EmailVerified.eq(true))
        .count(&state.db).await.unwrap_or(0) as i64;
    let admin_users = users::Entity::find()
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::IsAdmin.eq(true))
        .count(&state.db).await.unwrap_or(0) as i64;
    let users_today = users::Entity::find()
        .filter(users::Column::CreatedAt.gte(day_ago))
        .count(&state.db).await.unwrap_or(0) as i64;

    let total_links = links::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let active_links = links::Entity::find()
        .filter(links::Column::DeletedAt.is_null())
        .count(&state.db).await.unwrap_or(0) as i64;
    let links_today = links::Entity::find()
        .filter(links::Column::CreatedAt.gte(day_ago))
        .count(&state.db).await.unwrap_or(0) as i64;

    // Aggregate in SQL — loading every link row to sum click counts does not
    // survive a table with millions of rows.
    let total_clicks: i64 = links::Entity::find()
        .select_only()
        .column_as(links::Column::ClickCount.sum(), "total")
        .into_tuple::<Option<i64>>()
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or(0);

    let clicks_today = click_events::Entity::find()
        .filter(click_events::Column::CreatedAt.gte(day_ago))
        .count(&state.db).await.unwrap_or(0) as i64;

    let total_orgs = organizations::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let blocked_links_count = blocked_links::Entity::find().count(&state.db).await.unwrap_or(0) as i64;
    let blocked_domains_count = blocked_domains::Entity::find().count(&state.db).await.unwrap_or(0) as i64;

    let suspicious_links_count = links::Entity::find()
        .filter(links::Column::DeletedAt.is_null())
        .filter(suspicious_sql_condition())
        .count(&state.db).await.unwrap_or(0) as i64;

    (StatusCode::OK, Json(AdminStatsResponse {
        total_users,
        active_users,
        verified_users,
        admin_users,
        total_links,
        active_links,
        total_clicks,
        total_orgs,
        users_today,
        links_today,
        clicks_today,
        blocked_links_count,
        blocked_domains_count,
        suspicious_links_count,
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
            // Make the block retroactive for any already-cached redirects.
            invalidate_cache_for_url(&state, &payload.url).await;
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
            // Make the block retroactive for any already-cached redirects.
            invalidate_cache_for_domain(&state, &domain).await;
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

/// Get all users with per-user aggregates (admin only)
#[utoipa::path(
    get,
    path = "/admin/users",
    params(AdminUsersQuery),
    responses(
        (status = 200, description = "Paginated list of users with link/click aggregates", body = AdminUsersListResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_all_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminUsersQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let (page, per_page) = clamp_pagination(query.page, query.per_page);

    let mut finder = users::Entity::find();

    match query.status.as_deref() {
        Some("active") => finder = finder.filter(users::Column::DeletedAt.is_null()),
        Some("deleted") => finder = finder.filter(users::Column::DeletedAt.is_not_null()),
        Some("admins") => {
            finder = finder
                .filter(users::Column::IsAdmin.eq(true))
                .filter(users::Column::DeletedAt.is_null())
        }
        Some("unverified") => {
            finder = finder
                .filter(users::Column::EmailVerified.eq(false))
                .filter(users::Column::DeletedAt.is_null())
        }
        _ => {}
    }

    if let Some(search) = query.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = ilike_pattern(search);
        finder = finder.filter(
            Condition::any()
                .add(Expr::col((users::Entity, users::Column::Email)).ilike(pattern.clone()))
                .add(Expr::col((users::Entity, users::Column::DisplayName)).ilike(pattern.clone()))
                .add(Expr::col((users::Entity, users::Column::BioUsername)).ilike(pattern)),
        );
    }

    finder = match query.order.as_deref() {
        Some("asc") => finder.order_by_asc(users::Column::CreatedAt),
        _ => finder.order_by_desc(users::Column::CreatedAt),
    };

    let paginator = finder.paginate(&state.db, per_page);
    let total = paginator.num_items().await.unwrap_or(0);
    let users_page = paginator.fetch_page(page - 1).await.unwrap_or_default();

    // Aggregates for just the users on this page — grouped queries instead of
    // per-user lookups.
    let user_ids: Vec<i32> = users_page.iter().map(|u| u.id).collect();

    let mut link_stats: HashMap<i32, (i64, i64)> = HashMap::new();
    let mut api_key_counts: HashMap<i32, i64> = HashMap::new();
    let mut passkey_counts: HashMap<i32, i64> = HashMap::new();
    let mut orgs_owned_counts: HashMap<i32, i64> = HashMap::new();

    if !user_ids.is_empty() {
        let rows: Vec<(Option<i32>, i64, Option<i64>)> = links::Entity::find()
            .select_only()
            .column(links::Column::UserId)
            .column_as(links::Column::Id.count(), "links_count")
            .column_as(links::Column::ClickCount.sum(), "clicks")
            .filter(links::Column::UserId.is_in(user_ids.clone()))
            .filter(links::Column::DeletedAt.is_null())
            .group_by(links::Column::UserId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        for (user_id, count, clicks) in rows {
            if let Some(user_id) = user_id {
                link_stats.insert(user_id, (count, clicks.unwrap_or(0)));
            }
        }

        let rows: Vec<(i32, i64)> = api_keys::Entity::find()
            .select_only()
            .column(api_keys::Column::UserId)
            .column_as(api_keys::Column::Id.count(), "count")
            .filter(api_keys::Column::UserId.is_in(user_ids.clone()))
            .group_by(api_keys::Column::UserId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        api_key_counts.extend(rows);

        let rows: Vec<(i32, i64)> = passkeys::Entity::find()
            .select_only()
            .column(passkeys::Column::UserId)
            .column_as(passkeys::Column::Id.count(), "count")
            .filter(passkeys::Column::UserId.is_in(user_ids.clone()))
            .group_by(passkeys::Column::UserId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        passkey_counts.extend(rows);

        let rows: Vec<(i32, i64)> = organizations::Entity::find()
            .select_only()
            .column(organizations::Column::OwnerId)
            .column_as(organizations::Column::Id.count(), "count")
            .filter(organizations::Column::OwnerId.is_in(user_ids.clone()))
            .group_by(organizations::Column::OwnerId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        orgs_owned_counts.extend(rows);
    }

    let responses: Vec<AdminUserResponse> = users_page.into_iter().map(|u| {
        let (links_count, total_clicks) = link_stats.get(&u.id).copied().unwrap_or((0, 0));
        AdminUserResponse {
            id: u.id,
            email: u.email,
            display_name: u.display_name,
            is_admin: u.is_admin,
            email_verified: u.email_verified,
            created_at: u.created_at.to_string(),
            deleted_at: u.deleted_at.map(|d| d.to_string()),
            bio_username: u.bio_username,
            bio_enabled: u.bio_enabled,
            links_count,
            total_clicks,
            api_keys_count: api_key_counts.get(&u.id).copied().unwrap_or(0),
            passkeys_count: passkey_counts.get(&u.id).copied().unwrap_or(0),
            orgs_owned: orgs_owned_counts.get(&u.id).copied().unwrap_or(0),
        }
    }).collect();

    (StatusCode::OK, Json(AdminUsersListResponse {
        users: responses,
        total,
        page,
        per_page,
    })).into_response()
}

// ==================== ADMIN: ALL LINKS ====================

#[derive(Deserialize, IntoParams)]
pub struct AdminLinksQuery {
    /// 1-based page number (default 1)
    pub page: Option<u64>,
    /// Items per page, 1-100 (default 25)
    pub per_page: Option<u64>,
    /// Substring match on code, destination URL, title, or owner email
    pub search: Option<String>,
    /// Filter: all (default) | live | deleted
    pub status: Option<String>,
    /// Only links belonging to this user
    pub user_id: Option<i32>,
    /// Sort key: created (default) | clicks
    pub sort: Option<String>,
    /// Sort order: desc (default) | asc
    pub order: Option<String>,
    /// When true, return only links flagged suspicious (dangerous file type or
    /// raw-IP host). Applied after the DB query since the heuristic is computed
    /// in Rust.
    pub suspicious: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminLinkResponse {
    pub id: i32,
    pub code: String,
    pub original_url: String,
    pub title: Option<String>,
    pub user_id: Option<i32>,
    pub user_email: Option<String>,
    pub org_id: Option<i32>,
    pub folder_id: Option<i32>,
    pub click_count: i32,
    pub max_clicks: Option<i32>,
    pub created_at: String,
    pub starts_at: Option<String>,
    pub expires_at: Option<String>,
    pub deleted_at: Option<String>,
    pub burned_at: Option<String>,
    pub is_pinned: bool,
    pub burn_after_reading: bool,
    pub safe_link_interstitial: bool,
    pub bio_visible: bool,
    pub has_password: bool,
    pub is_active: bool,
    pub inactive_reason: Option<String>,
    /// True when the destination trips an abuse heuristic (dangerous file type
    /// or raw-IP host). Computed live, independent of the creation-time guard.
    pub suspicious: bool,
    /// Human-readable reason the link is flagged, when `suspicious`.
    pub suspicion_reason: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminLinksListResponse {
    pub links: Vec<AdminLinkResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

/// List every link in the system, across all users (admin only)
#[utoipa::path(
    get,
    path = "/admin/links",
    params(AdminLinksQuery),
    responses(
        (status = 200, description = "Paginated list of all links with owner info", body = AdminLinksListResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_all_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminLinksQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let (page, per_page) = clamp_pagination(query.page, query.per_page);

    // Join the owner so search can match their email and the response can
    // show who a link belongs to.
    let mut finder = links::Entity::find().find_also_related(users::Entity);

    match query.status.as_deref() {
        Some("live") => finder = finder.filter(links::Column::DeletedAt.is_null()),
        Some("deleted") => finder = finder.filter(links::Column::DeletedAt.is_not_null()),
        _ => {}
    }

    if let Some(user_id) = query.user_id {
        finder = finder.filter(links::Column::UserId.eq(user_id));
    }

    if query.suspicious == Some(true) {
        finder = finder.filter(suspicious_sql_condition());
    }

    if let Some(search) = query.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = ilike_pattern(search);
        finder = finder.filter(
            Condition::any()
                .add(Expr::col((links::Entity, links::Column::Code)).ilike(pattern.clone()))
                .add(Expr::col((links::Entity, links::Column::OriginalUrl)).ilike(pattern.clone()))
                .add(Expr::col((links::Entity, links::Column::Title)).ilike(pattern.clone()))
                .add(Expr::col((users::Entity, users::Column::Email)).ilike(pattern)),
        );
    }

    let descending = !matches!(query.order.as_deref(), Some("asc"));
    finder = match (query.sort.as_deref(), descending) {
        (Some("clicks"), true) => finder.order_by_desc(links::Column::ClickCount),
        (Some("clicks"), false) => finder.order_by_asc(links::Column::ClickCount),
        (_, false) => finder.order_by_asc(links::Column::CreatedAt),
        (_, true) => finder.order_by_desc(links::Column::CreatedAt),
    };

    let paginator = finder.paginate(&state.db, per_page);
    let total = paginator.num_items().await.unwrap_or(0);
    let rows = paginator.fetch_page(page - 1).await.unwrap_or_default();

    let responses: Vec<AdminLinkResponse> = rows.into_iter().map(|(link, owner)| {
        let is_active = link.is_active();
        let inactive_reason = link.inactive_reason().map(str::to_string);
        let suspicion_reason = crate::utils::url_policy::suspicion_reason(&link.original_url);
        AdminLinkResponse {
            id: link.id,
            code: link.code,
            original_url: link.original_url,
            title: link.title,
            user_id: link.user_id,
            user_email: owner.map(|u| u.email),
            org_id: link.org_id,
            folder_id: link.folder_id,
            click_count: link.click_count,
            max_clicks: link.max_clicks,
            created_at: link.created_at.to_string(),
            starts_at: link.starts_at.map(|d| d.to_string()),
            expires_at: link.expires_at.map(|d| d.to_string()),
            deleted_at: link.deleted_at.map(|d| d.to_string()),
            burned_at: link.burned_at.map(|d| d.to_string()),
            is_pinned: link.is_pinned,
            burn_after_reading: link.burn_after_reading,
            safe_link_interstitial: link.safe_link_interstitial,
            bio_visible: link.bio_visible,
            has_password: link.password_hash.is_some(),
            is_active,
            inactive_reason,
            suspicious: suspicion_reason.is_some(),
            suspicion_reason,
        }
    }).collect();

    (StatusCode::OK, Json(AdminLinksListResponse {
        links: responses,
        total,
        page,
        per_page,
    })).into_response()
}

/// Soft delete any user's link (admin only)
#[utoipa::path(
    delete,
    path = "/admin/links/{link_id}",
    params(
        ("link_id" = i32, Path, description = "Link ID to soft delete")
    ),
    responses(
        (status = 200, description = "Link deleted", body = AdminResponse),
        (status = 400, description = "Link already deleted"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn admin_delete_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(link_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let link = links::Entity::find_by_id(link_id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    let Some(link) = link else {
        return (StatusCode::NOT_FOUND, Json(AdminResponse {
            success: false,
            message: "Link not found".to_string(),
        })).into_response();
    };

    if link.deleted_at.is_some() {
        return (StatusCode::BAD_REQUEST, Json(AdminResponse {
            success: false,
            message: "Link already deleted".to_string(),
        })).into_response();
    }

    let code = link.code.clone();
    let mut active: links::ActiveModel = link.into();
    active.deleted_at = Set(Some(Utc::now().naive_utc()));

    if active.update(&state.db).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
            success: false,
            message: "Failed to delete link".to_string(),
        })).into_response();
    }

    // Drop any cached redirect so the takedown is immediate, not after TTL.
    if let Some(cache) = &state.redis_cache {
        let _ = cache.invalidate_link(&code).await;
    }

    (StatusCode::OK, Json(AdminResponse {
        success: true,
        message: format!("Link {} deleted", code),
    })).into_response()
}

/// Restore a soft-deleted link (admin only)
#[utoipa::path(
    post,
    path = "/admin/links/{link_id}/restore",
    params(
        ("link_id" = i32, Path, description = "Link ID to restore")
    ),
    responses(
        (status = 200, description = "Link restored", body = AdminResponse),
        (status = 400, description = "Link is not deleted"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn admin_restore_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(link_id): Path<i32>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let link = links::Entity::find_by_id(link_id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    let Some(link) = link else {
        return (StatusCode::NOT_FOUND, Json(AdminResponse {
            success: false,
            message: "Link not found".to_string(),
        })).into_response();
    };

    if link.deleted_at.is_none() {
        return (StatusCode::BAD_REQUEST, Json(AdminResponse {
            success: false,
            message: "Link is not deleted".to_string(),
        })).into_response();
    }

    let code = link.code.clone();
    let mut active: links::ActiveModel = link.into();
    active.deleted_at = Set(None);

    if active.update(&state.db).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
            success: false,
            message: "Failed to restore link".to_string(),
        })).into_response();
    }

    (StatusCode::OK, Json(AdminResponse {
        success: true,
        message: format!("Link {} restored", code),
    })).into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct BulkLinkIdsRequest {
    /// Link IDs to act on.
    pub ids: Vec<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkLinkActionResponse {
    pub success: bool,
    pub affected: u64,
    pub message: String,
}

/// Soft delete many links at once (admin only) — the fast path for clearing an
/// abuse spree. Invalidates the redirect cache for each affected code.
#[utoipa::path(
    post,
    path = "/admin/links/bulk/delete",
    request_body = BulkLinkIdsRequest,
    responses(
        (status = 200, description = "Links deleted", body = BulkLinkActionResponse),
        (status = 400, description = "No IDs provided"),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn admin_bulk_delete_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkLinkIdsRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }
    if payload.ids.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(AdminResponse {
            success: false,
            message: "No link IDs provided".to_string(),
        })).into_response();
    }

    // Collect the codes first so we can purge them from the redirect cache after
    // the update (soft delete is an UPDATE, so nothing cascades on its own).
    let affected_links = links::Entity::find()
        .filter(links::Column::Id.is_in(payload.ids.clone()))
        .filter(links::Column::DeletedAt.is_null())
        .all(&state.db)
        .await
        .unwrap_or_default();
    let codes: Vec<String> = affected_links.iter().map(|l| l.code.clone()).collect();

    let res = links::Entity::update_many()
        .col_expr(links::Column::DeletedAt, Expr::value(Utc::now().naive_utc()))
        .filter(links::Column::Id.is_in(payload.ids.clone()))
        .filter(links::Column::DeletedAt.is_null())
        .exec(&state.db)
        .await;

    match res {
        Ok(r) => {
            if let Some(cache) = &state.redis_cache {
                for code in &codes {
                    let _ = cache.invalidate_link(code).await;
                }
            }
            (StatusCode::OK, Json(BulkLinkActionResponse {
                success: true,
                affected: r.rows_affected,
                message: format!("Deleted {} link(s)", r.rows_affected),
            })).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
            success: false,
            message: "Failed to delete links".to_string(),
        })).into_response(),
    }
}

/// Restore many soft-deleted links at once (admin only).
#[utoipa::path(
    post,
    path = "/admin/links/bulk/restore",
    request_body = BulkLinkIdsRequest,
    responses(
        (status = 200, description = "Links restored", body = BulkLinkActionResponse),
        (status = 400, description = "No IDs provided"),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn admin_bulk_restore_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkLinkIdsRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }
    if payload.ids.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(AdminResponse {
            success: false,
            message: "No link IDs provided".to_string(),
        })).into_response();
    }

    let res = links::Entity::update_many()
        .col_expr(links::Column::DeletedAt, Expr::value(Option::<chrono::NaiveDateTime>::None))
        .filter(links::Column::Id.is_in(payload.ids.clone()))
        .filter(links::Column::DeletedAt.is_not_null())
        .exec(&state.db)
        .await;

    match res {
        Ok(r) => (StatusCode::OK, Json(BulkLinkActionResponse {
            success: true,
            affected: r.rows_affected,
            message: format!("Restored {} link(s)", r.rows_affected),
        })).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
            success: false,
            message: "Failed to restore links".to_string(),
        })).into_response(),
    }
}

#[derive(Serialize, ToSchema)]
pub struct BlockFromLinkResponse {
    pub success: bool,
    pub domain: String,
    pub message: String,
}

/// One-click takedown from the Links tab: block the destination's host (so no
/// link can point at it again) and soft-delete this link. The host is extracted
/// server-side from the stored URL rather than trusted from the client.
#[utoipa::path(
    post,
    path = "/admin/links/{link_id}/block-domain",
    params(
        ("link_id" = i32, Path, description = "Link whose destination host to block")
    ),
    responses(
        (status = 200, description = "Domain blocked and link deleted", body = BlockFromLinkResponse),
        (status = 400, description = "Link has no usable host"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn admin_block_domain_from_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(link_id): Path<i32>,
) -> impl IntoResponse {
    let admin_id = match require_admin(&state, &headers).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    let link = links::Entity::find_by_id(link_id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    let Some(link) = link else {
        return (StatusCode::NOT_FOUND, Json(AdminResponse {
            success: false,
            message: "Link not found".to_string(),
        })).into_response();
    };

    let host = url::Url::parse(&link.original_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_end_matches('.').to_lowercase()));

    let Some(domain) = host.filter(|h| !h.is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(AdminResponse {
            success: false,
            message: "Link destination has no host to block".to_string(),
        })).into_response();
    };

    // Block the domain if not already blocked (idempotent).
    let already = blocked_domains::Entity::find()
        .filter(blocked_domains::Column::Domain.eq(&domain))
        .one(&state.db)
        .await
        .ok()
        .flatten();
    if already.is_none() {
        let blocked = blocked_domains::ActiveModel {
            domain: Set(domain.clone()),
            reason: Set(Some("Blocked via admin link takedown".to_string())),
            blocked_by: Set(Some(admin_id)),
            ..Default::default()
        };
        if blocked.insert(&state.db).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
                success: false,
                message: "Failed to block domain".to_string(),
            })).into_response();
        }
    }

    // Soft-delete this link and purge the whole domain from the redirect cache
    // (covers every already-cached link pointing at the now-blocked host).
    let code = link.code.clone();
    let mut active: links::ActiveModel = link.into();
    active.deleted_at = Set(Some(Utc::now().naive_utc()));
    let _ = active.update(&state.db).await;
    invalidate_cache_for_domain(&state, &domain).await;
    if let Some(cache) = &state.redis_cache {
        let _ = cache.invalidate_link(&code).await;
    }

    (StatusCode::OK, Json(BlockFromLinkResponse {
        success: true,
        domain: domain.clone(),
        message: format!("Blocked {} and deleted link {}", domain, code),
    })).into_response()
}

/// Force-verify a user's email (admin only)
#[utoipa::path(
    post,
    path = "/admin/users/{user_id}/verify-email",
    params(
        ("user_id" = i32, Path, description = "User ID to verify")
    ),
    responses(
        (status = 200, description = "Email marked verified", body = AdminResponse),
        (status = 400, description = "Email already verified"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn admin_verify_email(
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

    let Some(user) = user else {
        return (StatusCode::NOT_FOUND, Json(AdminResponse {
            success: false,
            message: "User not found".to_string(),
        })).into_response();
    };

    if user.email_verified {
        return (StatusCode::BAD_REQUEST, Json(AdminResponse {
            success: false,
            message: "Email is already verified".to_string(),
        })).into_response();
    }

    let mut active: users::ActiveModel = user.into();
    active.email_verified = Set(true);
    active.verification_token = Set(None);
    active.verification_token_expires = Set(None);

    if active.update(&state.db).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(AdminResponse {
            success: false,
            message: "Failed to verify email".to_string(),
        })).into_response();
    }

    (StatusCode::OK, Json(AdminResponse {
        success: true,
        message: format!("Email verified for user {}", user_id),
    })).into_response()
}

// ==================== ADMIN: ORGANIZATIONS ====================

#[derive(Deserialize, IntoParams)]
pub struct AdminOrgsQuery {
    /// 1-based page number (default 1)
    pub page: Option<u64>,
    /// Items per page, 1-100 (default 25)
    pub per_page: Option<u64>,
    /// Substring match on org name or slug
    pub search: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminOrgResponse {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub owner_id: i32,
    pub owner_email: Option<String>,
    pub member_count: i64,
    pub links_count: i64,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct AdminOrgsListResponse {
    pub orgs: Vec<AdminOrgResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

/// List every organization with owner and member counts (admin only)
#[utoipa::path(
    get,
    path = "/admin/orgs",
    params(AdminOrgsQuery),
    responses(
        (status = 200, description = "Paginated list of all organizations", body = AdminOrgsListResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_all_orgs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminOrgsQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let (page, per_page) = clamp_pagination(query.page, query.per_page);

    let mut finder = organizations::Entity::find();

    if let Some(search) = query.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = ilike_pattern(search);
        finder = finder.filter(
            Condition::any()
                .add(Expr::col((organizations::Entity, organizations::Column::Name)).ilike(pattern.clone()))
                .add(Expr::col((organizations::Entity, organizations::Column::Slug)).ilike(pattern)),
        );
    }

    let paginator = finder
        .order_by_desc(organizations::Column::CreatedAt)
        .paginate(&state.db, per_page);
    let total = paginator.num_items().await.unwrap_or(0);
    let orgs_page = paginator.fetch_page(page - 1).await.unwrap_or_default();

    let org_ids: Vec<i32> = orgs_page.iter().map(|o| o.id).collect();
    let owner_ids: Vec<i32> = orgs_page.iter().map(|o| o.owner_id).collect();

    let mut member_counts: HashMap<i32, i64> = HashMap::new();
    let mut link_counts: HashMap<i32, i64> = HashMap::new();
    let mut owner_emails: HashMap<i32, String> = HashMap::new();

    if !org_ids.is_empty() {
        let rows: Vec<(i32, i64)> = org_members::Entity::find()
            .select_only()
            .column(org_members::Column::OrgId)
            .column_as(org_members::Column::Id.count(), "count")
            .filter(org_members::Column::OrgId.is_in(org_ids.clone()))
            .group_by(org_members::Column::OrgId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        member_counts.extend(rows);

        let rows: Vec<(Option<i32>, i64)> = links::Entity::find()
            .select_only()
            .column(links::Column::OrgId)
            .column_as(links::Column::Id.count(), "count")
            .filter(links::Column::OrgId.is_in(org_ids.clone()))
            .filter(links::Column::DeletedAt.is_null())
            .group_by(links::Column::OrgId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        for (org_id, count) in rows {
            if let Some(org_id) = org_id {
                link_counts.insert(org_id, count);
            }
        }

        let owners = users::Entity::find()
            .filter(users::Column::Id.is_in(owner_ids))
            .all(&state.db)
            .await
            .unwrap_or_default();
        for owner in owners {
            owner_emails.insert(owner.id, owner.email);
        }
    }

    let responses: Vec<AdminOrgResponse> = orgs_page.into_iter().map(|o| AdminOrgResponse {
        id: o.id,
        name: o.name,
        slug: o.slug,
        owner_id: o.owner_id,
        owner_email: owner_emails.get(&o.owner_id).cloned(),
        member_count: member_counts.get(&o.id).copied().unwrap_or(0),
        links_count: link_counts.get(&o.id).copied().unwrap_or(0),
        created_at: o.created_at.to_string(),
    }).collect();

    (StatusCode::OK, Json(AdminOrgsListResponse {
        orgs: responses,
        total,
        page,
        per_page,
    })).into_response()
}

// ==================== ADMIN: ACTIVITY TIMESERIES ====================

#[derive(Deserialize, IntoParams)]
pub struct AdminActivityQuery {
    /// Days of history, 1-365 (default 30)
    pub days: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct ActivityDay {
    pub date: String,
    pub new_users: i64,
    pub new_links: i64,
    pub clicks: i64,
}

#[derive(Serialize, ToSchema)]
pub struct AdminActivityResponse {
    pub days: Vec<ActivityDay>,
}

/// Count rows per day for a table since `cutoff`. Table name comes from a
/// fixed internal list, never user input.
async fn day_counts(
    db: &DatabaseConnection,
    table: &str,
    cutoff: chrono::NaiveDateTime,
) -> HashMap<String, i64> {
    let sql = format!(
        "SELECT to_char(created_at::date, 'YYYY-MM-DD') AS day, COUNT(*)::bigint AS cnt \
         FROM {} WHERE created_at >= $1 GROUP BY 1",
        table
    );
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [cutoff.into()],
        ))
        .await
        .unwrap_or_default();

    let mut map = HashMap::new();
    for row in rows {
        if let (Ok(day), Ok(cnt)) = (row.try_get::<String>("", "day"), row.try_get::<i64>("", "cnt")) {
            map.insert(day, cnt);
        }
    }
    map
}

/// Daily signups, link creations, and clicks for the admin overview chart
#[utoipa::path(
    get,
    path = "/admin/activity",
    params(AdminActivityQuery),
    responses(
        (status = 200, description = "Per-day activity for the requested window", body = AdminActivityResponse),
        (status = 403, description = "Admin access required"),
    ),
    tag = "Admin",
    security(("bearer_auth" = []))
)]
pub async fn get_admin_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminActivityQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_admin(&state, &headers).await {
        return e.into_response();
    }

    let days = query.days.unwrap_or(30).clamp(1, 365) as i64;
    let today = Utc::now().date_naive();
    let start = today - Duration::days(days - 1);
    let cutoff = start.and_hms_opt(0, 0, 0).unwrap_or_else(|| Utc::now().naive_utc());

    let users_by_day = day_counts(&state.db, "users", cutoff).await;
    let links_by_day = day_counts(&state.db, "links", cutoff).await;
    let clicks_by_day = day_counts(&state.db, "click_events", cutoff).await;

    // Emit every day in the window, zero-filled, so charts don't skip days.
    let mut out = Vec::with_capacity(days as usize);
    let mut day = start;
    while day <= today {
        let key = day.format("%Y-%m-%d").to_string();
        out.push(ActivityDay {
            date: key.clone(),
            new_users: users_by_day.get(&key).copied().unwrap_or(0),
            new_links: links_by_day.get(&key).copied().unwrap_or(0),
            clicks: clicks_by_day.get(&key).copied().unwrap_or(0),
        });
        day += Duration::days(1);
    }

    (StatusCode::OK, Json(AdminActivityResponse { days: out })).into_response()
}
