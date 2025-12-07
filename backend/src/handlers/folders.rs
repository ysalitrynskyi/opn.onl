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
use crate::entity::{folders, links, link_tags, tags};
use crate::utils::decode_jwt;
use crate::handlers::links::TagInfo;

// ============= DTOs =============

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    pub name: String,
    pub color: Option<String>,
    pub org_id: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFolderRequest {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct FolderQuery {
    pub org_id: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FolderResponse {
    pub id: i32,
    pub name: String,
    pub color: Option<String>,
    pub user_id: Option<i32>,
    pub org_id: Option<i32>,
    pub created_at: String,
    pub link_count: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveLinkToFolderRequest {
    pub link_ids: Vec<i32>,
}

// ============= Helper Functions =============

fn get_user_id_from_header(headers: &HeaderMap) -> Option<i32> {
    let auth_header = headers.get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let claims = decode_jwt(token).ok()?;
    Some(claims.user_id)
}

async fn get_link_tags(db: &sea_orm::DatabaseConnection, link_id: i32) -> Vec<TagInfo> {
    let link_tags_list = link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.eq(link_id))
        .all(db)
        .await
        .unwrap_or_default();

    let tag_ids: Vec<i32> = link_tags_list.iter().map(|lt| lt.tag_id).collect();

    if tag_ids.is_empty() {
        return vec![];
    }

    let tags_list = tags::Entity::find()
        .filter(tags::Column::Id.is_in(tag_ids))
        .all(db)
        .await
        .unwrap_or_default();

    tags_list.into_iter().map(|t| TagInfo {
        id: t.id,
        name: t.name,
        color: t.color,
    }).collect()
}

// ============= Handlers =============

/// Create a new folder
#[utoipa::path(
    post,
    path = "/folders",
    request_body = CreateFolderRequest,
    responses(
        (status = 201, description = "Folder created", body = FolderResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Folders"
)]
pub async fn create_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<FolderResponse>), (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let folder = folders::ActiveModel {
        name: Set(payload.name.clone()),
        color: Set(payload.color.clone()),
        user_id: Set(if payload.org_id.is_some() { None } else { Some(user_id) }),
        org_id: Set(payload.org_id),
        ..Default::default()
    };

    let folder = folder.insert(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to create folder"})),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(FolderResponse {
            id: folder.id,
            name: folder.name,
            color: folder.color,
            user_id: folder.user_id,
            org_id: folder.org_id,
            created_at: folder.created_at.to_string(),
            link_count: 0,
        }),
    ))
}

/// Get user's folders
#[utoipa::path(
    get,
    path = "/folders",
    params(FolderQuery),
    responses(
        (status = 200, description = "List of folders", body = Vec<FolderResponse>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Folders"
)]
pub async fn get_folders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FolderQuery>,
) -> Result<Json<Vec<FolderResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let mut folder_query = folders::Entity::find();

    if let Some(org_id) = query.org_id {
        // Verify user is member of this org before listing its folders
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
        
        folder_query = folder_query.filter(folders::Column::OrgId.eq(org_id));
    } else {
        folder_query = folder_query.filter(folders::Column::UserId.eq(user_id));
    }

    let folders = folder_query
        .order_by_asc(folders::Column::Name)
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let mut responses = Vec::new();
    for folder in folders {
        let link_count = links::Entity::find()
            .filter(links::Column::FolderId.eq(folder.id))
            .count(&state.db)
            .await
            .unwrap_or(0) as i64;

        responses.push(FolderResponse {
            id: folder.id,
            name: folder.name.clone(),
            color: folder.color.clone(),
            user_id: folder.user_id,
            org_id: folder.org_id,
            created_at: folder.created_at.to_string(),
            link_count,
        });
    }

    Ok(Json(responses))
}

/// Get folder by ID
#[utoipa::path(
    get,
    path = "/folders/{folder_id}",
    params(
        ("folder_id" = i32, Path, description = "Folder ID")
    ),
    responses(
        (status = 200, description = "Folder details", body = FolderResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Folders"
)]
pub async fn get_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(folder_id): Path<i32>,
) -> Result<Json<FolderResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let folder = folders::Entity::find_by_id(folder_id)
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
                Json(serde_json::json!({"error": "Folder not found"})),
            )
        })?;

    // Check ownership - must own the folder directly, or be member of the org that owns it
    let has_access = if folder.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = folder.org_id {
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

    let link_count = links::Entity::find()
        .filter(links::Column::FolderId.eq(folder.id))
        .filter(links::Column::DeletedAt.is_null())
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(FolderResponse {
        id: folder.id,
        name: folder.name.clone(),
        color: folder.color.clone(),
        user_id: folder.user_id,
        org_id: folder.org_id,
        created_at: folder.created_at.to_string(),
        link_count,
    }))
}

/// Update folder
#[utoipa::path(
    put,
    path = "/folders/{folder_id}",
    params(
        ("folder_id" = i32, Path, description = "Folder ID")
    ),
    request_body = UpdateFolderRequest,
    responses(
        (status = 200, description = "Folder updated", body = FolderResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Folders"
)]
pub async fn update_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(folder_id): Path<i32>,
    Json(payload): Json<UpdateFolderRequest>,
) -> Result<Json<FolderResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let folder = folders::Entity::find_by_id(folder_id)
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
                Json(serde_json::json!({"error": "Folder not found"})),
            )
        })?;

    // Check ownership - must own the folder directly, or be member of the org that owns it
    let has_access = if folder.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = folder.org_id {
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

    let mut folder: folders::ActiveModel = folder.into();

    if let Some(name) = payload.name {
        folder.name = Set(name);
    }
    if let Some(color) = payload.color {
        folder.color = Set(Some(color));
    }

    let folder = folder.update(&state.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update folder"})),
        )
    })?;

    let link_count = links::Entity::find()
        .filter(links::Column::FolderId.eq(folder.id))
        .count(&state.db)
        .await
        .unwrap_or(0) as i64;

    Ok(Json(FolderResponse {
        id: folder.id,
        name: folder.name.clone(),
        color: folder.color.clone(),
        user_id: folder.user_id,
        org_id: folder.org_id,
        created_at: folder.created_at.to_string(),
        link_count,
    }))
}

/// Delete folder
#[utoipa::path(
    delete,
    path = "/folders/{folder_id}",
    params(
        ("folder_id" = i32, Path, description = "Folder ID")
    ),
    responses(
        (status = 204, description = "Folder deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Folders"
)]
pub async fn delete_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(folder_id): Path<i32>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    let folder = folders::Entity::find_by_id(folder_id)
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
                Json(serde_json::json!({"error": "Folder not found"})),
            )
        })?;

    // Check ownership - must own the folder directly, or be member of the org that owns it
    let has_access = if folder.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = folder.org_id {
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

    // Clear folder_id on all links in this folder before deleting
    use sea_orm::sea_query::Expr;
    links::Entity::update_many()
        .col_expr(links::Column::FolderId, Expr::value(Option::<i32>::None))
        .filter(links::Column::FolderId.eq(folder_id))
        .exec(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to update links"})),
            )
        })?;

    folders::Entity::delete_by_id(folder_id)
        .exec(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to delete folder"})),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Move links to folder
#[utoipa::path(
    post,
    path = "/folders/{folder_id}/links",
    params(
        ("folder_id" = i32, Path, description = "Folder ID")
    ),
    request_body = MoveLinkToFolderRequest,
    responses(
        (status = 200, description = "Links moved", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    tag = "Folders"
)]
pub async fn move_links_to_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(folder_id): Path<i32>,
    Json(payload): Json<MoveLinkToFolderRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    // Verify folder exists and user has access
    let folder = folders::Entity::find_by_id(folder_id)
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
                Json(serde_json::json!({"error": "Folder not found"})),
            )
        })?;

    // Check folder ownership - must own directly or be member of org
    let folder_access = if folder.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = folder.org_id {
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
    
    if !folder_access {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    let mut moved_count = 0;
    for link_id in payload.link_ids {
        let link = links::Entity::find_by_id(link_id)
            .filter(links::Column::DeletedAt.is_null())
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link {
            // Check link ownership - must own directly or be member of same org
            let link_access = if link.user_id == Some(user_id) {
                true
            } else if let Some(link_org) = link.org_id {
                folder.org_id == Some(link_org) // Only if moving within same org
            } else {
                false
            };
            
            if link_access {
                let mut link: links::ActiveModel = link.into();
                link.folder_id = Set(Some(folder_id));
                let _ = link.update(&state.db).await;
                moved_count += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "moved": moved_count
    })))
}

/// Get links in folder
#[utoipa::path(
    get,
    path = "/folders/{folder_id}/links",
    params(
        ("folder_id" = i32, Path, description = "Folder ID")
    ),
    responses(
        (status = 200, description = "Links in folder"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    tag = "Folders"
)]
pub async fn get_folder_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(folder_id): Path<i32>,
) -> Result<Json<Vec<crate::handlers::links::LinkResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let user_id = get_user_id_from_header(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Unauthorized"})),
        )
    })?;

    // Verify folder exists and user has access
    let folder = folders::Entity::find_by_id(folder_id)
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
                Json(serde_json::json!({"error": "Folder not found"})),
            )
        })?;

    // Check ownership - must own the folder directly, or be member of the org that owns it
    let has_access = if folder.user_id == Some(user_id) {
        true
    } else if let Some(org_id) = folder.org_id {
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

    let links_list = links::Entity::find()
        .filter(links::Column::FolderId.eq(folder_id))
        .filter(links::Column::DeletedAt.is_null())
        .order_by_desc(links::Column::CreatedAt)
        .all(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
        })?;

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let mut responses = Vec::new();
    for l in links_list {
        let link_tags = get_link_tags(&state.db, l.id).await;
        responses.push(crate::handlers::links::LinkResponse {
            id: l.id,
            code: l.code.clone(),
            short_url: format!("{}/{}", base_url, l.code),
            original_url: l.original_url.clone(),
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
            tags: link_tags,
        });
    }

    Ok(Json(responses))
}

