use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{tags, link_tags, links};
use crate::utils::decode_jwt;

// ============= DTOs =============

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
    pub org_id: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTagRequest {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct TagQuery {
    pub org_id: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TagResponse {
    pub id: i32,
    pub name: String,
    pub color: Option<String>,
    pub user_id: Option<i32>,
    pub org_id: Option<i32>,
    pub created_at: String,
    pub link_count: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddTagsToLinkRequest {
    pub tag_ids: Vec<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RemoveTagsFromLinkRequest {
    pub tag_ids: Vec<i32>,
}

// ============= Helper Functions =============

fn get_user_id_from_header(headers: &HeaderMap) -> Option<i32> {
    let auth_header = headers.get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let claims = decode_jwt(token).ok()?;
    Some(claims.user_id)
}

// ============= Handlers =============

/// Create a new tag
#[utoipa::path(
    post,
    path = "/tags",
    request_body = CreateTagRequest,
    responses(
        (status = 201, description = "Tag created", body = TagResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Tags"
)]
pub async fn create_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<TagResponse>), (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    // If org_id is provided, verify user is a member
    if let Some(org_id) = payload.org_id {
        use crate::entity::org_members;
        let is_member = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await
            .ok()
            .flatten()
            .is_some();
        
        if !is_member {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Not a member of this organization"})),
            ));
        }
    }

    let tag = tags::ActiveModel {
        name: Set(payload.name.clone()),
        color: Set(payload.color.clone()),
        user_id: Set(if payload.org_id.is_some() { None } else { Some(user_id) }),
        org_id: Set(payload.org_id),
        ..Default::default()
    };

    let tag = tag.insert(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to create tag"})),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(TagResponse {
            id: tag.id,
            name: tag.name,
            color: tag.color,
            user_id: tag.user_id,
            org_id: tag.org_id,
            created_at: tag.created_at.to_string(),
            link_count: 0,
        }),
    ))
}

/// Get user's tags
#[utoipa::path(
    get,
    path = "/tags",
    params(TagQuery),
    responses(
        (status = 200, description = "List of tags", body = Vec<TagResponse>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Tags"
)]
pub async fn get_tags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TagQuery>,
) -> Result<Json<Vec<TagResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let mut tag_query = tags::Entity::find();

    if let Some(org_id) = query.org_id {
        // Verify user is member of this org before listing its tags
        use crate::entity::org_members;
        let is_member = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await
            .ok()
            .flatten()
            .is_some();
        
        if !is_member {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Not a member of this organization"})),
            ));
        }
        
        tag_query = tag_query.filter(tags::Column::OrgId.eq(org_id));
    } else {
        tag_query = tag_query.filter(tags::Column::UserId.eq(user_id));
    }

    let tags_list = tag_query
        .order_by_asc(tags::Column::Name)
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let mut responses = Vec::new();
    for tag in tags_list {
        let link_count = link_tags::Entity::find()
            .filter(link_tags::Column::TagId.eq(tag.id))
            .count(&state.db)
            .await
            .unwrap_or(0) as i64;

        responses.push(TagResponse {
            id: tag.id,
            name: tag.name.clone(),
            color: tag.color.clone(),
            user_id: tag.user_id,
            org_id: tag.org_id,
            created_at: tag.created_at.to_string(),
            link_count,
        });
    }

    Ok(Json(responses))
}

/// Get tag by ID
#[utoipa::path(
    get,
    path = "/tags/{tag_id}",
    params(
        ("tag_id" = i32, Path, description = "Tag ID")
    ),
    responses(
        (status = 200, description = "Tag details", body = TagResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Tags"
)]
pub async fn get_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tag_id): Path<i32>,
) -> Result<Json<TagResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let tag = tags::Entity::find_by_id(tag_id)
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
                Json(serde_json::json!({"error": "Tag not found"})),
            )
        })?;

    // Check ownership - must own the tag directly, or be member of the org that owns it
    let has_access = if tag.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = tag.org_id {
        use crate::entity::org_members;
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };
    
    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let link_count = link_tags::Entity::find()
        .filter(link_tags::Column::TagId.eq(tag.id))
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name.clone(),
        color: tag.color.clone(),
        user_id: tag.user_id,
        org_id: tag.org_id,
        created_at: tag.created_at.to_string(),
        link_count,
    }))
}

/// Update tag
#[utoipa::path(
    put,
    path = "/tags/{tag_id}",
    params(
        ("tag_id" = i32, Path, description = "Tag ID")
    ),
    request_body = UpdateTagRequest,
    responses(
        (status = 200, description = "Tag updated", body = TagResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Tags"
)]
pub async fn update_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tag_id): Path<i32>,
    Json(payload): Json<UpdateTagRequest>,
) -> Result<Json<TagResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let tag = tags::Entity::find_by_id(tag_id)
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
                Json(serde_json::json!({"error": "Tag not found"})),
            )
        })?;

    // Check ownership - must own the tag directly, or be member of the org that owns it
    let has_access = if tag.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = tag.org_id {
        use crate::entity::org_members;
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };
    
    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let mut tag: tags::ActiveModel = tag.into();

    if let Some(name) = payload.name {
        tag.name = Set(name);
    }
    if let Some(color) = payload.color {
        tag.color = Set(Some(color));
    }

    let tag = tag.update(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update tag"})),
        )
    })?;

    let link_count = link_tags::Entity::find()
        .filter(link_tags::Column::TagId.eq(tag.id))
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name.clone(),
        color: tag.color.clone(),
        user_id: tag.user_id,
        org_id: tag.org_id,
        created_at: tag.created_at.to_string(),
        link_count,
    }))
}

/// Delete tag
#[utoipa::path(
    delete,
    path = "/tags/{tag_id}",
    params(
        ("tag_id" = i32, Path, description = "Tag ID")
    ),
    responses(
        (status = 204, description = "Tag deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Tags"
)]
pub async fn delete_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tag_id): Path<i32>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let tag = tags::Entity::find_by_id(tag_id)
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
                Json(serde_json::json!({"error": "Tag not found"})),
            )
        })?;

    // Check ownership - must own the tag directly, or be member of the org that owns it
    let has_access = if tag.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = tag.org_id {
        use crate::entity::org_members;
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };
    
    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    tags::Entity::delete_by_id(tag_id)
        .exec(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to delete tag"})),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Add tags to a link
#[utoipa::path(
    post,
    path = "/links/{link_id}/tags",
    params(
        ("link_id" = i32, Path, description = "Link ID")
    ),
    request_body = AddTagsToLinkRequest,
    responses(
        (status = 200, description = "Tags added", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Tags"
)]
pub async fn add_tags_to_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(link_id): Path<i32>,
    Json(payload): Json<AddTagsToLinkRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    // Verify link exists, not deleted, and user has access
    let link = links::Entity::find_by_id(link_id)
        .filter(links::Column::DeletedAt.is_null())
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
                Json(serde_json::json!({"error": "Link not found"})),
            )
        })?;

    if link.user_id != Some(user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let mut added_count = 0;
    for tag_id in payload.tag_ids {
        // Check if tag belongs to user
        let tag = tags::Entity::find_by_id(tag_id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(tag) = tag {
            if tag.user_id == Some(user_id) || tag.org_id == link.org_id {
                // Check if already linked
                let existing = link_tags::Entity::find()
                    .filter(link_tags::Column::LinkId.eq(link_id))
                    .filter(link_tags::Column::TagId.eq(tag_id))
                    .one(&state.db)
                    .await
                    .ok()
                    .flatten();

                if existing.is_none() {
                    let link_tag = link_tags::ActiveModel {
                        link_id: Set(link_id),
                        tag_id: Set(tag_id),
                        ..Default::default()
                    };
                    let _ = link_tag.insert(&state.db).await;
                    added_count += 1;
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "added": added_count
    })))
}

/// Remove tags from a link
#[utoipa::path(
    delete,
    path = "/links/{link_id}/tags",
    params(
        ("link_id" = i32, Path, description = "Link ID")
    ),
    request_body = RemoveTagsFromLinkRequest,
    responses(
        (status = 200, description = "Tags removed", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Tags"
)]
pub async fn remove_tags_from_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(link_id): Path<i32>,
    Json(payload): Json<RemoveTagsFromLinkRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    // Verify link exists, not deleted, and user has access
    let link = links::Entity::find_by_id(link_id)
        .filter(links::Column::DeletedAt.is_null())
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
                Json(serde_json::json!({"error": "Link not found"})),
            )
        })?;

    if link.user_id != Some(user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let mut removed_count = 0;
    for tag_id in payload.tag_ids {
        let result = link_tags::Entity::delete_many()
            .filter(link_tags::Column::LinkId.eq(link_id))
            .filter(link_tags::Column::TagId.eq(tag_id))
            .exec(&state.db)
            .await;

        if let Ok(res) = result {
            removed_count += res.rows_affected;
        }
    }

    Ok(Json(serde_json::json!({
        "removed": removed_count
    })))
}

/// Get links by tag
#[utoipa::path(
    get,
    path = "/tags/{tag_id}/links",
    params(
        ("tag_id" = i32, Path, description = "Tag ID")
    ),
    responses(
        (status = 200, description = "Links with tag"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Tag not found"),
    ),
    tag = "Tags"
)]
pub async fn get_links_by_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tag_id): Path<i32>,
) -> Result<Json<Vec<crate::handlers::links::LinkResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    // Verify tag exists and user has access
    let tag = tags::Entity::find_by_id(tag_id)
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
                Json(serde_json::json!({"error": "Tag not found"})),
            )
        })?;

    // Check ownership - must own the tag directly, or be member of the org that owns it
    let has_access = if tag.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = tag.org_id {
        use crate::entity::org_members;
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };
    
    if !has_access {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    // Get link IDs with this tag
    let link_tags_list = link_tags::Entity::find()
        .filter(link_tags::Column::TagId.eq(tag_id))
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let link_ids: Vec<i32> = link_tags_list.iter().map(|lt| lt.link_id).collect();

    let links_list = links::Entity::find()
        .filter(links::Column::Id.is_in(link_ids))
        .filter(links::Column::DeletedAt.is_null())
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let base_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
    let api_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let responses: Vec<crate::handlers::links::LinkResponse> = links_list
        .into_iter()
        .map(|l| crate::handlers::links::LinkResponse {
            id: l.id,
            code: l.code.clone(),
            short_url: format!("{}/{}", base_url, l.code),
            api_url: format!("{}/{}", api_url, l.code),
            original_url: l.original_url.clone(),
            title: l.title.clone(),
            click_count: l.click_count,
            created_at: l.created_at.to_string(),
            expires_at: l.expires_at.map(|e| e.to_string()),
            has_password: l.password_hash.is_some(),
            notes: l.notes.clone(),
            folder_id: l.folder_id,
            org_id: l.org_id,
            starts_at: l.starts_at.map(|s| s.to_string()),
            max_clicks: l.max_clicks,
            is_active: l.is_active(),
            tags: vec![],
        })
        .collect();

    Ok(Json(responses))
}

