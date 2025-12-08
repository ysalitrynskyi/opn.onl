use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sea_orm::*;
use validator::Validate;
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::users;
use crate::utils::jwt::{hash_password, verify_password, create_jwt};
use axum::http::HeaderMap;
use crate::utils::email::generate_token;

#[derive(Deserialize, Validate, ToSchema)]
pub struct RegisterRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Deserialize, Validate, ToSchema)]
pub struct ResetPasswordRequest {
    pub token: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ResendVerificationRequest {
    pub email: String,
}

#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: i32,
    pub email: String,
    pub email_verified: bool,
    pub is_admin: bool,
}

#[derive(Serialize, ToSchema)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// Register a new user
#[utoipa::path(
    post,
    path = "/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered successfully", body = AuthResponse),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Email already exists"),
    ),
    tag = "Authentication"
)]
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> impl IntoResponse {
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response();
    }

    let hashed_password = match hash_password(&payload.password) {
        Ok(h) => h,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Password hashing failed".to_string() })).into_response(),
    };

    // Generate verification token
    let verification_token = generate_token();
    let verification_expires = Utc::now() + Duration::hours(24);

    // Check if this is the first user - make them admin
    let user_count = users::Entity::find()
        .count(&state.db)
        .await
        .unwrap_or(0);
    let is_first_user = user_count == 0;

    let new_user = users::ActiveModel {
        email: Set(payload.email.clone()),
        password_hash: Set(hashed_password),
        email_verified: Set(false),
        verification_token: Set(Some(verification_token.clone())),
        verification_token_expires: Set(Some(verification_expires.naive_utc())),
        is_admin: Set(is_first_user), // First user is automatically admin
        ..Default::default()
    };

    let result = users::Entity::insert(new_user).exec(&state.db).await;

    match result {
        Ok(user_res) => {
            // Send verification email if email service is configured
            if let Some(email_service) = &state.email_service {
                if email_service.is_configured() {
                    if let Err(e) = email_service.send_verification_email(&payload.email, &verification_token).await {
                        tracing::error!("Failed to send verification email: {}", e);
                    }
                }
            }

            let token = match create_jwt(user_res.last_insert_id, &payload.email) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("Failed to create JWT: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to create session".to_string() })).into_response();
                }
            };
            (StatusCode::CREATED, Json(AuthResponse { 
                token,
                user_id: user_res.last_insert_id,
                email: payload.email,
                email_verified: false,
                is_admin: is_first_user,
            })).into_response()
        }
        Err(DbErr::Query(err)) => {
             if err.to_string().contains("duplicate key value") {
                 (StatusCode::CONFLICT, Json(ErrorResponse { error: "Email already exists".to_string() })).into_response()
             } else {
                 (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response()
             }
        }
        Err(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() })).into_response()
        }
    }
}

/// Login with email and password
#[utoipa::path(
    post,
    path = "/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthResponse),
        (status = 401, description = "Invalid credentials"),
    ),
    tag = "Authentication"
)]
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.email))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        if verify_password(&payload.password, &user.password_hash).unwrap_or(false) {
            let token = match create_jwt(user.id, &user.email) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("Failed to create JWT: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to create session".to_string() })).into_response();
                }
            };
            return (StatusCode::OK, Json(AuthResponse { 
                token,
                user_id: user.id,
                email: user.email,
                email_verified: user.email_verified,
                is_admin: user.is_admin,
            })).into_response();
        }
    }

    (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Invalid credentials".to_string() })).into_response()
}

/// Verify email with token
#[utoipa::path(
    post,
    path = "/auth/verify-email",
    request_body = VerifyEmailRequest,
    responses(
        (status = 200, description = "Email verified successfully", body = MessageResponse),
        (status = 400, description = "Invalid or expired token"),
    ),
    tag = "Authentication"
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Json(payload): Json<VerifyEmailRequest>,
) -> impl IntoResponse {
    let user = users::Entity::find()
        .filter(users::Column::VerificationToken.eq(&payload.token))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        // Check if token is expired
        if let Some(expires) = user.verification_token_expires {
            if Utc::now().naive_utc() > expires {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Token expired".to_string() })).into_response();
            }
        }

        // Update user as verified
        let mut active_user: users::ActiveModel = user.clone().into();
        active_user.email_verified = Set(true);
        active_user.verification_token = Set(None);
        active_user.verification_token_expires = Set(None);

        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to verify email".to_string() })).into_response();
        }

        // Send welcome email
        if let Some(email_service) = &state.email_service {
            if email_service.is_configured() {
                if let Err(e) = email_service.send_welcome_email(&user.email).await {
                    tracing::error!("Failed to send welcome email: {}", e);
                }
            }
        }

        return (StatusCode::OK, Json(MessageResponse { message: "Email verified successfully".to_string() })).into_response();
    }

    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid token".to_string() })).into_response()
}

/// Resend verification email
#[utoipa::path(
    post,
    path = "/auth/resend-verification",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "Verification email sent", body = MessageResponse),
        (status = 400, description = "Email already verified or not found"),
    ),
    tag = "Authentication"
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> impl IntoResponse {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.email))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        if user.email_verified {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Email already verified".to_string() })).into_response();
        }

        // Generate new token
        let verification_token = generate_token();
        let verification_expires = Utc::now() + Duration::hours(24);

        let mut active_user: users::ActiveModel = user.clone().into();
        active_user.verification_token = Set(Some(verification_token.clone()));
        active_user.verification_token_expires = Set(Some(verification_expires.naive_utc()));

        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to generate token".to_string() })).into_response();
        }

        // Send verification email
        if let Some(email_service) = &state.email_service {
            if email_service.is_configured() {
                if let Err(e) = email_service.send_verification_email(&user.email, &verification_token).await {
                    tracing::error!("Failed to send verification email: {}", e);
                }
            }
        }

        return (StatusCode::OK, Json(MessageResponse { message: "Verification email sent".to_string() })).into_response();
    }

    // Don't reveal if email exists
    (StatusCode::OK, Json(MessageResponse { message: "If account exists, verification email sent".to_string() })).into_response()
}

/// Request password reset
#[utoipa::path(
    post,
    path = "/auth/forgot-password",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Reset email sent if account exists", body = MessageResponse),
    ),
    tag = "Authentication"
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> impl IntoResponse {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.email))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        let reset_token = generate_token();
        let reset_expires = Utc::now() + Duration::hours(1);

        let mut active_user: users::ActiveModel = user.clone().into();
        active_user.password_reset_token = Set(Some(reset_token.clone()));
        active_user.password_reset_expires = Set(Some(reset_expires.naive_utc()));

        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to generate token".to_string() })).into_response();
        }

        // Send password reset email
        if let Some(email_service) = &state.email_service {
            if email_service.is_configured() {
                if let Err(e) = email_service.send_password_reset_email(&user.email, &reset_token).await {
                    tracing::error!("Failed to send password reset email: {}", e);
                }
            }
        }
    }

    // Always return success to prevent email enumeration
    (StatusCode::OK, Json(MessageResponse { message: "If account exists, password reset email sent".to_string() })).into_response()
}

/// Reset password with token
#[utoipa::path(
    post,
    path = "/auth/reset-password",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset successfully", body = MessageResponse),
        (status = 400, description = "Invalid or expired token"),
    ),
    tag = "Authentication"
)]
pub async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> impl IntoResponse {
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response();
    }

    let user = users::Entity::find()
        .filter(users::Column::PasswordResetToken.eq(&payload.token))
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        // Check if token is expired
        if let Some(expires) = user.password_reset_expires {
            if Utc::now().naive_utc() > expires {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Token expired".to_string() })).into_response();
            }
        }

        let hashed_password = match hash_password(&payload.password) {
            Ok(h) => h,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Password hashing failed".to_string() })).into_response(),
        };

        let mut active_user: users::ActiveModel = user.into();
        active_user.password_hash = Set(hashed_password);
        active_user.password_reset_token = Set(None);
        active_user.password_reset_expires = Set(None);

        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to reset password".to_string() })).into_response();
        }

        return (StatusCode::OK, Json(MessageResponse { message: "Password reset successfully".to_string() })).into_response();
    }

    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid token".to_string() })).into_response()
}

#[derive(Deserialize, Validate, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

/// Change password for authenticated user
#[utoipa::path(
    post,
    path = "/auth/change-password",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed successfully", body = MessageResponse),
        (status = 400, description = "Invalid request or wrong current password"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Authentication",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> impl IntoResponse {
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response();
    }

    let user_id = match crate::handlers::links::get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let user = users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        // Verify current password
        if user.password_hash.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "No password set for this account".to_string() })).into_response();
        }
        
        match verify_password(&payload.current_password, &user.password_hash) {
            Ok(true) => {},
            Ok(false) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Current password is incorrect".to_string() })).into_response();
            }
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Password verification failed".to_string() })).into_response();
            }
        }

        // Hash new password
        let hashed_password = match hash_password(&payload.new_password) {
            Ok(h) => h,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Password hashing failed".to_string() })).into_response(),
        };

        // Update password
        let mut active_user: users::ActiveModel = user.into();
        active_user.password_hash = Set(hashed_password);

        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to change password".to_string() })).into_response();
        }

        return (StatusCode::OK, Json(MessageResponse { message: "Password changed successfully".to_string() })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "User not found".to_string() })).into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteAccountRequest {
    pub password: String,
}

/// Delete own account (self-service, if enabled)
#[utoipa::path(
    post,
    path = "/auth/delete-account",
    request_body = DeleteAccountRequest,
    responses(
        (status = 200, description = "Account deleted successfully", body = MessageResponse),
        (status = 400, description = "Invalid request or wrong password"),
        (status = 403, description = "Account deletion is disabled"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Authentication",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DeleteAccountRequest>,
) -> impl IntoResponse {
    // Check if account deletion is enabled
    let deletion_enabled = std::env::var("ENABLE_ACCOUNT_DELETION")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    if !deletion_enabled {
        return (StatusCode::FORBIDDEN, Json(ErrorResponse { 
            error: "Account deletion is disabled. Contact support if you need to delete your account.".to_string() 
        })).into_response();
    }

    let user_id = match crate::handlers::links::get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let user = users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        // Verify password
        if user.password_hash.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "No password set for this account".to_string() })).into_response();
        }
        
        match verify_password(&payload.password, &user.password_hash) {
            Ok(true) => {},
            Ok(false) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Password is incorrect".to_string() })).into_response();
            }
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Password verification failed".to_string() })).into_response();
            }
        }

        // Soft delete user
        let mut active_user: users::ActiveModel = user.into();
        active_user.deleted_at = Set(Some(Utc::now().naive_utc()));

        if let Err(_) = active_user.update(&state.db).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to delete account".to_string() })).into_response();
        }

        // Soft delete all user's links
        use sea_orm::sea_query::Expr;
        use crate::entity::links;
        links::Entity::update_many()
            .col_expr(links::Column::DeletedAt, Expr::value(Utc::now().naive_utc()))
            .filter(links::Column::UserId.eq(user_id))
            .filter(links::Column::DeletedAt.is_null())
            .exec(&state.db)
            .await
            .ok();

        return (StatusCode::OK, Json(MessageResponse { message: "Account deleted successfully".to_string() })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "User not found".to_string() })).into_response()
}

#[derive(Serialize, ToSchema)]
pub struct AppSettingsResponse {
    pub account_deletion_enabled: bool,
    pub custom_aliases_enabled: bool,
    pub max_links_per_user: Option<i32>,
    pub passkeys_enabled: bool,
    pub min_alias_length: usize,
    pub max_alias_length: usize,
    pub url_sanitization_enabled: bool,
}

/// Get app settings
#[utoipa::path(
    get,
    path = "/auth/settings",
    responses(
        (status = 200, description = "App settings", body = AppSettingsResponse),
    ),
    tag = "Authentication"
)]
pub async fn get_app_settings() -> impl IntoResponse {
    let account_deletion_enabled = std::env::var("ENABLE_ACCOUNT_DELETION")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);
    
    let custom_aliases_enabled = std::env::var("ENABLE_CUSTOM_ALIASES")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);
    
    let max_links_per_user = std::env::var("MAX_LINKS_PER_USER")
        .ok()
        .and_then(|v| v.parse().ok());
    
    let min_alias_length = std::env::var("MIN_ALIAS_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    
    let max_alias_length = std::env::var("MAX_ALIAS_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    
    let url_sanitization_enabled = std::env::var("ENABLE_URL_SANITIZATION")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);

    (StatusCode::OK, Json(AppSettingsResponse {
        account_deletion_enabled,
        custom_aliases_enabled,
        max_links_per_user,
        passkeys_enabled: true,
        min_alias_length,
        max_alias_length,
        url_sanitization_enabled,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct UserProfileResponse {
    pub id: i32,
    pub email: String,
    pub email_verified: bool,
    pub is_admin: bool,
    pub created_at: String,
    pub link_count: i64,
    pub total_clicks: i64,
    // Profile fields
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
}

/// Get current user profile
#[utoipa::path(
    get,
    path = "/auth/me",
    responses(
        (status = 200, description = "User profile", body = UserProfileResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn get_current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let user = users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        use crate::entity::{links, click_events};
        
        let link_count = links::Entity::find()
            .filter(links::Column::UserId.eq(user_id))
            .filter(links::Column::DeletedAt.is_null())
            .count(&state.db)
            .await
            .unwrap_or(0) as i64;
        
        let total_clicks = click_events::Entity::find()
            .inner_join(links::Entity)
            .filter(links::Column::UserId.eq(user_id))
            .count(&state.db)
            .await
            .unwrap_or(0) as i64;

        return (StatusCode::OK, Json(UserProfileResponse {
            id: user.id,
            email: user.email,
            email_verified: user.email_verified,
            is_admin: user.is_admin,
            created_at: user.created_at.to_string(),
            link_count,
            total_clicks,
            display_name: user.display_name,
            bio: user.bio,
            website: user.website,
            avatar_url: user.avatar_url,
            location: user.location,
        })).into_response();
    }

    (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "User not found".to_string() })).into_response()
}

/// Update user profile
#[utoipa::path(
    put,
    path = "/auth/profile",
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Profile updated", body = UserProfileResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Authentication",
    security(("bearer_auth" = []))
)]
pub async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateProfileRequest>,
) -> impl IntoResponse {
    let user_id = match crate::handlers::links::get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Unauthorized".to_string() })).into_response(),
    };

    let user = users::Entity::find_by_id(user_id)
        .filter(users::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(user) = user {
        let mut active_user: users::ActiveModel = user.clone().into();
        
        if let Some(name) = payload.display_name {
            active_user.display_name = Set(Some(name));
        }
        if let Some(bio) = payload.bio {
            // Validate bio length (max 500 chars)
            if bio.len() > 500 {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Bio must be 500 characters or less".to_string() })).into_response();
            }
            active_user.bio = Set(Some(bio));
        }
        if let Some(website) = payload.website {
            // Validate website URL
            if !website.is_empty() {
                if let Err(_) = url::Url::parse(&website) {
                    return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid website URL".to_string() })).into_response();
                }
            }
            active_user.website = Set(if website.is_empty() { None } else { Some(website) });
        }
        if let Some(avatar) = payload.avatar_url {
            active_user.avatar_url = Set(if avatar.is_empty() { None } else { Some(avatar) });
        }
        if let Some(location) = payload.location {
            active_user.location = Set(if location.is_empty() { None } else { Some(location) });
        }
        
        match active_user.update(&state.db).await {
            Ok(updated) => {
                use crate::entity::{links, click_events};
                
                let link_count = links::Entity::find()
                    .filter(links::Column::UserId.eq(user_id))
                    .filter(links::Column::DeletedAt.is_null())
                    .count(&state.db)
                    .await
                    .unwrap_or(0) as i64;
                
                let total_clicks = click_events::Entity::find()
                    .inner_join(links::Entity)
                    .filter(links::Column::UserId.eq(user_id))
                    .count(&state.db)
                    .await
                    .unwrap_or(0) as i64;
                
                (StatusCode::OK, Json(UserProfileResponse {
                    id: updated.id,
                    email: updated.email,
                    email_verified: updated.email_verified,
                    is_admin: updated.is_admin,
                    created_at: updated.created_at.to_string(),
                    link_count,
                    total_clicks,
                    display_name: updated.display_name,
                    bio: updated.bio,
                    website: updated.website,
                    avatar_url: updated.avatar_url,
                    location: updated.location,
                })).into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to update profile".to_string() })).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "User not found".to_string() })).into_response()
    }
}
