use axum::{
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap},
    Json,
    response::{IntoResponse, Redirect},
};
use serde::{Deserialize, Serialize};
use sea_orm::*;
use validator::Validate;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use chrono::{DateTime, Utc};
use bcrypt::{hash, DEFAULT_COST};
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{links, click_events, link_tags, tags, blocked_links, blocked_domains};
use crate::utils::jwt::decode_jwt;
use crate::utils::geoip::{lookup_ip, parse_user_agent};
use crate::handlers::websocket::ClickEvent;

/// Check if URL or its domain is blocked
async fn check_blocked(db: &DatabaseConnection, url: &str) -> Result<(), String> {
    // Parse URL to get domain
    let parsed_url = url::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    let domain = parsed_url.host_str().unwrap_or("");
    
    // Check if exact URL is blocked
    let blocked_url = blocked_links::Entity::find()
        .filter(blocked_links::Column::Url.eq(url))
        .one(db)
        .await
        .ok()
        .flatten();
    
    if let Some(blocked) = blocked_url {
        return Err(format!("This URL is blocked: {}", blocked.reason.unwrap_or_else(|| "Policy violation".to_string())));
    }
    
    // Check if domain is blocked (including subdomains)
    let blocked_domain = blocked_domains::Entity::find()
        .all(db)
        .await
        .unwrap_or_default();
    
    for bd in blocked_domain {
        if domain == bd.domain || domain.ends_with(&format!(".{}", bd.domain)) {
            return Err(format!("This domain is blocked: {}", bd.reason.unwrap_or_else(|| "Policy violation".to_string())));
        }
    }
    
    Ok(())
}

// ============= DTOs =============

#[derive(Deserialize, Validate, ToSchema)]
pub struct CreateLinkRequest {
    #[validate(url)]
    #[serde(default)]
    pub original_url: String,
    #[validate(length(min = 3, max = 20))]
    pub custom_alias: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub password: Option<String>,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub starts_at: Option<DateTime<Utc>>,
    pub max_clicks: Option<i32>,
    pub tag_ids: Option<Vec<i32>>,
}

#[derive(Deserialize, Validate, ToSchema)]
pub struct UpdateLinkRequest {
    #[validate(url)]
    pub original_url: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub password: Option<String>,
    pub remove_password: Option<bool>,
    pub remove_expiration: Option<bool>,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub starts_at: Option<DateTime<Utc>>,
    pub max_clicks: Option<i32>,
    pub remove_starts_at: Option<bool>,
    pub remove_max_clicks: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkCreateLinkRequest {
    pub urls: Vec<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
}

fn is_valid_url(url: &str) -> bool {
    url::Url::parse(url).map(|u| u.scheme() == "http" || u.scheme() == "https").unwrap_or(false)
}

#[derive(Deserialize, ToSchema)]
pub struct BulkDeleteRequest {
    pub ids: Vec<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkUpdateRequest {
    pub ids: Vec<i32>,
    pub folder_id: Option<i32>,
    pub expires_at: Option<DateTime<Utc>>,
    pub remove_expiration: Option<bool>,
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct LinksQuery {
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub tag_id: Option<i32>,
    pub search: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkCreateLinkResponse {
    pub links: Vec<CreateLinkResponse>,
    pub errors: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateLinkResponse {
    pub id: i32,
    pub code: String,
    pub short_url: String,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TagInfo {
    pub id: i32,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct LinkResponse {
    pub id: i32,
    pub code: String,
    pub short_url: String,
    pub original_url: String,
    pub click_count: i32,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub has_password: bool,
    pub notes: Option<String>,
    pub folder_id: Option<i32>,
    pub org_id: Option<i32>,
    pub starts_at: Option<String>,
    pub max_clicks: Option<i32>,
    pub is_active: bool,
    pub tags: Vec<TagInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize, ToSchema)]
pub struct SuccessResponse {
    pub message: String,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyPasswordRequest {
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct BulkDeleteResponse {
    pub deleted: u64,
}

#[derive(Serialize, ToSchema)]
pub struct BulkUpdateResponse {
    pub updated: u64,
}

// ============= Helper Functions =============

fn generate_short_code() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect()
}

pub fn get_user_id_from_header(headers: &HeaderMap) -> Option<i32> {
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = &auth_str[7..];
                if let Ok(claims) = decode_jwt(token) {
                    return Some(claims.user_id);
                }
            }
        }
    }
    None
}

fn get_base_url() -> String {
    std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

async fn get_link_tags(db: &DatabaseConnection, link_id: i32) -> Vec<TagInfo> {
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

/// Create a new shortened link
#[utoipa::path(
    post,
    path = "/links",
    request_body = CreateLinkRequest,
    responses(
        (status = 201, description = "Link created", body = LinkResponse),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Alias already exists"),
    ),
    tag = "Links"
)]
pub async fn create_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateLinkRequest>,
) -> impl IntoResponse {
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response();
    }

    let user_id = get_user_id_from_header(&headers);

    // Check if custom aliases are enabled
    let custom_aliases_enabled = std::env::var("ENABLE_CUSTOM_ALIASES")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);
    
    // Check if reusing deleted slugs is allowed
    let allow_deleted_slug_reuse = std::env::var("ALLOW_DELETED_SLUG_REUSE")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let code = if let Some(alias) = payload.custom_alias {
        // Check if custom aliases are enabled
        if !custom_aliases_enabled {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Custom aliases are disabled".to_string() })).into_response();
        }
        
        // Check if alias exists (active links)
        let exists_active = links::Entity::find()
            .filter(links::Column::Code.eq(&alias))
            .filter(links::Column::DeletedAt.is_null())
            .one(&state.db)
            .await
            .unwrap_or(None);
        
        if exists_active.is_some() {
            return (StatusCode::CONFLICT, Json(ErrorResponse { error: "Alias already taken".to_string() })).into_response();
        }
        
        // Check if alias was used by a deleted link
        if !allow_deleted_slug_reuse {
            let exists_deleted = links::Entity::find()
                .filter(links::Column::Code.eq(&alias))
                .filter(links::Column::DeletedAt.is_not_null())
                .one(&state.db)
                .await
                .unwrap_or(None);
            
            if exists_deleted.is_some() {
                return (StatusCode::CONFLICT, Json(ErrorResponse { error: "This alias was previously used and cannot be reused".to_string() })).into_response();
            }
        }
        
        // Check if URL or domain is blocked
        if let Err(e) = check_blocked(&state.db, &payload.original_url).await {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
        }
        
        alias
    } else {
        // Check if URL or domain is blocked
        if let Err(e) = check_blocked(&state.db, &payload.original_url).await {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
        }
        
        let mut code = generate_short_code();
        while links::Entity::find().filter(links::Column::Code.eq(&code)).one(&state.db).await.unwrap_or(None).is_some() {
            code = generate_short_code();
        }
        code
    };

    let password_hash = if let Some(password) = &payload.password {
        match hash(password, DEFAULT_COST) {
            Ok(h) => Some(h),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password").into_response(),
        }
    } else {
        None
    };

    // If org_id is provided, verify user is a member
    if let Some(org_id) = payload.org_id {
        if let Some(uid) = user_id {
            use crate::entity::org_members;
            let is_member = org_members::Entity::find()
                .filter(org_members::Column::OrgId.eq(org_id))
                .filter(org_members::Column::UserId.eq(uid))
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .is_some();
            
            if !is_member {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Not a member of this organization".to_string() })).into_response();
            }
        } else {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Authentication required to create org links".to_string() })).into_response();
        }
    }

    let link = links::ActiveModel {
        original_url: Set(payload.original_url.clone()),
        code: Set(code.clone()),
        user_id: Set(user_id),
        expires_at: Set(payload.expires_at.map(|d| d.naive_utc())),
        password_hash: Set(password_hash.clone()),
        notes: Set(payload.notes.clone()),
        folder_id: Set(payload.folder_id),
        org_id: Set(payload.org_id),
        starts_at: Set(payload.starts_at.map(|d| d.naive_utc())),
        max_clicks: Set(payload.max_clicks),
        ..Default::default()
    };

    let result = links::Entity::insert(link).exec(&state.db).await;

    match result {
        Ok(link_res) => {
            let link_id = link_res.last_insert_id;

            // Add tags if provided
            if let Some(tag_ids) = payload.tag_ids {
                for tag_id in tag_ids {
                    let link_tag = link_tags::ActiveModel {
                        link_id: Set(link_id),
                        tag_id: Set(tag_id),
                        ..Default::default()
                    };
                    let _ = link_tag.insert(&state.db).await;
                }
            }

            let tags = get_link_tags(&state.db, link_id).await;

            let base_url = get_base_url();
            (StatusCode::CREATED, Json(LinkResponse {
                id: link_id,
                code: code.clone(),
                short_url: format!("{}/{}", base_url, code),
                original_url: payload.original_url,
                click_count: 0,
                created_at: chrono::Utc::now().to_rfc3339(),
                expires_at: payload.expires_at.map(|d| d.to_rfc3339()),
                has_password: password_hash.is_some(),
                notes: payload.notes,
                folder_id: payload.folder_id,
                org_id: payload.org_id,
                starts_at: payload.starts_at.map(|d| d.to_rfc3339()),
                max_clicks: payload.max_clicks,
                is_active: true,
                tags,
            })).into_response()
        }
        Err(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response()
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LinkPreviewResponse {
    pub code: String,
    pub short_url: String,
    pub original_url: String,
    pub domain: String,
    pub has_password: bool,
    pub is_expired: bool,
    pub created_at: String,
    pub click_count: i32,
}

/// Get link preview (add + to any short link URL to see preview)
#[utoipa::path(
    get,
    path = "/{code}/preview",
    params(
        ("code" = String, Path, description = "Short link code")
    ),
    responses(
        (status = 200, description = "Link preview", body = LinkPreviewResponse),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn preview_link(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    // Remove trailing + if present (for URL compatibility)
    let clean_code = code.trim_end_matches('+');
    
    let link = links::Entity::find()
        .filter(links::Column::Code.eq(clean_code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    match link {
        Some(link) => {
            let is_expired = if let Some(exp) = link.expires_at {
                exp < Utc::now().naive_utc()
            } else {
                false
            };

            // Extract domain from original URL
            let domain = url::Url::parse(&link.original_url)
                .map(|u| u.host_str().unwrap_or("unknown").to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            let base_url = get_base_url();

            (StatusCode::OK, Json(LinkPreviewResponse {
                code: link.code.clone(),
                short_url: format!("{}/{}", base_url, link.code),
                original_url: link.original_url,
                domain,
                has_password: link.password_hash.is_some(),
                is_expired,
                created_at: link.created_at.to_string(),
                click_count: link.click_count,
            })).into_response()
        }
        None => {
            (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
        }
    }
}

/// Redirect to original URL
#[utoipa::path(
    get,
    path = "/{code}",
    params(
        ("code" = String, Path, description = "Short link code")
    ),
    responses(
        (status = 302, description = "Redirect to original URL"),
        (status = 401, description = "Password required"),
        (status = 404, description = "Link not found"),
        (status = 410, description = "Link expired or inactive"),
    ),
    tag = "Links"
)]
pub async fn redirect_link(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    use crate::utils::cache::CachedLink;
    
    // Try to get from Redis cache first (for non-password-protected links)
    if let Some(cache) = &state.redis_cache {
        if let Some(cached) = cache.get_link(&code).await {
            // Skip cache for password-protected links and max_clicks links (need precise counting)
            if !cached.has_password && cached.max_clicks.is_none() {
                // Check if link is active based on cached data
                let now = chrono::Utc::now().timestamp();
                
                if let Some(starts_at) = cached.starts_at {
                    if now < starts_at {
                        return (StatusCode::GONE, "Link is scheduled to activate later").into_response();
                    }
                }
                
                if let Some(expires_at) = cached.expires_at {
                    if now > expires_at {
                        return (StatusCode::GONE, "Link has expired").into_response();
                    }
                }
                
                if let Some(max_clicks) = cached.max_clicks {
                    if cached.click_count >= max_clicks {
                        return (StatusCode::GONE, "Link has reached maximum clicks").into_response();
                    }
                }
                
                // Record click using buffer (synchronous, non-blocking)
                record_click_buffered(
                    &state.click_buffer,
                    state.ws_state.as_ref().map(|w| w.as_ref()),
                    cached.id,
                    &code,
                    cached.user_id,
                    cached.click_count,
                    &headers,
                );
                
                // Invalidate cache after click
                let cache_clone = state.redis_cache.clone();
                let code_clone = code.clone();
                tokio::spawn(async move {
                    if let Some(c) = cache_clone {
                        let _ = c.invalidate_link(&code_clone).await;
                    }
                });
                
                return Redirect::temporary(&cached.original_url).into_response();
            }
        }
    }
    
    // Fallback to database lookup
    let link = links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        // Check if link is active
        if !link.is_active() {
            let reason = link.inactive_reason().unwrap_or("Link is inactive");
            return (StatusCode::GONE, reason).into_response();
        }

        if link.password_hash.is_some() {
            let provided_password = headers.get("x-link-password")
                .and_then(|h| h.to_str().ok());

            if let Some(pwd) = provided_password {
                if let Some(hash_str) = &link.password_hash {
                     if !bcrypt::verify(pwd, hash_str).unwrap_or(false) {
                         return (StatusCode::UNAUTHORIZED, "Invalid password").into_response();
                     }
                }
            } else {
                let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
                return Redirect::temporary(&format!("{}/password/{}", frontend_url, code)).into_response();
            }
        }

        // Cache the link for future requests (only non-password-protected and without max_clicks)
        if link.password_hash.is_none() && link.max_clicks.is_none() {
            if let Some(cache) = &state.redis_cache {
                let cached = CachedLink {
                    id: link.id,
                    original_url: link.original_url.clone(),
                    has_password: false,
                    expires_at: link.expires_at.map(|e| e.and_utc().timestamp()),
                    starts_at: link.starts_at.map(|s| s.and_utc().timestamp()),
                    max_clicks: link.max_clicks,
                    click_count: link.click_count,
                    user_id: link.user_id,
                };
                let _ = cache.set_link(&code, &cached).await;
            }
        }

        // Record click using buffer
        record_click_buffered(
            &state.click_buffer,
            state.ws_state.as_ref().map(|w| w.as_ref()),
            link.id,
            &code,
            link.user_id,
            link.click_count,
            &headers,
        );

        Redirect::temporary(&link.original_url).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Link not found").into_response()
    }
}

/// Helper function to record a click event using the click buffer
fn record_click_buffered(
    click_buffer: &crate::utils::ClickBuffer,
    ws_state: Option<&crate::handlers::websocket::WsState>,
    link_id: i32,
    link_code: &str,
    user_id: Option<i32>,
    current_click_count: i32,
    headers: &HeaderMap,
) {
    use crate::utils::click_buffer::ClickData;
    
    // Extract request info
    let ip = headers.get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .or_else(|| headers.get("x-real-ip").and_then(|h| h.to_str().ok()))
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
    
    let user_agent = headers.get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let referer = headers.get("referer")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // GeoIP lookup
    let geo = ip.as_ref().map(|ip| lookup_ip(ip)).unwrap_or_default();
    
    // Parse user agent
    let ua_info = user_agent.as_ref().map(|ua| parse_user_agent(ua)).unwrap_or_default();

    // Add to click buffer instead of writing directly
    let click_data = ClickData {
        link_id,
        ip_address: ip,
        user_agent,
        referer,
        country: geo.country.clone(),
        city: geo.city.clone(),
        region: geo.region,
        latitude: geo.latitude,
        longitude: geo.longitude,
        device: ua_info.device.clone(),
        browser: ua_info.browser.clone(),
        os: ua_info.os,
    };
    click_buffer.add_click(click_data);

    // Broadcast real-time event
    let new_click_count = current_click_count + 1;
    if let Some(ws) = ws_state {
        let event = ClickEvent {
            link_id,
            link_code: link_code.to_string(),
            user_id,
            click_count: new_click_count,
            country: geo.country,
            city: geo.city,
            device: ua_info.device,
            browser: ua_info.browser,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        ws.broadcast_click(event);
    }
}

/// Verify password for protected link
#[utoipa::path(
    post,
    path = "/{code}/verify",
    params(
        ("code" = String, Path, description = "Short link code")
    ),
    request_body = VerifyPasswordRequest,
    responses(
        (status = 200, description = "Password verified"),
        (status = 401, description = "Invalid password"),
        (status = 404, description = "Link not found"),
        (status = 410, description = "Link expired"),
    ),
    tag = "Links"
)]
pub async fn verify_link_password(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<VerifyPasswordRequest>,
) -> impl IntoResponse {
    let link = links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if !link.is_active() {
            let reason = link.inactive_reason().unwrap_or("Link is inactive");
            return (StatusCode::GONE, Json(ErrorResponse { error: reason.to_string() })).into_response();
        }

        if let Some(hash_str) = &link.password_hash {
            if bcrypt::verify(&payload.password, hash_str).unwrap_or(false) {
                // Use click buffer for consistent click tracking (same as regular redirects)
                record_click_buffered(
                    &state.click_buffer,
                    state.ws_state.as_ref().map(|w| w.as_ref()),
                    link.id,
                    &link.code,
                    link.user_id,
                    link.click_count,
                    &headers,
                );
                
                // For real-time broadcast, we need to get the info
                let ip = headers.get("x-forwarded-for")
                    .and_then(|h| h.to_str().ok())
                    .or_else(|| headers.get("x-real-ip").and_then(|h| h.to_str().ok()))
                    .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
                
                let user_agent = headers.get("user-agent")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());
                
                let geo = ip.as_ref().map(|ip| lookup_ip(ip)).unwrap_or_default();
                let ua_info = user_agent.as_ref().map(|ua| parse_user_agent(ua)).unwrap_or_default();
                
                let new_click_count = link.click_count + 1;

                // Broadcast real-time event
                if let Some(ws_state) = state.ws_state.as_ref() {
                    let event = ClickEvent {
                        link_id: link.id,
                        link_code: link.code.clone(),
                        user_id: link.user_id,
                        click_count: new_click_count,
                        country: geo.country,
                        city: geo.city,
                        device: ua_info.device,
                        browser: ua_info.browser,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    ws_state.broadcast_click(event);
                }

                return (StatusCode::OK, Json(serde_json::json!({ "url": link.original_url }))).into_response();
            } else {
                return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Invalid password".to_string() })).into_response();
            }
        } else {
            return (StatusCode::OK, Json(serde_json::json!({ "url": link.original_url }))).into_response();
        }
    }

    (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
}

/// Get QR code for a link
#[utoipa::path(
    get,
    path = "/links/{id}/qr",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "QR code image", content_type = "image/png"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Links"
)]
pub async fn get_qr_code(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    use qrcode::QrCode;
    use image::Luma;
    use std::io::Cursor;

    // Verify authentication
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        // Verify ownership (allow if user owns the link or it belongs to their org)
        let has_access = if link.user_id == Some(user_id) {
            true
        } else if let Some(org_id) = link.org_id {
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
            return (StatusCode::FORBIDDEN, "You don't have permission to access this link").into_response();
        }

        let url = format!("{}/{}", get_base_url(), link.code);
        
        let qr_code = match QrCode::new(url.as_bytes()) {
            Ok(code) => code,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate QR code").into_response(),
        };
        
        let image = qr_code.render::<Luma<u8>>().build();

        let mut buffer = Cursor::new(Vec::new());
        if image.write_to(&mut buffer, image::ImageFormat::Png).is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to encode QR code image").into_response();
        }

        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            buffer.into_inner(),
        ).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Link not found").into_response()
    }
}

/// Get user's links with filtering
#[utoipa::path(
    get,
    path = "/links",
    params(LinksQuery),
    responses(
        (status = 200, description = "List of links", body = Vec<LinkResponse>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn get_user_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LinksQuery>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let mut link_query = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.is_null());

    // Filter by folder
    if let Some(folder_id) = query.folder_id {
        link_query = link_query.filter(links::Column::FolderId.eq(folder_id));
    }

    // Filter by organization
    if let Some(org_id) = query.org_id {
        link_query = link_query.filter(links::Column::OrgId.eq(org_id));
    }

    // Search by URL or code
    if let Some(search) = query.search {
        link_query = link_query.filter(
            Condition::any()
                .add(links::Column::OriginalUrl.contains(&search))
                .add(links::Column::Code.contains(&search))
                .add(links::Column::Notes.contains(&search))
        );
    }

    // Filter by tag
    if let Some(tag_id) = query.tag_id {
        let link_tag_ids: Vec<i32> = link_tags::Entity::find()
            .filter(link_tags::Column::TagId.eq(tag_id))
            .all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|lt| lt.link_id)
            .collect();

        link_query = link_query.filter(links::Column::Id.is_in(link_tag_ids));
    }

    let link_query = link_query.order_by_desc(links::Column::CreatedAt);

    // Pagination
    let link_query = if let Some(limit) = query.limit {
        link_query.limit(limit)
    } else {
        link_query
    };

    let link_query = if let Some(offset) = query.offset {
        link_query.offset(offset)
    } else {
        link_query
    };

    let user_links = link_query.all(&state.db).await.unwrap_or_default();

    let base_url = get_base_url();
    let mut response = Vec::new();
    for l in user_links {
        let tags = get_link_tags(&state.db, l.id).await;
        response.push(LinkResponse {
            id: l.id,
            code: l.code.clone(),
            short_url: format!("{}/{}", base_url, l.code),
            original_url: l.original_url.clone(),
            click_count: l.click_count,
            created_at: l.created_at.to_string(),
            expires_at: l.expires_at.map(|d| d.to_string()),
            has_password: l.password_hash.is_some(),
            notes: l.notes.clone(),
            folder_id: l.folder_id,
            org_id: l.org_id,
            starts_at: l.starts_at.map(|s| s.to_string()),
            max_clicks: l.max_clicks,
            is_active: l.is_active(),
            tags,
        });
    }

    (StatusCode::OK, Json(response)).into_response()
}

/// Delete a link
#[utoipa::path(
    delete,
    path = "/links/{id}",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "Link deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Links"
)]
pub async fn delete_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if link.user_id != Some(user_id) {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "You don't have permission to delete this link".to_string() })).into_response();
        }

        if link.deleted_at.is_some() {
            return (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response();
        }

        // Soft delete - set deleted_at timestamp instead of actually deleting
        let mut active_link: links::ActiveModel = link.clone().into();
        active_link.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));

        // Invalidate cache
        if let Some(cache) = &state.redis_cache {
            let _ = cache.invalidate_link(&link.code).await;
        }

        match active_link.update(&state.db).await {
            Ok(_) => (StatusCode::OK, Json(SuccessResponse { message: "Link deleted successfully".to_string() })).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to delete link".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
    }
}

/// Update a link
#[utoipa::path(
    put,
    path = "/links/{id}",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    request_body = UpdateLinkRequest,
    responses(
        (status = 200, description = "Link updated", body = LinkResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Links"
)]
pub async fn update_link(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Json(payload): Json<UpdateLinkRequest>,
) -> impl IntoResponse {
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response();
    }

    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let link = links::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(link) = link {
        if link.user_id != Some(user_id) {
            return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: "You don't have permission to update this link".to_string() })).into_response();
        }

        let mut active_link: links::ActiveModel = link.clone().into();

        if let Some(ref url) = payload.original_url {
            // Check if new URL is blocked
            if let Err(e) = check_blocked(&state.db, url).await {
                return (StatusCode::FORBIDDEN, Json(ErrorResponse { error: e })).into_response();
            }
            active_link.original_url = Set(url.clone());
        }

        if payload.remove_expiration == Some(true) {
            active_link.expires_at = Set(None);
        } else if let Some(expires) = payload.expires_at {
            active_link.expires_at = Set(Some(expires.naive_utc()));
        }

        if payload.remove_password == Some(true) {
            active_link.password_hash = Set(None);
        } else if let Some(password) = payload.password {
            match hash(password, DEFAULT_COST) {
                Ok(h) => active_link.password_hash = Set(Some(h)),
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to hash password".to_string() })).into_response(),
            }
        }

        if let Some(notes) = payload.notes {
            active_link.notes = Set(Some(notes));
        }

        if let Some(folder_id) = payload.folder_id {
            active_link.folder_id = Set(Some(folder_id));
        }

        if payload.remove_starts_at == Some(true) {
            active_link.starts_at = Set(None);
        } else if let Some(starts_at) = payload.starts_at {
            active_link.starts_at = Set(Some(starts_at.naive_utc()));
        }

        if payload.remove_max_clicks == Some(true) {
            active_link.max_clicks = Set(None);
        } else if let Some(max_clicks) = payload.max_clicks {
            active_link.max_clicks = Set(Some(max_clicks));
        }

        match active_link.update(&state.db).await {
            Ok(updated) => {
                // Invalidate cache
                if let Some(cache) = &state.redis_cache {
                    let _ = cache.invalidate_link(&updated.code).await;
                }

                let tags = get_link_tags(&state.db, updated.id).await;
                let base_url = get_base_url();
                (StatusCode::OK, Json(LinkResponse {
                    id: updated.id,
                    code: updated.code.clone(),
                    short_url: format!("{}/{}", base_url, updated.code),
                    original_url: updated.original_url.clone(),
                    click_count: updated.click_count,
                    created_at: updated.created_at.to_string(),
                    expires_at: updated.expires_at.map(|d| d.to_string()),
                    has_password: updated.password_hash.is_some(),
                    notes: updated.notes.clone(),
                    folder_id: updated.folder_id,
                    org_id: updated.org_id,
                    starts_at: updated.starts_at.map(|s| s.to_string()),
                    max_clicks: updated.max_clicks,
                    is_active: updated.is_active(),
                    tags,
                })).into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to update link".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Link not found".to_string() })).into_response()
    }
}

/// Bulk create links
#[utoipa::path(
    post,
    path = "/links/bulk",
    request_body = BulkCreateLinkRequest,
    responses(
        (status = 200, description = "Links created", body = BulkCreateLinkResponse),
    ),
    tag = "Links"
)]
pub async fn bulk_create_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkCreateLinkRequest>,
) -> impl IntoResponse {
    let user_id = get_user_id_from_header(&headers);

    // If org_id is provided, verify user is a member
    if let Some(org_id) = payload.org_id {
        if let Some(uid) = user_id {
            use crate::entity::org_members;
            let is_member = org_members::Entity::find()
                .filter(org_members::Column::OrgId.eq(org_id))
                .filter(org_members::Column::UserId.eq(uid))
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .is_some();
            
            if !is_member {
                return (StatusCode::FORBIDDEN, Json(BulkCreateLinkResponse { 
                    links: vec![], 
                    errors: vec!["Not a member of this organization".to_string()] 
                })).into_response();
            }
        } else {
            return (StatusCode::FORBIDDEN, Json(BulkCreateLinkResponse { 
                links: vec![], 
                errors: vec!["Authentication required to create org links".to_string()] 
            })).into_response();
        }
    }

    let mut result_links = Vec::new();
    let mut errors = Vec::new();
    let base_url = get_base_url();

    for url in payload.urls {
        // Validate URL before creating link
        if !is_valid_url(&url) {
            errors.push(format!("Invalid URL: {}", url));
            continue;
        }
        
        // Check if URL or domain is blocked
        if let Err(e) = check_blocked(&state.db, &url).await {
            errors.push(format!("{}: {}", url, e));
            continue;
        }
        
        let code: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();

        let link = links::ActiveModel {
            original_url: Set(url.clone()),
            code: Set(code.clone()),
            user_id: Set(user_id),
            folder_id: Set(payload.folder_id),
            org_id: Set(payload.org_id),
            ..Default::default()
        };

        match links::Entity::insert(link).exec(&state.db).await {
            Ok(link_res) => {
                result_links.push(CreateLinkResponse {
                    id: link_res.last_insert_id,
                    code: code.clone(),
                    short_url: format!("{}/{}", base_url, code),
                });
            }
            Err(e) => {
                errors.push(format!("Failed to shorten {}: {}", url, e));
            }
        }
    }

    (StatusCode::OK, Json(BulkCreateLinkResponse { links: result_links, errors })).into_response()
}

/// Bulk delete links
#[utoipa::path(
    post,
    path = "/links/bulk/delete",
    request_body = BulkDeleteRequest,
    responses(
        (status = 200, description = "Links deleted", body = BulkDeleteResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn bulk_delete_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkDeleteRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let mut deleted = 0u64;

    for id in payload.ids {
        let link = links::Entity::find_by_id(id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link {
            if link.user_id == Some(user_id) && link.deleted_at.is_none() {
                // Soft delete instead of hard delete
                let mut active_link: links::ActiveModel = link.into();
                active_link.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));

                if active_link.update(&state.db).await.is_ok() {
                    deleted += 1;
                }
            }
        }
    }

    (StatusCode::OK, Json(BulkDeleteResponse { deleted })).into_response()
}

/// Bulk update links
#[utoipa::path(
    post,
    path = "/links/bulk/update",
    request_body = BulkUpdateRequest,
    responses(
        (status = 200, description = "Links updated", body = BulkUpdateResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn bulk_update_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkUpdateRequest>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let mut updated = 0u64;

    for id in payload.ids {
        let link = links::Entity::find_by_id(id)
            .one(&state.db)
            .await
            .ok()
            .flatten();

        if let Some(link) = link {
            if link.user_id == Some(user_id) {
                let mut active_link: links::ActiveModel = link.into();

                if let Some(folder_id) = payload.folder_id {
                    active_link.folder_id = Set(Some(folder_id));
                }

                if payload.remove_expiration == Some(true) {
                    active_link.expires_at = Set(None);
                } else if let Some(expires) = payload.expires_at {
                    active_link.expires_at = Set(Some(expires.naive_utc()));
                }

                if active_link.update(&state.db).await.is_ok() {
                    updated += 1;
                }
            }
        }
    }

    (StatusCode::OK, Json(BulkUpdateResponse { updated })).into_response()
}

/// Export links to CSV
#[utoipa::path(
    get,
    path = "/links/export",
    responses(
        (status = 200, description = "CSV file", content_type = "text/csv"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Links"
)]
pub async fn export_links_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let user_links = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .order_by_desc(links::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let base_url = get_base_url();
    let mut csv_content = String::from("ID,Code,Original URL,Short URL,Click Count,Created At,Expires At,Has Password,Notes,Folder ID,Max Clicks,Starts At\n");
    
    for link in user_links {
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            link.id,
            link.code,
            link.original_url.replace(',', "%2C"),
            format!("{}/{}", base_url, link.code),
            link.click_count,
            link.created_at.format("%Y-%m-%d %H:%M:%S"),
            link.expires_at.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default(),
            link.password_hash.is_some(),
            link.notes.as_ref().map(|n| n.replace(',', "%2C")).unwrap_or_default(),
            link.folder_id.map(|f| f.to_string()).unwrap_or_default(),
            link.max_clicks.map(|m| m.to_string()).unwrap_or_default(),
            link.starts_at.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default(),
        ));
    }

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv"),
            (axum::http::header::CONTENT_DISPOSITION, "attachment; filename=\"links.csv\""),
        ],
        csv_content,
   ).into_response()
}
