use utoipa::{OpenApi, ToResponse, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{
    admin, analytics, api_keys, auth, bio, contact, folders, links, organizations, passkeys, tags,
};

/// `info.description` of the served document (Markdown), set in [`api_doc`].
///
/// The rate-limit table mirrors `RateLimiters::default()` and the routing in
/// `rate_limit_middleware`. `tests/audit_openapi.rs` fails if the numbers drift.
const API_DESCRIPTION: &str = "\
A modern, feature-rich URL shortening service with analytics, teams, and real-time updates.

## Authentication

Protected operations take `Authorization: Bearer <token>`. Each operation lists the kinds of \
token it accepts:

- `bearer_auth`: the session JWT returned by `POST /auth/register`, `POST /auth/login`, and \
`POST /auth/passkey/login/finish`. Every protected operation accepts it.
- `api_key`: a personal API key (`opn_...`) created with `POST /auth/api-keys`. Links, folders, \
tags, organizations, analytics, and the profile accept it. Account security (password, account \
deletion, passkeys, API keys) and `/admin` require the session JWT.

Operations without a security requirement are public. `POST /links` also works without a token \
and then creates an anonymous link.

## Rate limits

Limits are counted per client IP address (per /64 for IPv6). A response that passed the limiter \
carries `X-RateLimit-Limit` and `X-RateLimit-Remaining` for the budget it was charged to. Over a \
limit, the answer is `429 Too Many Requests` with `Retry-After` (seconds, rounded up), the same \
two headers, and a `RateLimitResponse` body.

| Budget | Limit | Requests it covers |
|---|---|---|
| Burst | 10 per second | Every request except short-link visits (`/{code}`, `/{code}/preview`, \
`/{code}/verify`), counted in addition to the request's own budget below |
| Sign-in | 10 per minute | Every `POST` under `/auth/` |
| Link creation | 100 per hour | `POST /links` and `POST /links/{id}/clone`. `POST /links/bulk` \
spends one per URL and lists the URLs past the budget in `errors` instead of answering 429 |
| Contact | 10 per hour | `POST /contact` |
| Link password | 5 per minute per link, 20 per minute in total | `POST /{code}/verify`, and \
`GET /{code}` with an `X-Link-Password` header |
| Redirects | 100 per second | `GET /{code}` and `GET /{code}/preview` |
| General | 100 per minute | Every other request, including `GET`, `PUT`, and `DELETE` under \
`/auth/` |
";

// Only the document uses this type: `rate_limit_middleware` builds the body
// with `json!`, and tests/audit_openapi.rs checks that the two agree.
/// Body of a `429 Too Many Requests` answer from the rate limiter.
#[derive(ToSchema)]
pub struct RateLimitResponse {
    /// Always `Too many requests`.
    pub error: String,
    /// Seconds until the budget refills, the same value as `Retry-After`.
    pub retry_after: u64,
    /// Human-readable explanation.
    pub message: String,
}

/// The rate limiter's answer when a budget is spent.
#[derive(ToResponse)]
#[response(
    description = "Rate limit exceeded. Retry after the number of seconds in `Retry-After`. \
                   The budgets are listed under Rate limits in the API description.",
    headers(
        ("Retry-After" = u64, description = "Seconds until the budget refills, rounded up"),
        ("X-RateLimit-Limit" = u32, description = "Size of the budget that ran out"),
        ("X-RateLimit-Remaining" = u32, description = "Requests left in that budget (0)")
    )
)]
pub struct TooManyRequests(pub RateLimitResponse);

#[derive(OpenApi)]
#[openapi(
    info(
        title = "opn.onl URL Shortener API",
        // No `version` or `description` here on purpose — see `api_doc()`.
        license(
            name = "AGPL-3.0-only",
            url = "https://www.gnu.org/licenses/agpl-3.0.html"
        ),
        contact(
            name = "opn.onl Support",
            url = "https://opn.onl",
            email = "support@opn.onl"
        )
    ),
    servers(
        (url = "http://localhost:3000", description = "Local development server"),
        (url = "https://l.opn.onl", description = "Hosted opn.onl API")
    ),
    tags(
        (name = "Authentication", description = "User registration, login, and passkey management"),
        (name = "Links", description = "Create, manage, and redirect shortened URLs"),
        (name = "Analytics", description = "View click statistics and analytics"),
        (name = "Organizations", description = "Team and organization management"),
        (name = "Folders", description = "Organize links into folders"),
        (name = "Tags", description = "Tag and categorize links"),
        (name = "Admin", description = "Instance administration: users, links, organizations, blocking, backups"),
        (name = "Contact", description = "Contact form"),
        (name = "Bio", description = "Public link-in-bio pages"),
    ),
    paths(
        // Authentication
        auth::register,
        auth::login,
        auth::verify_email,
        auth::resend_verification,
        auth::forgot_password,
        auth::reset_password,
        auth::change_password,
        auth::delete_account,
        auth::get_app_settings,
        auth::get_current_user,
        auth::update_profile,

        // API keys (personal access tokens)
        api_keys::create_api_key,
        api_keys::list_api_keys,
        api_keys::delete_api_key,

        // Passkeys (WebAuthn)
        passkeys::register_start,
        passkeys::register_finish,
        passkeys::login_start,
        passkeys::login_finish,
        passkeys::list_passkeys,
        passkeys::delete_passkey,
        passkeys::rename_passkey,

        // Link-in-bio
        bio::update_bio_settings,
        bio::get_public_bio,
        links::proxy_bio_avatar,

        // Links
        links::create_link,
        links::redirect_link,
        links::verify_link_password,
        links::get_qr_code,
        links::get_user_links,
        links::delete_link,
        links::update_link,
        links::bulk_create_links,
        links::bulk_delete_links,
        links::bulk_update_links,
        links::export_links_csv,
        links::clone_link,
        links::toggle_pin,
        links::check_code_availability,
        links::check_url_health,
        links::build_utm_url,
        links::get_sparklines,
        links::get_link_preview_metadata,
        links::get_routing_rules,
        links::replace_routing_rules,
        links::preview_link,

        // Analytics
        analytics::get_link_stats,
        analytics::get_dashboard_stats,
        analytics::get_realtime_clicks,

        // Organizations
        organizations::create_organization,
        organizations::get_user_organizations,
        organizations::get_organization,
        organizations::update_organization,
        organizations::delete_organization,
        organizations::get_organization_members,
        organizations::invite_member,
        organizations::update_member_role,
        organizations::remove_member,
        organizations::transfer_ownership,
        organizations::get_audit_log,

        // Folders
        folders::create_folder,
        folders::get_folders,
        folders::get_folder,
        folders::update_folder,
        folders::delete_folder,
        folders::move_links_to_folder,
        folders::get_folder_links,

        // Tags
        tags::create_tag,
        tags::get_tags,
        tags::get_tag,
        tags::update_tag,
        tags::delete_tag,
        tags::add_tags_to_link,
        tags::remove_tags_from_link,
        tags::get_links_by_tag,

        // Admin
        admin::get_admin_stats,
        admin::get_admin_activity,
        admin::get_all_users,
        admin::delete_user,
        admin::hard_delete_user,
        admin::restore_user,
        admin::enable_user,
        admin::make_admin,
        admin::remove_admin,
        admin::admin_verify_email,
        admin::get_all_links,
        admin::admin_delete_link,
        admin::admin_restore_link,
        admin::admin_bulk_delete_links,
        admin::admin_bulk_restore_links,
        admin::admin_block_domain_from_link,
        admin::get_all_orgs,
        admin::get_blocked_links,
        admin::block_link,
        admin::unblock_link,
        admin::get_blocked_domains,
        admin::block_domain,
        admin::unblock_domain,
        admin::get_blocked_email_domains,
        admin::block_email_domain,
        admin::unblock_email_domain,
        admin::create_backup,
        admin::list_backups,
        admin::cleanup_backups,

        // Contact
        contact::send_contact_message,

        // Deliberately unpublished (not listed above):
        // - GET /health — ops probe, not an API consumer contract
        // - GET /swagger-ui, GET /api-docs/openapi.json — the docs UI itself
        // - GET /ws, GET /sse — live transports; OpenAPI cannot describe the
        //   upgrade/event stream, and the query-token auth is not a REST call
    ),
    components(
        schemas(
            // Auth schemas
            auth::RegisterRequest,
            auth::LoginRequest,
            auth::VerifyEmailRequest,
            auth::ResendVerificationRequest,
            auth::ForgotPasswordRequest,
            auth::ResetPasswordRequest,
            auth::ChangePasswordRequest,
            auth::DeleteAccountRequest,
            auth::AuthResponse,
            auth::MessageResponse,
            auth::AppSettingsResponse,
            auth::UserProfileResponse,
            auth::UpdateProfileRequest,

            // API key schemas
            api_keys::CreateApiKeyRequest,
            api_keys::CreateApiKeyResponse,
            api_keys::ApiKeyInfo,

            // Passkey schemas (WebAuthn ceremony bodies are opaque and not expanded)
            passkeys::PasskeyAuthResponse,
            passkeys::PasskeyInfo,
            passkeys::PasskeyListResponse,
            passkeys::RegisterStartRequest,
            passkeys::RegisterFinishRequest,
            passkeys::LoginStartRequest,
            passkeys::LoginFinishRequest,
            passkeys::DeletePasskeyRequest,
            passkeys::RenamePasskeyRequest,

            // Link-in-bio schemas
            bio::BioSettingsRequest,
            bio::BioSettingsResponse,
            bio::BioLink,
            bio::BioProfileResponse,

            // Link schemas
            links::CreateLinkRequest,
            links::UpdateLinkRequest,
            links::BulkCreateLinkRequest,
            links::BulkDeleteRequest,
            links::BulkUpdateRequest,
            links::LinksQuery,
            links::LinkResponse,
            links::CreateLinkResponse,
            links::BulkCreateLinkResponse,
            links::BulkDeleteResponse,
            links::BulkUpdateResponse,
            links::ErrorResponse,
            links::SuccessResponse,
            links::VerifyPasswordRequest,
            links::TagInfo,
            links::LinkPreviewResponse,
            links::ReputationInfo,
            links::CloneLinkResponse,
            links::PinResponse,
            links::CheckCodeResponse,
            links::HealthCheckRequest,
            links::UrlHealthResponse,
            links::BuildUtmRequest,
            links::BuildUtmResponse,
            links::SparklineData,
            links::SparklineResponse,
            links::PreviewMetadataRequest,
            links::LinkPreviewData,
            links::RoutingRuleInput,
            links::ReplaceRoutingRulesRequest,
            links::RoutingRuleResponse,
            links::RoutingRulesSavedResponse,
            links::AvatarProxyQuery,

            // Analytics schemas
            analytics::AnalyticsQuery,
            analytics::LinkStatsResponse,
            analytics::DashboardStats,
            analytics::DayStats,
            analytics::CountryStats,
            analytics::CityStats,
            analytics::DeviceStats,
            analytics::BrowserStats,
            analytics::OsStats,
            analytics::RefererStats,
            analytics::RecentClick,
            analytics::GeoPoint,
            analytics::TopLink,

            // Organization schemas
            organizations::CreateOrgRequest,
            organizations::UpdateOrgRequest,
            organizations::InviteMemberRequest,
            organizations::UpdateMemberRoleRequest,
            organizations::TransferOwnershipRequest,
            organizations::OrgResponse,
            organizations::OrgMemberResponse,
            organizations::AuditLogResponse,
            organizations::AuditQuery,

            // Folder schemas
            folders::CreateFolderRequest,
            folders::UpdateFolderRequest,
            folders::FolderQuery,
            folders::FolderResponse,
            folders::MoveLinkToFolderRequest,

            // Tag schemas
            tags::CreateTagRequest,
            tags::UpdateTagRequest,
            tags::TagQuery,
            tags::TagResponse,
            tags::AddTagsToLinkRequest,
            tags::RemoveTagsFromLinkRequest,

            // Contact schemas
            contact::ContactRequest,
            contact::ContactResponse,

            // Admin schemas
            admin::AdminResponse,
            admin::AdminStatsResponse,
            admin::AdminUserResponse,
            admin::AdminUsersListResponse,
            admin::AdminLinkResponse,
            admin::AdminLinksListResponse,
            admin::BulkLinkIdsRequest,
            admin::BulkLinkActionResponse,
            admin::BlockFromLinkResponse,
            admin::AdminOrgResponse,
            admin::AdminOrgsListResponse,
            admin::ActivityDay,
            admin::AdminActivityResponse,
            admin::BlockLinkRequest,
            admin::BlockDomainRequest,
            admin::BlockEmailDomainRequest,
            admin::BlockedLinkResponse,
            admin::BlockedDomainResponse,
            admin::BlockedEmailDomainResponse,
            admin::BackupResponse,
            admin::BackupListResponse,

            // Rate limiting
            RateLimitResponse,
        ),
        responses(TooManyRequests)
    ),
    modifiers(&SecurityAddon, &RateLimitAddon)
)]
pub struct ApiDoc;

/// Handlers authenticate themselves by reading `Authorization: Bearer ...`.
/// Operations name the schemes they accept in `security(...)`: `bearer_auth`
/// alone where the handler insists on a session JWT, `("bearer_auth" = []),
/// ("api_key" = [])` where an `opn_` API key works too, and a leading `()`
/// where credentials are optional.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some(
                            "Session JWT from `POST /auth/register`, `POST /auth/login`, or \
                             `POST /auth/passkey/login/finish`.",
                        ))
                        .build(),
                ),
            );
            components.add_security_scheme(
                "api_key",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("opn_ API key")
                        .description(Some(
                            "Personal API key from `POST /auth/api-keys`, sent as \
                             `Authorization: Bearer opn_...`.",
                        ))
                        .build(),
                ),
            );
        }
    }
}

/// Every route sits behind `rate_limit_middleware`, so any operation can be
/// answered 429. Add the shared response to each operation that does not
/// describe a 429 of its own.
struct RateLimitAddon;

impl utoipa::Modify for RateLimitAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::{Ref, RefOr};

        let (name, _) = TooManyRequests::response();
        for item in openapi.paths.paths.values_mut() {
            for operation in item.operations.values_mut() {
                operation
                    .responses
                    .responses
                    .entry("429".to_string())
                    .or_insert_with(|| RefOr::Ref(Ref::from_response_name(name)));
            }
        }
    }
}

/// The OpenAPI document served at `/api-docs/openapi.json`.
///
/// The version is stamped here rather than in the `#[openapi(info(...))]`
/// attribute because utoipa only accepts a string literal there, and the literal
/// that used to live in it went stale: after the 1.3.0 release the published spec
/// still advertised 1.2.1. Reading `CARGO_PKG_VERSION` keeps the served document
/// in step with Cargo.toml on its own. The description is set here too, because
/// the attribute cannot hold a Markdown document legibly.
pub fn api_doc() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.info.version = env!("CARGO_PKG_VERSION").to_string();
    doc.info.description = Some(API_DESCRIPTION.to_string());
    doc
}

/// Create Swagger UI routes
pub fn swagger_routes() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api_doc())
}
