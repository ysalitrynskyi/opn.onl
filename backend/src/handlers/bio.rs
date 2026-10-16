use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sea_orm::sea_query::Expr;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{links, users};
use crate::handlers::json_error;
use crate::handlers::link_access::{LinkAction, live_link_for};
use crate::utils::env::env_flag;
use crate::utils::validation::MAX_IDS_PER_REQUEST;

/// Themes the public page renders. Anything else is rejected; an empty string
/// clears the stored theme and the page uses its default look. Keep this in
/// step with `BIO_THEMES` in `frontend/src/pages/bioThemes.ts`.
pub const BIO_THEMES: &[&str] = &["ink", "cobalt"];

/// A bio label is the text of a button, not a description.
pub const MAX_BIO_LABEL_CHARS: usize = 80;

/// Usernames that would collide with app routes or API paths.
const RESERVED_USERNAMES: &[&str] = &[
    "dashboard",
    "settings",
    "analytics",
    "password",
    "login",
    "register",
    "admin",
    "docs",
    "api",
    "faq",
    "pricing",
    "about",
    "privacy",
    "terms",
    "contact",
    "features",
    "verify-email",
    "reset-password",
    "forgot-password",
    "health",
    "links",
    "auth",
    "orgs",
    "folders",
    "tags",
    "ws",
    "sse",
    "sitemap",
    "robots",
    "swagger-ui",
    "api-docs",
    "404",
    "bio",
    "avatar",
];

pub(crate) fn link_in_bio_enabled() -> bool {
    env_flag("ENABLE_LINK_IN_BIO", true)
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
        return Err(
            "Username may only contain lowercase letters, numbers, hyphens and underscores".into(),
        );
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
#[utoipa::path(
    put,
    path = "/auth/bio",
    request_body = BioSettingsRequest,
    responses(
        (status = 200, description = "Updated bio settings", body = BioSettingsResponse),
        (status = 400, description = "Invalid username or bio enabled without a username"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Link-in-bio is not enabled on this instance"),
        (status = 404, description = "User not found"),
        (status = 409, description = "Username is taken"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn update_bio_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BioSettingsRequest>,
) -> impl IntoResponse {
    if !link_in_bio_enabled() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Link-in-bio is not enabled on this instance",
        );
    }
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    let user = match users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .ok()
        .flatten()
    {
        Some(u) => u,
        None => return json_error(StatusCode::NOT_FOUND, "User not found"),
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
                Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
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
                return json_error(StatusCode::CONFLICT, "That username is taken");
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
        if theme.is_empty() {
            active.bio_theme = Set(None);
        } else if !BIO_THEMES.contains(&theme.as_str()) {
            return json_error(StatusCode::BAD_REQUEST, "Theme must be ink or cobalt");
        } else {
            active.bio_theme = Set(Some(theme.clone()));
        }
    }

    // The page can't go live without a username.
    if eff_enabled && eff_username.is_none() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Choose a username before enabling your bio page",
        );
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
        // The read-then-write uniqueness check races; idx-users-bio_username
        // is the real guard. Map that unique violation to the documented 409.
        Err(err) if err.to_string().contains("duplicate key value") => {
            json_error(StatusCode::CONFLICT, "That username is taken")
        }
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save settings"),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct BioOrderRequest {
    /// Personal bio-visible link ids, first to last. Must be exactly the
    /// caller's set: every one of them, each once.
    pub link_ids: Vec<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct BioOrderResponse {
    pub link_ids: Vec<i32>,
}

/// Replace the order of the caller's personal links on their bio page.
///
/// One request, because writing `bio_position` link by link would leave a
/// half-applied order when a later write failed. Organization links stay out:
/// the page is the caller's, and an organization link's place is not theirs
/// to arrange from here.
#[utoipa::path(
    put,
    path = "/auth/bio/order",
    request_body = BioOrderRequest,
    responses(
        (status = 200, description = "Bio link order saved", body = BioOrderResponse),
        (status = 400, description = "The list is not exactly the caller's personal bio-visible links"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Link-in-bio is off, or a link id is not one the caller can edit"),
        (status = 404, description = "User not found"),
        (status = 409, description = "The set of bio links changed while saving"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn update_bio_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BioOrderRequest>,
) -> impl IntoResponse {
    if !link_in_bio_enabled() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Link-in-bio is not enabled on this instance",
        );
    }
    let user_id = match crate::handlers::links::get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => return json_error(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    if users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return json_error(StatusCode::NOT_FOUND, "User not found");
    }
    if payload.link_ids.len() > MAX_IDS_PER_REQUEST {
        return json_error(
            StatusCode::BAD_REQUEST,
            format!("Too many items in one request (max {MAX_IDS_PER_REQUEST})"),
        );
    }
    let mut seen = HashSet::with_capacity(payload.link_ids.len());
    if !payload.link_ids.iter().all(|id| seen.insert(*id)) {
        return json_error(StatusCode::BAD_REQUEST, "Each link can appear only once");
    }

    let expected: HashSet<i32> = match links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::OrgId.is_null())
        .filter(links::Column::DeletedAt.is_null())
        .filter(links::Column::BioVisible.eq(true))
        .all(&state.db)
        .await
    {
        Ok(rows) => rows.into_iter().map(|link| link.id).collect(),
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
    };

    for id in &payload.link_ids {
        if expected.contains(id) {
            continue;
        }
        // Missing, deleted and not-editable ids are the same 403 as every
        // other link-by-id write, so the answer does not reveal which exist.
        let link = match live_link_for(&state.db, *id, user_id, LinkAction::Write).await {
            Ok(link) => link,
            Err(denied) => return denied.into_response(),
        };
        if link.org_id.is_some() {
            return json_error(
                StatusCode::BAD_REQUEST,
                "Only personal links can be ordered on your bio page",
            );
        }
        if !link.bio_visible {
            return json_error(
                StatusCode::BAD_REQUEST,
                "Only links shown on your bio page can be ordered",
            );
        }
        return json_error(
            StatusCode::BAD_REQUEST,
            "List every personal link on your bio page",
        );
    }
    if payload.link_ids.len() != expected.len() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "List every personal link on your bio page",
        );
    }

    if !payload.link_ids.is_empty() {
        let txn = match state.db.begin().await {
            Ok(txn) => txn,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
        };
        for (index, id) in payload.link_ids.iter().enumerate() {
            let updated = links::Entity::update_many()
                .col_expr(links::Column::BioPosition, Expr::value(index as i32))
                .filter(links::Column::Id.eq(*id))
                .filter(links::Column::UserId.eq(user_id))
                .filter(links::Column::OrgId.is_null())
                .filter(links::Column::DeletedAt.is_null())
                .filter(links::Column::BioVisible.eq(true))
                .exec(&txn)
                .await;
            match updated {
                Ok(result) if result.rows_affected == 1 => {}
                Ok(_) => {
                    let _ = txn.rollback().await;
                    return json_error(
                        StatusCode::CONFLICT,
                        "Your bio links changed. Reload and try again.",
                    );
                }
                Err(_) => {
                    let _ = txn.rollback().await;
                    return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save order");
                }
            }
        }
        if txn.commit().await.is_err() {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save order");
        }
    }

    (
        StatusCode::OK,
        Json(BioOrderResponse {
            link_ids: payload.link_ids,
        }),
    )
        .into_response()
}

/// Public bio page data. Returns 404 unless the instance flag AND the user's
/// `bio_enabled` are both on (and the user is neither deleted nor disabled by an
/// admin) — 404 rather than 403 so a disabled page never reveals whether a
/// username exists.
#[utoipa::path(
    get,
    path = "/api/bio/{username}",
    params(("username" = String, Path, description = "Public bio username")),
    responses(
        (status = 200, description = "Public bio profile and visible active links (click counts are omitted)", body = BioProfileResponse),
        (status = 404, description = "No public bio for this username"),
    ),
    tag = "Bio"
)]
pub async fn get_public_bio(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    if !link_in_bio_enabled() {
        return json_error(StatusCode::NOT_FOUND, "Not found");
    }
    let uname = username.trim().to_lowercase();
    let user = match users::Entity::find()
        .filter(users::Column::BioUsername.eq(uname))
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(&state.db)
        .await
        .ok()
        .flatten()
    {
        Some(u) if u.bio_enabled => u,
        _ => return json_error(StatusCode::NOT_FOUND, "Not found"),
    };

    let base_url =
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());

    // Organization links stay on the page only while their creator still
    // belongs to the organization.
    // Position ascending (nulls last), then newest, then highest id. The id
    // matches Settings: two links from one instant, or a fractional second a
    // string sort would swap, stay in this order on the public page too.
    let link_models = links::Entity::find()
        .filter(crate::handlers::link_access::own_links(user.id))
        .filter(links::Column::DeletedAt.is_null())
        .filter(links::Column::BioVisible.eq(true))
        .order_by_asc(links::Column::BioPosition)
        .order_by_desc(links::Column::CreatedAt)
        .order_by_desc(links::Column::Id)
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
        assert!(validate_username("avatar").is_err()); // `/api/bio/avatar`
    }
}
