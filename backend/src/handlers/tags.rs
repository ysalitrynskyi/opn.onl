use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{link_tags, links, org_members, tags};
use crate::utils::time::utc_rfc3339;

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
    /// Tag IDs to attach. At most 50 are considered; duplicates are ignored.
    pub tag_ids: Vec<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RemoveTagsFromLinkRequest {
    pub tag_ids: Vec<i32>,
}

// ============= Helper Functions =============

async fn get_user_id_from_header(
    db: &sea_orm::DatabaseConnection,
    headers: &HeaderMap,
) -> Option<i32> {
    // Delegate to the shared resolver (handles both JWT and `opn_` API keys).
    crate::handlers::links::get_user_id_from_header(db, headers).await
}

/// Organization ownership always wins over the legacy `user_id` creator field.
/// A removed creator must not retain access to an organization tag.
async fn can_view_tag(db: &sea_orm::DatabaseConnection, tag: &tags::Model, user_id: i32) -> bool {
    match tag.org_id {
        Some(org_id) => org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(db)
            .await
            .ok()
            .flatten()
            .is_some(),
        None => tag.user_id == Some(user_id),
    }
}

async fn can_edit_tag(db: &sea_orm::DatabaseConnection, tag: &tags::Model, user_id: i32) -> bool {
    match tag.org_id {
        Some(org_id) => crate::handlers::organizations::member_can_edit(db, org_id, user_id).await,
        None => tag.user_id == Some(user_id),
    }
}

async fn can_edit_link(
    db: &sea_orm::DatabaseConnection,
    link: &links::Model,
    user_id: i32,
) -> bool {
    match link.org_id {
        Some(org_id) => crate::handlers::organizations::member_can_edit(db, org_id, user_id).await,
        None => link.user_id == Some(user_id),
    }
}

/// Tags and links cannot cross personal/organization ownership boundaries.
/// In particular, `None == None` is not enough for personal tags: the caller
/// must own the tag.
fn tag_matches_link_scope(tag: &tags::Model, link: &links::Model, user_id: i32) -> bool {
    match link.org_id {
        Some(org_id) => tag.org_id == Some(org_id),
        None => tag.org_id.is_none() && tag.user_id == Some(user_id),
    }
}

/// Count links carrying a tag, excluding soft-deleted links so the reported
/// `link_count` matches what the tag's link listing actually shows. Counting
/// raw `link_tags` rows over-counts, since deleting a link is a soft delete that
/// leaves its tag rows in place.
async fn count_active_tagged_links(db: &sea_orm::DatabaseConnection, tag_id: i32) -> i64 {
    let link_ids: Vec<i32> = link_tags::Entity::find()
        .filter(link_tags::Column::TagId.eq(tag_id))
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|lt| lt.link_id)
        .collect();
    if link_ids.is_empty() {
        return 0;
    }
    links::Entity::find()
        .filter(links::Column::Id.is_in(link_ids))
        .filter(links::Column::DeletedAt.is_null())
        .count(db)
        .await
        .unwrap_or(0) as i64
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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
        })?;

    // Org tags can only be created by members with edit rights (not viewers).
    if let Some(org_id) = payload.org_id
        && !crate::handlers::organizations::member_can_edit(&state.db, org_id, user_id).await
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                serde_json::json!({"error": "Insufficient permissions to create an organization tag"}),
            ),
        ));
    }

    let tag = tags::ActiveModel {
        name: Set(payload.name.clone()),
        color: Set(payload.color.clone()),
        user_id: Set(if payload.org_id.is_some() {
            None
        } else {
            Some(user_id)
        }),
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
            created_at: utc_rfc3339(tag.created_at),
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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    let tag_ids: Vec<i32> = tags_list.iter().map(|t| t.id).collect();
    let mut link_counts: HashMap<i32, i64> = HashMap::new();
    if !tag_ids.is_empty() {
        let rows: Vec<(i32, i64)> = link_tags::Entity::find()
            .select_only()
            .column(link_tags::Column::TagId)
            .column_as(link_tags::Column::LinkId.count(), "cnt")
            .inner_join(links::Entity)
            .filter(link_tags::Column::TagId.is_in(tag_ids))
            .filter(links::Column::DeletedAt.is_null())
            .group_by(link_tags::Column::TagId)
            .into_tuple()
            .all(&state.db)
            .await
            .unwrap_or_default();
        link_counts.extend(rows);
    }

    let responses = tags_list
        .into_iter()
        .map(|tag| TagResponse {
            id: tag.id,
            name: tag.name.clone(),
            color: tag.color.clone(),
            user_id: tag.user_id,
            org_id: tag.org_id,
            created_at: utc_rfc3339(tag.created_at),
            link_count: link_counts.get(&tag.id).copied().unwrap_or(0),
        })
        .collect();

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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    if !can_view_tag(&state.db, &tag, user_id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let link_count = count_active_tagged_links(&state.db, tag.id).await;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name.clone(),
        color: tag.color.clone(),
        user_id: tag.user_id,
        org_id: tag.org_id,
        created_at: utc_rfc3339(tag.created_at),
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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    if !can_edit_tag(&state.db, &tag, user_id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Insufficient permissions"})),
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

    let link_count = count_active_tagged_links(&state.db, tag.id).await;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name.clone(),
        color: tag.color.clone(),
        user_id: tag.user_id,
        org_id: tag.org_id,
        created_at: utc_rfc3339(tag.created_at),
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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    if !can_edit_tag(&state.db, &tag, user_id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Insufficient permissions"})),
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
    path = "/links/{id}/tags",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    request_body = AddTagsToLinkRequest,
    responses(
        (status = 200, description = "Tags added (`{\"added\": N}`)"),
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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    if !can_edit_link(&state.db, &link, user_id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    const MAX_TAGS_PER_ATTACH: usize = 50;
    let mut seen = HashSet::new();
    let tag_ids: Vec<i32> = payload
        .tag_ids
        .into_iter()
        .take(MAX_TAGS_PER_ATTACH)
        .filter(|id| seen.insert(*id))
        .collect();

    let mut added_count = 0;
    if !tag_ids.is_empty() {
        let loaded = tags::Entity::find()
            .filter(tags::Column::Id.is_in(tag_ids))
            .all(&state.db)
            .await
            .unwrap_or_default();
        let in_scope: Vec<i32> = loaded
            .into_iter()
            .filter(|tag| tag_matches_link_scope(tag, &link, user_id))
            .map(|tag| tag.id)
            .collect();

        if !in_scope.is_empty() {
            let existing: HashSet<i32> = link_tags::Entity::find()
                .filter(link_tags::Column::LinkId.eq(link_id))
                .filter(link_tags::Column::TagId.is_in(in_scope.clone()))
                .all(&state.db)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|lt| lt.tag_id)
                .collect();

            let to_insert: Vec<link_tags::ActiveModel> = in_scope
                .into_iter()
                .filter(|id| !existing.contains(id))
                .map(|tag_id| link_tags::ActiveModel {
                    link_id: Set(link_id),
                    tag_id: Set(tag_id),
                    ..Default::default()
                })
                .collect();
            added_count = to_insert.len();
            if !to_insert.is_empty() {
                let _ = link_tags::Entity::insert_many(to_insert)
                    .exec(&state.db)
                    .await;
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
    path = "/links/{id}/tags",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    request_body = RemoveTagsFromLinkRequest,
    responses(
        (status = 200, description = "Tags removed (`{\"removed\": N}`)"),
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
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    if !can_edit_link(&state.db, &link, user_id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let mut removed_count = 0;
    for tag_id in payload.tag_ids {
        let tag = tags::Entity::find_by_id(tag_id)
            .one(&state.db)
            .await
            .ok()
            .flatten();
        if !tag
            .as_ref()
            .is_some_and(|tag| tag_matches_link_scope(tag, &link, user_id))
        {
            continue;
        }

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
        (status = 200, description = "Non-deleted links with this tag"),
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
) -> Result<Json<Vec<crate::handlers::links::LinkResponse>>, (StatusCode, Json<serde_json::Value>)>
{
    let user_id = get_user_id_from_header(&state.db, &headers)
        .await
        .ok_or_else(|| {
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

    if !can_view_tag(&state.db, &tag, user_id).await {
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

    let mut links_query = links::Entity::find()
        .filter(links::Column::Id.is_in(link_ids))
        .filter(links::Column::DeletedAt.is_null());
    links_query = match tag.org_id {
        Some(org_id) => links_query.filter(links::Column::OrgId.eq(org_id)),
        None => links_query
            .filter(links::Column::OrgId.is_null())
            .filter(links::Column::UserId.eq(user_id)),
    };

    let links_list = links_query.all(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    let base_url =
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
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
            created_at: utc_rfc3339(l.created_at),
            expires_at: l.expires_at.map(utc_rfc3339),
            has_password: l.password_hash.is_some(),
            notes: l.notes.clone(),
            folder_id: l.folder_id,
            org_id: l.org_id,
            starts_at: l.starts_at.map(utc_rfc3339),
            max_clicks: l.max_clicks,
            burn_after_reading: l.burn_after_reading,
            burned_at: l.burned_at.map(utc_rfc3339),
            safe_link_interstitial: l.safe_link_interstitial,
            bio_visible: l.bio_visible,
            is_active: l.is_active(),
            is_pinned: l.is_pinned,
            tags: vec![],
        })
        .collect();

    Ok(Json(responses))
}
