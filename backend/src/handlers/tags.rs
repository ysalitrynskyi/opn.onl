use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use sea_orm::sea_query::OnConflict;
use sea_orm::sea_query::SelectStatement;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{link_tags, links, org_members, tags};
use crate::handlers::link_access::{LinkAction, live_link_for};
use crate::handlers::links::{LinkPage, LinkPageQuery, link_page, page_links};
use crate::utils::time::utc_rfc3339;
use crate::utils::validation::{check_id_list, check_name, check_optional_color};

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
    /// Tag IDs to attach. At most 500; duplicates are ignored.
    pub tag_ids: Vec<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RemoveTagsFromLinkRequest {
    /// Tag IDs to detach. At most 500; duplicates are ignored.
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

/// Tags and links cannot cross personal/organization ownership boundaries.
/// In particular, `None == None` is not enough for personal tags: the caller
/// must own the tag.
fn tag_matches_link_scope(tag: &tags::Model, link: &links::Model, user_id: i32) -> bool {
    match link.org_id {
        Some(org_id) => tag.org_id == Some(org_id),
        None => tag.org_id.is_none() && tag.user_id == Some(user_id),
    }
}

/// The ids of every link carrying a tag, as a subquery for `links.id IN (...)`.
/// A tag can sit on more links than one statement has bind parameters for, so
/// the ids are never loaded and bound.
pub(crate) fn link_ids_with_tag(tag_id: i32) -> SelectStatement {
    sea_orm::sea_query::Query::select()
        .column(link_tags::Column::LinkId)
        .from(link_tags::Entity)
        .and_where(link_tags::Column::TagId.eq(tag_id))
        .to_owned()
}

/// Count links carrying a tag, excluding soft-deleted links so the reported
/// `link_count` matches what the tag's link listing actually shows. Counting
/// raw `link_tags` rows over-counts, since deleting a link is a soft delete that
/// leaves its tag rows in place. A database error is returned: a count of 0 for
/// a tag in use would read as "unused".
async fn count_active_tagged_links(
    db: &sea_orm::DatabaseConnection,
    tag_id: i32,
) -> Result<i64, DbErr> {
    let count = links::Entity::find()
        .filter(links::Column::Id.in_subquery(link_ids_with_tag(tag_id)))
        .filter(links::Column::DeletedAt.is_null())
        .count(db)
        .await?;
    Ok(count as i64)
}

fn db_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "Database error"})),
    )
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
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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

    if let Err(e) = check_name("Tag name", &payload.name)
        .and_then(|()| check_optional_color(payload.color.as_deref()))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ));
    }

    let tag = tags::ActiveModel {
        name: Set(payload.name.clone()),
        color: Set(payload.color.clone().filter(|c| !c.is_empty())),
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
        (status = 500, description = "The tags could not be read"),
    ),
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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
            .map_err(|_| db_error())?;
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
        (status = 500, description = "The tag could not be read"),
    ),
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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

    let link_count = count_active_tagged_links(&state.db, tag.id)
        .await
        .map_err(|_| db_error())?;

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
        (status = 500, description = "The tag could not be updated"),
    ),
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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

    if let Err(e) = payload
        .name
        .as_deref()
        .map_or(Ok(()), |name| check_name("Tag name", name))
        .and_then(|()| check_optional_color(payload.color.as_deref()))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ));
    }

    let mut tag: tags::ActiveModel = tag.into();

    if let Some(name) = payload.name {
        tag.name = Set(name);
    }
    if let Some(color) = payload.color {
        tag.color = Set(Some(color).filter(|c| !c.is_empty()));
    }

    let tag = tag.update(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update tag"})),
        )
    })?;

    let link_count = count_active_tagged_links(&state.db, tag.id)
        .await
        .map_err(|_| db_error())?;

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
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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
        (status = 200, description = "Tags added (`{\"added\": N}`). `added` counts the tags this request attached; ones already on the link, unknown, or out of scope for it are not counted"),
        (status = 400, description = "More than 500 tag ids"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Missing, deleted, or not yours to change (the same answer for all three)"),
        (status = 500, description = "The tags could not be added"),
    ),
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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

    let tag_ids = check_id_list(payload.tag_ids).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
    })?;

    // A missing, deleted or foreign link is the same 403, so ids cannot be probed.
    let link = live_link_for(&state.db, link_id, user_id, LinkAction::Write).await?;

    let mut added_count = 0;
    if !tag_ids.is_empty() {
        let loaded = tags::Entity::find()
            .filter(tags::Column::Id.is_in(tag_ids))
            .all(&state.db)
            .await
            .map_err(|_| db_error())?;
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
                .map_err(|_| db_error())?
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
            if !to_insert.is_empty() {
                // A concurrent request may attach the same tag between the
                // lookup above and this insert. The unique index would reject
                // the whole statement, so a clash is skipped instead, and the
                // reply counts the rows this request really wrote.
                added_count = link_tags::Entity::insert_many(to_insert)
                    .on_conflict(
                        OnConflict::columns([link_tags::Column::LinkId, link_tags::Column::TagId])
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec_without_returning(&state.db)
                    .await
                    .map_err(|_| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Failed to add tags"})),
                        )
                    })?;
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
        (status = 400, description = "More than 500 tag ids"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Missing, deleted, or not yours to change (the same answer for all three)"),
    ),
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
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

    let tag_ids = check_id_list(payload.tag_ids).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
    })?;

    // A missing, deleted or foreign link is the same 403, so ids cannot be probed.
    let link = live_link_for(&state.db, link_id, user_id, LinkAction::Write).await?;

    // Only tags the link could carry are touched, whatever ids were sent.
    let mut removed_count = 0;
    if !tag_ids.is_empty() {
        let in_scope: Vec<i32> = tags::Entity::find()
            .filter(tags::Column::Id.is_in(tag_ids))
            .all(&state.db)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Database error"})),
                )
            })?
            .into_iter()
            .filter(|tag| tag_matches_link_scope(tag, &link, user_id))
            .map(|tag| tag.id)
            .collect();

        if !in_scope.is_empty() {
            removed_count = link_tags::Entity::delete_many()
                .filter(link_tags::Column::LinkId.eq(link_id))
                .filter(link_tags::Column::TagId.is_in(in_scope))
                .exec(&state.db)
                .await
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Failed to remove tags"})),
                    )
                })?
                .rows_affected;
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
        ("tag_id" = i32, Path, description = "Tag ID"),
        LinkPageQuery
    ),
    responses(
        (
            status = 200,
            description = "Non-deleted links with this tag, each with its tags, newest first (limit defaults to 1000, maximum 1000)",
            body = Vec<LinkResponse>,
            headers(("X-Total-Count" = u64, description = "How many links carry the tag, ignoring `limit` and `offset`"))
        ),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Tag not found"),
        (status = 500, description = "The links could not be read"),
    ),
    tag = "Tags",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_links_by_tag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tag_id): Path<i32>,
    Query(page): Query<LinkPageQuery>,
) -> Result<LinkPage, (StatusCode, Json<serde_json::Value>)> {
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

    let mut links_query = links::Entity::find()
        .filter(links::Column::Id.in_subquery(link_ids_with_tag(tag_id)))
        .filter(links::Column::DeletedAt.is_null());
    links_query = match tag.org_id {
        Some(org_id) => links_query.filter(links::Column::OrgId.eq(org_id)),
        None => links_query
            .filter(links::Column::OrgId.is_null())
            .filter(links::Column::UserId.eq(user_id)),
    };

    let total = links_query.clone().count(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    // Same paging as GET /links; `id` keeps rows created in the same instant
    // in one order across pages.
    let links_query = links_query
        .order_by_desc(links::Column::CreatedAt)
        .order_by_desc(links::Column::Id);
    let links_list = page_links(links_query, page.limit, page.offset)
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let link_ids: Vec<i32> = links_list.iter().map(|l| l.id).collect();
    let mut tags_by_link = crate::handlers::links::get_tags_by_link_ids(&state.db, &link_ids)
        .await
        .map_err(|_| db_error())?;

    let base_url = crate::handlers::links::get_base_url();
    let api_url = crate::handlers::links::get_api_url();
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
            bio_label: l.bio_label.clone(),
            bio_position: l.bio_position,
            is_active: l.is_active(),
            is_pinned: l.is_pinned,
            tags: tags_by_link.remove(&l.id).unwrap_or_default(),
        })
        .collect();

    Ok(link_page(total, responses))
}
