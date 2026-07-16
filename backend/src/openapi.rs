use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{
    admin, analytics, api_keys, auth, bio, contact, folders, links, organizations, passkeys, tags,
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "opn.onl URL Shortener API",
        version = "1.2.1",
        description = "A modern, feature-rich URL shortening service with analytics, teams, and real-time updates.",
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
        (url = "https://api.opn.onl", description = "Production server")
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
    ),
    components(
        schemas(
            // Auth schemas
            auth::RegisterRequest,
            auth::LoginRequest,
            auth::AuthResponse,
            auth::MessageResponse,

            // API key schemas
            api_keys::CreateApiKeyRequest,
            api_keys::CreateApiKeyResponse,
            api_keys::ApiKeyInfo,

            // Passkey schemas (WebAuthn ceremony bodies are opaque and not expanded)
            passkeys::PasskeyAuthResponse,
            passkeys::PasskeyInfo,
            passkeys::PasskeyListResponse,

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
        )
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}

/// Create Swagger UI routes
pub fn swagger_routes() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}
