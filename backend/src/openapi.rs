use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{
    auth, links, analytics, organizations, folders, tags,
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "opn.onl URL Shortener API",
        version = "1.0.0",
        description = "A modern, feature-rich URL shortening service with analytics, teams, and real-time updates.",
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
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
    ),
    paths(
        // Authentication
        auth::register,
        auth::login,
        
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
    ),
    components(
        schemas(
            // Auth schemas
            auth::RegisterRequest,
            auth::LoginRequest,
            auth::AuthResponse,
            
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
                        utoipa::openapi::security::HttpAuthScheme::Bearer
                    )
                )
            );
        }
    }
}

/// Create Swagger UI routes
pub fn swagger_routes() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}

