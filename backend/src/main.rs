//! Thin binary wrapper: environment, logging, database, background services,
//! then serve the router built by the library (`opn_onl_backend::build_router`).
//! All routes and middleware live in `src/lib.rs` so integration tests exercise
//! exactly what this binary serves.

use sea_orm::{Database, DatabaseConnection};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use opn_onl_backend::handlers::websocket::WsState;
use opn_onl_backend::utils::cache::RedisCache;
use opn_onl_backend::utils::{self, BackupService, ClickBuffer, EmailService};
use opn_onl_backend::{build_router, ensure_admin_exists, AppState};

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Fail fast if the JWT secret is missing or too weak. Closes the previous
    // hardcoded-fallback hole where an unset JWT_SECRET let anyone forge admin tokens.
    utils::jwt::validate_jwt_secret();

    // Initialize structured logging
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "logs".to_string());
    let file_appender = tracing_appender::rolling::daily(&log_dir, "opn-onl.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    // Database connection. Required — fail fast rather than silently falling back
    // to an insecure hardcoded dev credential in production.
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (no default is used)");

    let db = Database::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    // Run migrations
    use migration::{Migrator, MigratorTrait};
    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");
    tracing::info!("Migrations completed");

    // Ensure at least one admin exists - promote first user if no admins
    ensure_admin_exists(&db).await;

    // Initialize WebSocket state
    let ws_state = Arc::new(WsState::new());

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

    // Daily sweep anonymizing per-visitor click identifiers past the
    // retention window (ANALYTICS_PII_RETENTION_DAYS, default ~13 months).
    utils::privacy::spawn_retention_task(db.clone());

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
        rate_limiters: std::sync::Arc::new(
            opn_onl_backend::utils::rate_limiter::RateLimiters::new(),
        ),
    };

    // Handles for the graceful-shutdown flush (so buffered clicks aren't lost on
    // deploy/restart). Cloned before `app_state` is moved into the router.
    let shutdown_buffer = app_state.click_buffer.clone();
    let shutdown_db = app_state.db.clone();

    // Build router (routes + middleware defined in the library)
    let app = build_router(app_state);

    // Start server
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting server on {}", addr);
    tracing::info!(
        "Swagger UI available at http://localhost:{}/swagger-ui/",
        port
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    // Serve with ConnectInfo so the rate limiter can use the real socket peer IP.
    // On SIGTERM/Ctrl-C, drain the click buffer so buffered clicks aren't lost.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_buffer, shutdown_db))
    .await
    .unwrap();
}

/// Wait for SIGTERM / Ctrl-C, then flush the click buffer so a deploy or restart
/// doesn't drop buffered (not-yet-persisted) clicks.
async fn shutdown_signal(click_buffer: Arc<ClickBuffer>, db: DatabaseConnection) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received — flushing click buffer before exit");
    click_buffer.flush(&db).await;
    tracing::info!("Click buffer flushed; shutting down");
}
