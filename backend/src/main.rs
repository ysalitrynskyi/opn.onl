mod entity;
mod handlers;
mod utils;
mod openapi;

use axum::{
    routing::{get, post, put, delete},
    Router,
    middleware,
    http::Request,
    body::Body,
    response::{IntoResponse, Redirect},
};
use sea_orm::{Database, DatabaseConnection};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use handlers::websocket::WsState;
use utils::rate_limiter::{RateLimiters, rate_limit_middleware};
use utils::cache::RedisCache;
use utils::{EmailService, ClickBuffer, BackupService};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub ws_state: Option<Arc<WsState>>,
    pub redis_cache: Option<Arc<RedisCache>>,
    pub email_service: Option<Arc<EmailService>>,
    pub click_buffer: Arc<ClickBuffer>,
    pub backup: Arc<BackupService>,
}

/// Middleware to redirect HTTP to HTTPS in production
async fn https_redirect(
    req: Request<Body>,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    let force_https = std::env::var("FORCE_HTTPS")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    
    if !force_https {
        return next.run(req).await;
    }

    // Check X-Forwarded-Proto header (set by reverse proxy)
    let is_https = req
        .headers()
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .map(|proto| proto == "https")
        .unwrap_or(false);

    if is_https {
        next.run(req).await
    } else {
        // Get host from headers
        let host = req
            .headers()
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("localhost");
        
        let uri = req.uri();
        let redirect_url = format!("https://{}{}", host, uri);
        
        Redirect::permanent(&redirect_url).into_response()
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize structured logging
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "logs".to_string());
    let file_appender = tracing_appender::rolling::daily(&log_dir, "opn-onl.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    // Database connection
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/opn_onl".to_string());
    
    let db = Database::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    // Run migrations
    use migration::{Migrator, MigratorTrait};
    Migrator::up(&db, None).await.expect("Failed to run migrations");
    tracing::info!("Migrations completed");

    // Initialize WebSocket state
    let ws_state = Arc::new(WsState::new());

    // Initialize rate limiters
    let rate_limiters = Arc::new(RateLimiters::new());
    RateLimiters::spawn_cleanup_task(rate_limiters.clone());

    // Initialize Redis cache (optional)
    let redis_cache = RedisCache::new().await.map(Arc::new);
    if redis_cache.is_some() {
        tracing::info!("Redis cache enabled");
    } else {
        tracing::info!("Redis cache disabled (REDIS_URL not set or connection failed)");
    }

    // Initialize email service
    let email_service = {
        let service = EmailService::new();
        if service.is_configured() {
            tracing::info!("Email service enabled");
            Some(Arc::new(service))
        } else {
            tracing::info!("Email service disabled (SMTP not configured)");
            None
        }
    };

    // Initialize click buffer for batching
    let click_buffer = Arc::new(ClickBuffer::new());
    click_buffer.clone().start_flush_task(db.clone());
    tracing::info!("Click buffer initialized");

    // Initialize backup service
    let backup = Arc::new(BackupService::new().await);
    if backup.is_configured() {
        tracing::info!("Backup service enabled");
    } else {
        tracing::info!("Backup service disabled (S3/R2 not configured)");
    }

    let app_state = AppState {
        db,
        ws_state: Some(ws_state.clone()),
        redis_cache,
        email_service,
        click_buffer,
        backup,
    };

    // Build router
    let app = Router::new()
        // API Documentation
        .merge(openapi::swagger_routes())
        .route("/api-docs/openapi.json", get(|| async { 
            use utoipa::OpenApi;
            axum::Json(openapi::ApiDoc::openapi())
        }))
        
        // Authentication routes
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/verify-email", post(handlers::auth::verify_email))
        .route("/auth/resend-verification", post(handlers::auth::resend_verification))
        .route("/auth/forgot-password", post(handlers::auth::forgot_password))
        .route("/auth/reset-password", post(handlers::auth::reset_password))
        .route("/auth/passkey/register/start", post(handlers::passkeys::register_start))
        .route("/auth/passkey/register/finish", post(handlers::passkeys::register_finish))
        .route("/auth/passkey/login/start", post(handlers::passkeys::login_start))
        .route("/auth/passkey/login/finish", post(handlers::passkeys::login_finish))
        
        // Link routes
        .route("/links", get(handlers::links::get_user_links).post(handlers::links::create_link))
        .route("/links/bulk", post(handlers::links::bulk_create_links))
        .route("/links/bulk/delete", post(handlers::links::bulk_delete_links))
        .route("/links/bulk/update", post(handlers::links::bulk_update_links))
        .route("/links/export", get(handlers::links::export_links_csv))
        .route("/links/:id", put(handlers::links::update_link).delete(handlers::links::delete_link))
        .route("/links/:id/qr", get(handlers::links::get_qr_code))
        .route("/links/:id/stats", get(handlers::analytics::get_link_stats))
        .route("/links/:id/clicks/realtime", get(handlers::analytics::get_realtime_clicks))
        .route("/links/:id/tags", post(handlers::tags::add_tags_to_link).delete(handlers::tags::remove_tags_from_link))
        
        // Analytics routes
        .route("/analytics/dashboard", get(handlers::analytics::get_dashboard_stats))
        
        // Organization routes
        .route("/orgs", get(handlers::organizations::get_user_organizations).post(handlers::organizations::create_organization))
        .route("/orgs/:org_id", get(handlers::organizations::get_organization)
            .put(handlers::organizations::update_organization)
            .delete(handlers::organizations::delete_organization))
        .route("/orgs/:org_id/members", get(handlers::organizations::get_organization_members)
            .post(handlers::organizations::invite_member))
        .route("/orgs/:org_id/members/:member_id", put(handlers::organizations::update_member_role)
            .delete(handlers::organizations::remove_member))
        .route("/orgs/:org_id/audit", get(handlers::organizations::get_audit_log))
        
        // Folder routes
        .route("/folders", get(handlers::folders::get_folders).post(handlers::folders::create_folder))
        .route("/folders/:folder_id", get(handlers::folders::get_folder)
            .put(handlers::folders::update_folder)
            .delete(handlers::folders::delete_folder))
        .route("/folders/:folder_id/links", get(handlers::folders::get_folder_links)
            .post(handlers::folders::move_links_to_folder))
        
        // Tag routes
        .route("/tags", get(handlers::tags::get_tags).post(handlers::tags::create_tag))
        .route("/tags/:tag_id", get(handlers::tags::get_tag)
            .put(handlers::tags::update_tag)
            .delete(handlers::tags::delete_tag))
        .route("/tags/:tag_id/links", get(handlers::tags::get_links_by_tag))
        
        // Contact form
        .route("/contact", post(handlers::contact::send_contact_message))
        
        // Admin routes (protected)
        .route("/admin/users/:user_id", delete(handlers::admin::delete_user))
        .route("/admin/users/:user_id/hard", delete(handlers::admin::hard_delete_user))
        .route("/admin/users/:user_id/restore", post(handlers::admin::restore_user))
        .route("/admin/users/:user_id/make-admin", post(handlers::admin::make_admin))
        .route("/admin/backup", get(handlers::admin::list_backups).post(handlers::admin::create_backup))
        .route("/admin/backup/cleanup/:keep_count", delete(handlers::admin::cleanup_backups))
        
        // WebSocket for real-time updates
        .route("/ws", get(handlers::websocket::ws_handler))
        .route("/sse", get(handlers::websocket::sse_handler))
        
        // Health check
        .route("/health", get(health_check))
        
        // Redirect route (must be last to not conflict with other routes)
        .route("/:code/verify", post(handlers::links::verify_link_password))
        .route("/:code", get(handlers::links::redirect_link))
        
        // State
        .with_state(app_state)
        
        // HTTPS redirect middleware
        .layer(middleware::from_fn(https_redirect))
        
        // Rate limiting middleware
        .layer(middleware::from_fn_with_state(rate_limiters, rate_limit_middleware))
        
        // CORS
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        
        // Tracing
        .layer(TraceLayer::new_for_http());

    // Start server
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap_or(3000);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting server on {}", addr);
    tracing::info!("Swagger UI available at http://localhost:{}/swagger-ui/", port);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Health check endpoint
async fn health_check(
    axum::extract::State(state): axum::extract::State<AppState>
) -> axum::response::Response {
    use axum::http::StatusCode;
    
    // Check database connection
    let db_ok = sea_orm::DbConn::ping(&state.db).await.is_ok();
    
    if db_ok {
        let email_configured = state.email_service.as_ref().map_or(false, |e| e.is_configured());
        let backup_configured = state.backup.is_configured();
        let status = serde_json::json!({
            "status": "healthy",
            "database": "connected",
            "redis": if state.redis_cache.is_some() { "connected" } else { "disabled" },
            "email": if email_configured { "configured" } else { "disabled" },
            "backup": if backup_configured { "configured" } else { "disabled" }
        });
        (StatusCode::OK, axum::Json(status)).into_response()
    } else {
        let status = serde_json::json!({
            "status": "unhealthy",
            "database": "disconnected"
        });
        (StatusCode::SERVICE_UNAVAILABLE, axum::Json(status)).into_response()
    }
}
