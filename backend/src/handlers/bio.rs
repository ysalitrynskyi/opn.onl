use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::entity::{links, users};
use crate::AppState;

/// Usernames that would collide with app routes or API paths.
const RESERVED_USERNAMES: &[&str] = &[
    "dashboard", "settings", "analytics", "password", "login", "register", "admin", "docs",
    "api", "faq", "pricing", "about", "privacy", "terms", "contact", "features", "verify-email",
    "reset-password", "forgot-password", "health", "links", "auth", "orgs", "folders", "tags",
    "ws", "sse", "sitemap", "robots", "swagger-ui", "api-docs", "404", "bio",
];

fn link_in_bio_enabled() -> bool {
    std::env::var("ENABLE_LINK_IN_BIO")
        .map(|v| v != "false")
        .unwrap_or(true)
}

#[derive(Deserialize, ToSchema)]
pub struct BioSettingsRequest {
    pub bio_username: Option<String>,
    pub bio_enabled: Option<bool>,
    pub bio_theme: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct BioSettingsResponse {
    pub bio_username: Option<String>,
    pub bio_enabled: bool,
    pub bio_theme: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct BioLink {
    pub code: String,
    pub short_url: String,
    pub label: String,
    pub click_count: i32,
}

#[derive(Serialize, ToSchema)]
pub struct BioProfileResponse {
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
    pub theme: Option<String>,
    pub links: Vec<BioLink>,
}

fn validate_username(name: &str) -> Result<String, String> {
    let n = name.trim().to_lowercase();
    if n.len() < 3 || n.len() > 30 {
        return Err("Username must be 3–30 characters".into());
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("Username may only contain lowercase letters, numbers, hyphens and underscores".into());
    }
    if n.starts_with('-') || n.starts_with('_') || n.ends_with('-') || n.ends_with('_') {
        return Err("Username cannot start or end with a hyphen or underscore".into());
    }
    if RESERVED_USERNAMES.contains(&n.as_str()) {
        return Err("That username is reserved".into());
    }
    Ok(n)
}

/// Update the caller's link-in-bio settings (username, enabled, theme).
pub async fn update_bio_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BioSettingsRequest>,
) -> impl IntoResponse {
    if !link_in_bio_enabled() {
        return (StatusCode::FORBIDDEN, "Link-in-bio is not enabled on this instance").into_response();
    }
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };
    let user = match users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .ok()
        .flatten()
    {
        Some(u) => u,
        None => return (StatusCode::NOT_FOUND, "User not found").into_response(),
    };

    let mut active: users::ActiveModel = user.clone().into();
    let mut eff_username = user.bio_username.clone();
    let mut eff_enabled = user.bio_enabled;

    if let Some(raw) = &payload.bio_username {
        if raw.trim().is_empty() {
            active.bio_username = Set(None);
            eff_username = None;
        } else {
            let username = match validate_username(raw) {
                Ok(u) => u,
                Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
            };
            let taken = users::Entity::find()
                .filter(users::Column::BioUsername.eq(username.clone()))
                .filter(users::Column::Id.ne(user_id))
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .is_some();
            if taken {
                return (StatusCode::CONFLICT, "That username is taken").into_response();
            }
            active.bio_username = Set(Some(username.clone()));
            eff_username = Some(username);
        }
    }
    if let Some(enabled) = payload.bio_enabled {
        active.bio_enabled = Set(enabled);
        eff_enabled = enabled;
    }
    if let Some(theme) = &payload.bio_theme {
        active.bio_theme = Set(if theme.is_empty() { None } else { Some(theme.clone()) });
    }

    // The page can't go live without a username.
    if eff_enabled && eff_username.is_none() {
        return (StatusCode::BAD_REQUEST, "Choose a username before enabling your bio page").into_response();
    }

    match active.update(&state.db).await {
        Ok(updated) => (
            StatusCode::OK,
            Json(BioSettingsResponse {
                bio_username: updated.bio_username,
                bio_enabled: updated.bio_enabled,
                bio_theme: updated.bio_theme,
            }),
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save settings").into_response(),
    }
}

/// Public bio page data. Returns 404 unless the instance flag AND the user's
/// `bio_enabled` are both on (and the user isn't deleted) — 404 rather than 403
/// so a disabled page never reveals whether a username exists.
pub async fn get_public_bio(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    if !link_in_bio_enabled() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let uname = username.trim().to_lowercase();
    let user = match users::Entity::find()
        .filter(users::Column::BioUsername.eq(uname))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .ok()
        .flatten()
    {
        Some(u) if u.bio_enabled => u,
        _ => return (StatusCode::NOT_FOUND, "Not found").into_response(),
    };

    let base_url =
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());

    let link_models = links::Entity::find()
        .filter(links::Column::UserId.eq(user.id))
        .filter(links::Column::DeletedAt.is_null())
        .filter(links::Column::BioVisible.eq(true))
        .order_by_asc(links::Column::BioPosition)
        .order_by_desc(links::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let links_out: Vec<BioLink> = link_models
        .into_iter()
        .filter(|l| l.is_active())
        .map(|l| {
            let label = l
                .bio_label
                .clone()
                .or_else(|| l.title.clone())
                .unwrap_or_else(|| l.code.clone());
            BioLink {
                short_url: format!("{}/{}", base_url, l.code),
                code: l.code,
                label,
                click_count: l.click_count,
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(BioProfileResponse {
            username: user.bio_username.unwrap_or_default(),
            display_name: user.display_name,
            bio: user.bio,
            website: user.website,
            avatar_url: user.avatar_url,
            location: user.location,
            theme: user.bio_theme,
            links: links_out,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::validate_username;

    #[test]
    fn accepts_valid_usernames() {
        assert_eq!(validate_username("Jane_Doe"), Ok("jane_doe".to_string()));
        assert_eq!(validate_username("opn-fan"), Ok("opn-fan".to_string()));
    }

    #[test]
    fn rejects_bad_usernames() {
        assert!(validate_username("ab").is_err()); // too short
        assert!(validate_username("has space").is_err());
        assert!(validate_username("-lead").is_err());
        assert!(validate_username("trail_").is_err());
        assert!(validate_username("dashboard").is_err()); // reserved
        assert!(validate_username("admin").is_err()); // reserved
    }
}
