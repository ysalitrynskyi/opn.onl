use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{
    audit_log, click_events, folders, link_tags, links, org_members, organizations, tags, users,
};
use crate::utils::email_domain_policy::normalize_email;
use crate::utils::time::utc_rfc3339;
use crate::utils::validation::{check_name, check_slug};

// ============= DTOs =============

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateOrgRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InviteMemberRequest {
    pub email: String,
    pub role: String, // "admin", "editor", "viewer"
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TransferOwnershipRequest {
    /// User ID of the member who becomes the new owner.
    pub new_owner_user_id: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgResponse {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub owner_id: i32,
    pub created_at: String,
    pub member_count: i64,
    pub link_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgMemberResponse {
    pub id: i32,
    pub user_id: i32,
    pub email: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditLogResponse {
    pub id: i32,
    pub user_id: Option<i32>,
    pub user_email: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<i32>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub created_at: String,
}

// ============= Helper Functions =============

async fn get_user_id_from_header(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Option<i32> {
    // Delegate to the shared resolver (handles both JWT and `opn_` API keys).
    crate::handlers::links::get_user_id_from_header(db, headers).await
}

async fn check_org_permission(
    db: &sea_orm::DatabaseConnection,
    org_id: i32,
    user_id: i32,
    required_role: &str,
) -> Result<org_members::Model, (StatusCode, Json<serde_json::Value>)> {
    let member = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Not a member of this organization"})),
            )
        })?;

    let has_permission = match required_role {
        "viewer" => true,
        "editor" => member.can_edit(),
        "admin" => member.is_admin(),
        "owner" => member.is_owner(),
        _ => false,
    };

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Insufficient permissions"})),
        ));
    }

    Ok(member)
}

/// True if the user is a member of the org who may modify its resources.
/// Use the entity's explicit role allowlist so unknown/future roles remain
/// read-only until their permissions are deliberately defined.
pub(crate) async fn member_can_edit(
    db: &sea_orm::DatabaseConnection,
    org_id: i32,
    user_id: i32,
) -> bool {
    org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|m| m.can_edit())
        .unwrap_or(false)
}

/// Organizations owned by a user, split by what deleting that user would do
/// to them. `blocking` orgs still have other live members, so the account
/// cannot be deleted until ownership is transferred (or the org deliberately
/// deleted). Soft-deleted users are ignored: they cannot log in or accept a
/// transfer. `solo` orgs have no live member besides the owner and die with
/// the account on hard delete.
pub(crate) struct OwnedOrgsSplit {
    pub blocking: Vec<organizations::Model>,
    pub solo: Vec<organizations::Model>,
}

pub(crate) async fn split_owned_orgs<C: ConnectionTrait>(
    db: &C,
    user_id: i32,
) -> Result<OwnedOrgsSplit, sea_orm::DbErr> {
    let owned = organizations::Entity::find()
        .filter(organizations::Column::OwnerId.eq(user_id))
        .all(db)
        .await?;
    if owned.is_empty() {
        return Ok(OwnedOrgsSplit {
            blocking: Vec::new(),
            solo: Vec::new(),
        });
    }

    let owned_ids: Vec<i32> = owned.iter().map(|org| org.id).collect();
    let rows: Vec<(i32, i64)> = org_members::Entity::find()
        .select_only()
        .column(org_members::Column::OrgId)
        .column_as(org_members::Column::Id.count(), "cnt")
        .inner_join(users::Entity)
        .filter(org_members::Column::OrgId.is_in(owned_ids))
        .filter(org_members::Column::UserId.ne(user_id))
        .filter(users::Column::DeletedAt.is_null())
        .group_by(org_members::Column::OrgId)
        .into_tuple()
        .all(db)
        .await?;
    let other_live: HashMap<i32, i64> = rows.into_iter().collect();

    let mut blocking = Vec::new();
    let mut solo = Vec::new();
    for org in owned {
        if other_live.get(&org.id).copied().unwrap_or(0) > 0 {
            blocking.push(org);
        } else {
            solo.push(org);
        }
    }
    Ok(OwnedOrgsSplit { blocking, solo })
}

/// JSON body for refusing an account deletion because the user still owns
/// organizations with other members.
pub(crate) fn ownership_conflict_body(blocking: &[organizations::Model]) -> serde_json::Value {
    serde_json::json!({
        "error": "Account still owns organizations with other members. Transfer ownership (POST /orgs/{org_id}/transfer-ownership) or delete the organization first.",
        "code": "ORG_OWNERSHIP_TRANSFER_REQUIRED",
        "organizations": blocking.iter().map(|o| serde_json::json!({
            "id": o.id,
            "name": o.name,
            "slug": o.slug,
        })).collect::<Vec<_>>(),
    })
}

/// Delete an organization and all of its data: links (with click events and
/// tag assignments), folders, tags, audit log, memberships, then the org row.
pub(crate) async fn purge_organization<C: ConnectionTrait>(
    db: &C,
    org_id: i32,
) -> Result<(), sea_orm::DbErr> {
    let link_ids: Vec<i32> = links::Entity::find()
        .select_only()
        .column(links::Column::Id)
        .filter(links::Column::OrgId.eq(org_id))
        .into_tuple()
        .all(db)
        .await?;

    if !link_ids.is_empty() {
        click_events::Entity::delete_many()
            .filter(click_events::Column::LinkId.is_in(link_ids.clone()))
            .exec(db)
            .await?;
        link_tags::Entity::delete_many()
            .filter(link_tags::Column::LinkId.is_in(link_ids))
            .exec(db)
            .await?;
    }
    links::Entity::delete_many()
        .filter(links::Column::OrgId.eq(org_id))
        .exec(db)
        .await?;

    folders::Entity::delete_many()
        .filter(folders::Column::OrgId.eq(org_id))
        .exec(db)
        .await?;

    tags::Entity::delete_many()
        .filter(tags::Column::OrgId.eq(org_id))
        .exec(db)
        .await?;

    audit_log::Entity::delete_many()
        .filter(audit_log::Column::OrgId.eq(org_id))
        .exec(db)
        .await?;

    org_members::Entity::delete_many()
        .filter(org_members::Column::OrgId.eq(org_id))
        .exec(db)
        .await?;

    organizations::Entity::delete_by_id(org_id).exec(db).await?;

    Ok(())
}

// Audit logging naturally records many independent fields; grouping them into a
// struct would add indirection without improving clarity.
#[allow(clippy::too_many_arguments)]
async fn log_audit(
    db: &sea_orm::DatabaseConnection,
    org_id: i32,
    user_id: i32,
    action: &str,
    resource_type: &str,
    resource_id: Option<i32>,
    details: Option<serde_json::Value>,
    ip_address: Option<String>,
) {
    let audit_entry = audit_log::ActiveModel {
        org_id: Set(Some(org_id)),
        user_id: Set(Some(user_id)),
        action: Set(action.to_string()),
        resource_type: Set(resource_type.to_string()),
        resource_id: Set(resource_id),
        details: Set(details),
        ip_address: Set(ip_address),
        ..Default::default()
    };

    let _ = audit_entry.insert(db).await;
}

// ============= Handlers =============

/// Create a new organization
#[utoipa::path(
    post,
    path = "/orgs",
    request_body = CreateOrgRequest,
    responses(
        (status = 201, description = "Organization created", body = OrgResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 409, description = "Slug already exists"),
    ),
    tag = "Organizations"
)]
pub async fn create_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateOrgRequest>,
) -> Result<(StatusCode, Json<OrgResponse>), (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    if let Err(e) =
        check_name("Organization name", &payload.name).and_then(|()| check_slug(&payload.slug))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ));
    }

    // Check if slug already exists
    let existing = organizations::Entity::find()
        .filter(organizations::Column::Slug.eq(&payload.slug))
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Slug already exists"})),
        ));
    }

    let txn = state.db.begin().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    // Create organization and owner membership together. A membership insert
    // failure used to leave an org row whose slug was taken but which
    // `check_org_permission` could not see, so the owner could neither list
    // nor delete it.
    let org = organizations::ActiveModel {
        name: Set(payload.name.clone()),
        slug: Set(payload.slug.clone()),
        owner_id: Set(user_id),
        ..Default::default()
    };

    let org = match org.insert(&txn).await {
        Ok(org) => org,
        Err(err) if err.to_string().contains("duplicate key value") => {
            let _ = txn.rollback().await;
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "Slug already exists"})),
            ));
        }
        Err(_) => {
            let _ = txn.rollback().await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to create organization"})),
            ));
        }
    };

    let member = org_members::ActiveModel {
        org_id: Set(org.id),
        user_id: Set(user_id),
        role: Set("owner".to_string()),
        ..Default::default()
    };

    if member.insert(&txn).await.is_err() {
        let _ = txn.rollback().await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to add owner as member"})),
        ));
    }

    txn.commit().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to create organization"})),
        )
    })?;

    // Log audit
    log_audit(
        &state.db,
        org.id,
        user_id,
        "create",
        "organization",
        Some(org.id),
        None,
        None,
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(OrgResponse {
            id: org.id,
            name: org.name,
            slug: org.slug,
            owner_id: org.owner_id,
            created_at: utc_rfc3339(org.created_at),
            member_count: 1,
            link_count: 0,
        }),
    ))
}

/// Get user's organizations
#[utoipa::path(
    get,
    path = "/orgs",
    responses(
        (status = 200, description = "List of organizations", body = Vec<OrgResponse>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Organizations"
)]
pub async fn get_user_organizations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<OrgResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    // Get all organizations where user is a member
    let memberships = org_members::Entity::find()
        .filter(org_members::Column::UserId.eq(user_id))
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let org_ids: Vec<i32> = memberships.iter().map(|m| m.org_id).collect();

    let orgs = organizations::Entity::find()
        .filter(organizations::Column::Id.is_in(org_ids))
        .order_by_desc(organizations::Column::CreatedAt)
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let org_ids: Vec<i32> = orgs.iter().map(|o| o.id).collect();
    let mut member_counts: HashMap<i32, i64> = HashMap::new();
    let mut link_counts: HashMap<i32, i64> = HashMap::new();
    if !org_ids.is_empty() {
        let rows: Vec<(i32, i64)> = org_members::Entity::find()
            .select_only()
            .column(org_members::Column::OrgId)
            .column_as(org_members::Column::Id.count(), "cnt")
            .filter(org_members::Column::OrgId.is_in(org_ids.clone()))
            .group_by(org_members::Column::OrgId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        member_counts.extend(rows);

        let rows: Vec<(Option<i32>, i64)> = crate::entity::links::Entity::find()
            .select_only()
            .column(crate::entity::links::Column::OrgId)
            .column_as(crate::entity::links::Column::Id.count(), "cnt")
            .filter(crate::entity::links::Column::OrgId.is_in(org_ids))
            .filter(crate::entity::links::Column::DeletedAt.is_null())
            .group_by(crate::entity::links::Column::OrgId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        for (org_id, count) in rows {
            if let Some(org_id) = org_id {
                link_counts.insert(org_id, count);
            }
        }
    }

    let responses = orgs
        .into_iter()
        .map(|org| OrgResponse {
            id: org.id,
            name: org.name.clone(),
            slug: org.slug.clone(),
            owner_id: org.owner_id,
            created_at: utc_rfc3339(org.created_at),
            member_count: member_counts.get(&org.id).copied().unwrap_or(0),
            link_count: link_counts.get(&org.id).copied().unwrap_or(0),
        })
        .collect();

    Ok(Json(responses))
}

/// Get organization by ID
#[utoipa::path(
    get,
    path = "/orgs/{org_id}",
    params(
        ("org_id" = i32, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization details", body = OrgResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Organizations"
)]
pub async fn get_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
) -> Result<Json<OrgResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "viewer").await?;

    let org = organizations::Entity::find_by_id(org_id)
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Organization not found"})),
            )
        })?;

    let member_count = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org.id))
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    let link_count = crate::entity::links::Entity::find()
        .filter(crate::entity::links::Column::OrgId.eq(org.id))
        .filter(crate::entity::links::Column::DeletedAt.is_null())
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(OrgResponse {
        id: org.id,
        name: org.name.clone(),
        slug: org.slug.clone(),
        owner_id: org.owner_id,
        created_at: utc_rfc3339(org.created_at),
        member_count,
        link_count,
    }))
}

/// Update organization
#[utoipa::path(
    put,
    path = "/orgs/{org_id}",
    params(
        ("org_id" = i32, Path, description = "Organization ID")
    ),
    request_body = UpdateOrgRequest,
    responses(
        (status = 200, description = "Organization updated", body = OrgResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Slug already exists"),
    ),
    tag = "Organizations"
)]
pub async fn update_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Json(payload): Json<UpdateOrgRequest>,
) -> Result<Json<OrgResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "admin").await?;

    let org = organizations::Entity::find_by_id(org_id)
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Organization not found"})),
            )
        })?;

    if let Err(e) = payload
        .name
        .as_deref()
        .map_or(Ok(()), |name| check_name("Organization name", name))
        .and_then(|()| payload.slug.as_deref().map_or(Ok(()), check_slug))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ));
    }

    let mut org: organizations::ActiveModel = org.into();

    if let Some(name) = payload.name {
        org.name = Set(name);
    }
    if let Some(slug) = payload.slug {
        let taken = organizations::Entity::find()
            .filter(organizations::Column::Slug.eq(&slug))
            .filter(organizations::Column::Id.ne(org_id))
            .one(&state.db)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Database error"})),
                )
            })?;
        if taken.is_some() {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "Slug already exists"})),
            ));
        }
        org.slug = Set(slug);
    }

    let org = match org.update(&state.db).await {
        Ok(org) => org,
        Err(err) if err.to_string().contains("duplicate key value") => {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "Slug already exists"})),
            ));
        }
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to update organization"})),
            ));
        }
    };

    log_audit(
        &state.db,
        org_id,
        user_id,
        "update",
        "organization",
        Some(org_id),
        None,
        None,
    )
    .await;

    let member_count = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org.id))
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    let link_count = crate::entity::links::Entity::find()
        .filter(crate::entity::links::Column::OrgId.eq(org.id))
        .filter(crate::entity::links::Column::DeletedAt.is_null())
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(OrgResponse {
        id: org.id,
        name: org.name.clone(),
        slug: org.slug.clone(),
        owner_id: org.owner_id,
        created_at: utc_rfc3339(org.created_at),
        member_count,
        link_count,
    }))
}

/// Delete organization
#[utoipa::path(
    delete,
    path = "/orgs/{org_id}",
    params(
        ("org_id" = i32, Path, description = "Organization ID")
    ),
    responses(
        (status = 204, description = "Organization deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Organizations"
)]
pub async fn delete_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "owner").await?;

    let txn = state.db.begin().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    purge_organization(&txn, org_id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete organization"})),
        )
    })?;

    txn.commit().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete organization"})),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get organization members
#[utoipa::path(
    get,
    path = "/orgs/{org_id}/members",
    params(
        ("org_id" = i32, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of members", body = Vec<OrgMemberResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "Organizations"
)]
pub async fn get_organization_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
) -> Result<Json<Vec<OrgMemberResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "viewer").await?;

    let members = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .order_by_asc(org_members::Column::JoinedAt)
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let user_ids: Vec<i32> = members.iter().map(|m| m.user_id).collect();
    let mut users_by_id: HashMap<i32, users::Model> = HashMap::new();
    if !user_ids.is_empty() {
        let loaded = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .filter(users::Column::DeletedAt.is_null())
            .all(&state.db)
            .await
            .unwrap_or_default();
        users_by_id.extend(loaded.into_iter().map(|u| (u.id, u)));
    }

    let mut responses = Vec::new();
    for member in members {
        let Some(user) = users_by_id.get(&member.user_id) else {
            continue;
        };

        responses.push(OrgMemberResponse {
            id: member.id,
            user_id: member.user_id,
            email: user.email.clone(),
            role: member.role,
            joined_at: utc_rfc3339(member.joined_at),
        });
    }

    Ok(Json(responses))
}

/// Invite member to organization
#[utoipa::path(
    post,
    path = "/orgs/{org_id}/members",
    params(
        ("org_id" = i32, Path, description = "Organization ID")
    ),
    request_body = InviteMemberRequest,
    responses(
        (status = 201, description = "Member invited", body = OrgMemberResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "User not found"),
        (status = 409, description = "Already a member"),
    ),
    tag = "Organizations"
)]
pub async fn invite_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Json(payload): Json<InviteMemberRequest>,
) -> Result<(StatusCode, Json<OrgMemberResponse>), (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "admin").await?;

    // Validate role
    if !["admin", "editor", "viewer"].contains(&payload.role.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid role. Must be admin, editor, or viewer"})),
        ));
    }

    // Find user by email. Registration stores normalize_email (trimmed, domain
    // lowercased), so the invite lookup must use the same form or a real user 404s.
    let email = normalize_email(&payload.email);
    let invite_user = users::Entity::find()
        .filter(users::Column::Email.eq(&email))
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "User not found"})),
            )
        })?;

    // Check if already a member
    let existing = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(invite_user.id))
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "User is already a member"})),
        ));
    }

    // Add member
    let member = org_members::ActiveModel {
        org_id: Set(org_id),
        user_id: Set(invite_user.id),
        role: Set(payload.role.clone()),
        ..Default::default()
    };

    let member = member.insert(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to add member"})),
        )
    })?;

    log_audit(
        &state.db,
        org_id,
        user_id,
        "invite",
        "member",
        Some(member.id),
        Some(serde_json::json!({"email": payload.email, "role": payload.role})),
        None,
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(OrgMemberResponse {
            id: member.id,
            user_id: invite_user.id,
            email: invite_user.email,
            role: member.role,
            joined_at: utc_rfc3339(member.joined_at),
        }),
    ))
}

/// Update member role
#[utoipa::path(
    put,
    path = "/orgs/{org_id}/members/{member_id}",
    params(
        ("org_id" = i32, Path, description = "Organization ID"),
        ("member_id" = i32, Path, description = "Member ID")
    ),
    request_body = UpdateMemberRoleRequest,
    responses(
        (status = 200, description = "Member role updated", body = OrgMemberResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Member not found"),
    ),
    tag = "Organizations"
)]
pub async fn update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((org_id, member_id)): Path<(i32, i32)>,
    Json(payload): Json<UpdateMemberRoleRequest>,
) -> Result<Json<OrgMemberResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "admin").await?;

    // Validate role
    if !["admin", "editor", "viewer"].contains(&payload.role.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid role"})),
        ));
    }

    let member = org_members::Entity::find_by_id(member_id)
        .filter(org_members::Column::OrgId.eq(org_id))
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Member not found"})),
            )
        })?;

    // Can't change owner's role
    if member.role == "owner" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Cannot change owner's role"})),
        ));
    }

    let mut member: org_members::ActiveModel = member.into();
    member.role = Set(payload.role.clone());

    let member = member.update(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update member"})),
        )
    })?;

    let user = users::Entity::find_by_id(member.user_id)
        .one(&state.db)
        .await
        .ok()
        .flatten();

    log_audit(
        &state.db,
        org_id,
        user_id,
        "update_role",
        "member",
        Some(member_id),
        Some(serde_json::json!({"new_role": payload.role})),
        None,
    )
    .await;

    Ok(Json(OrgMemberResponse {
        id: member.id,
        user_id: member.user_id,
        email: user.map(|u| u.email).unwrap_or_default(),
        role: member.role,
        joined_at: utc_rfc3339(member.joined_at),
    }))
}

/// Remove member from organization
#[utoipa::path(
    delete,
    path = "/orgs/{org_id}/members/{member_id}",
    params(
        ("org_id" = i32, Path, description = "Organization ID"),
        ("member_id" = i32, Path, description = "Member ID")
    ),
    responses(
        (status = 204, description = "Member removed"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Member not found"),
    ),
    tag = "Organizations"
)]
pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((org_id, member_id)): Path<(i32, i32)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "admin").await?;

    let member = org_members::Entity::find_by_id(member_id)
        .filter(org_members::Column::OrgId.eq(org_id))
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Member not found"})),
            )
        })?;

    // Can't remove owner
    if member.role == "owner" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Cannot remove owner"})),
        ));
    }

    org_members::Entity::delete_by_id(member_id)
        .exec(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to remove member"})),
            )
        })?;

    log_audit(
        &state.db,
        org_id,
        user_id,
        "remove",
        "member",
        Some(member_id),
        None,
        None,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Transfer organization ownership to another member
#[utoipa::path(
    post,
    path = "/orgs/{org_id}/transfer-ownership",
    params(
        ("org_id" = i32, Path, description = "Organization ID")
    ),
    request_body = TransferOwnershipRequest,
    responses(
        (status = 200, description = "Ownership transferred", body = OrgResponse),
        (status = 400, description = "Invalid target (self, deleted user, or not a member)"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Only the owner can transfer ownership"),
        (status = 404, description = "Organization or member not found"),
    ),
    tag = "Organizations",
    security(("bearer_auth" = []))
)]
pub async fn transfer_ownership(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Json(payload): Json<TransferOwnershipRequest>,
) -> Result<Json<OrgResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "owner").await?;

    if payload.new_owner_user_id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "You already own this organization"})),
        ));
    }

    let org = organizations::Entity::find_by_id(org_id)
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Organization not found"})),
            )
        })?;

    // Target must be an existing, non-deleted, non-disabled user...
    let new_owner = users::Entity::find_by_id(payload.new_owner_user_id)
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "New owner must be an active user"})),
            )
        })?;

    // ...who is already a member of this organization.
    let new_owner_member = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(new_owner.id))
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": "New owner must be a member of the organization"}),
                ),
            )
        })?;

    let old_owner_member = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let txn = state.db.begin().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    let transfer = async {
        let mut org_active: organizations::ActiveModel = org.into();
        org_active.owner_id = Set(new_owner.id);
        let org = org_active.update(&txn).await?;

        let mut promoted: org_members::ActiveModel = new_owner_member.into();
        promoted.role = Set("owner".to_string());
        promoted.update(&txn).await?;

        // Previous owner stays in the org as an admin.
        if let Some(old_member) = old_owner_member {
            let mut demoted: org_members::ActiveModel = old_member.into();
            demoted.role = Set("admin".to_string());
            demoted.update(&txn).await?;
        }

        Ok::<organizations::Model, sea_orm::DbErr>(org)
    }
    .await;

    let org = match transfer {
        Ok(org) => org,
        Err(_) => {
            let _ = txn.rollback().await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to transfer ownership"})),
            ));
        }
    };

    txn.commit().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to transfer ownership"})),
        )
    })?;

    log_audit(
        &state.db,
        org_id,
        user_id,
        "transfer_ownership",
        "organization",
        Some(org_id),
        Some(serde_json::json!({
            "previous_owner_id": user_id,
            "new_owner_id": new_owner.id,
        })),
        None,
    )
    .await;

    let member_count = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org.id))
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    let link_count = links::Entity::find()
        .filter(links::Column::OrgId.eq(org.id))
        .filter(links::Column::DeletedAt.is_null())
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(OrgResponse {
        id: org.id,
        name: org.name.clone(),
        slug: org.slug.clone(),
        owner_id: org.owner_id,
        created_at: utc_rfc3339(org.created_at),
        member_count,
        link_count,
    }))
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct AuditQuery {
    /// Newest-first page size. Omitted or 0 defaults to 200. Maximum 500.
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

const DEFAULT_AUDIT_LIMIT: u64 = 200;
const MAX_AUDIT_LIMIT: u64 = 500;

fn clamp_audit_limit(limit: Option<u64>) -> u64 {
    limit
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_AUDIT_LIMIT)
        .min(MAX_AUDIT_LIMIT)
}

/// Get organization audit log
#[utoipa::path(
    get,
    path = "/orgs/{org_id}/audit",
    params(
        ("org_id" = i32, Path, description = "Organization ID"),
        AuditQuery,
    ),
    responses(
        (status = 200, description = "Audit log entries (newest first, default 200, maximum 500)", body = Vec<AuditLogResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "Organizations"
)]
pub async fn get_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditLogResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    check_org_permission(&state.db, org_id, user_id, "admin").await?;

    let mut logs_query = audit_log::Entity::find()
        .filter(audit_log::Column::OrgId.eq(org_id))
        .order_by_desc(audit_log::Column::CreatedAt)
        .limit(clamp_audit_limit(query.limit));
    if let Some(offset) = query.offset {
        logs_query = logs_query.offset(offset);
    }

    let logs = logs_query.all(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    let user_ids: Vec<i32> = logs.iter().filter_map(|log| log.user_id).collect();
    let mut emails: HashMap<i32, String> = HashMap::new();
    if !user_ids.is_empty() {
        let loaded = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .all(&state.db)
            .await
            .unwrap_or_default();
        emails.extend(loaded.into_iter().map(|u| (u.id, u.email)));
    }

    let mut responses = Vec::new();
    for log in logs {
        let user_email = log.user_id.and_then(|uid| emails.get(&uid).cloned());
        responses.push(AuditLogResponse {
            id: log.id,
            user_id: log.user_id,
            user_email,
            action: log.action,
            resource_type: log.resource_type,
            resource_id: log.resource_id,
            details: log.details,
            ip_address: log.ip_address,
            created_at: utc_rfc3339(log.created_at),
        });
    }

    Ok(Json(responses))
}
