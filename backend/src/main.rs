//! Thin binary wrapper: environment, logging, database, background services,
//! then serve the router built by the library (`opn_onl_backend::build_router`).
//! All routes and middleware live in `src/lib.rs` so integration tests exercise
//! exactly what this binary serves.

use sea_orm::Database;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use opn_onl_backend::handlers::websocket::WsState;
use opn_onl_backend::utils::backup::spawn_backup_task;
use opn_onl_backend::utils::cache::RedisCache;
use opn_onl_backend::utils::env::{env_flag, env_parse};
use opn_onl_backend::utils::logging::{DEFAULT_LOG_MAX_FILES, daily_log_appender};
use opn_onl_backend::utils::{self, BackupService, ClickBuffer, EmailService};
use opn_onl_backend::{AppState, build_router, ensure_admin_exists, warn_on_email_case_collisions};

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Fail fast if the JWT secret is missing or too weak. Closes the previous
    // hardcoded-fallback hole where an unset JWT_SECRET let anyone forge admin tokens.
    utils::jwt::validate_jwt_secret();

    // Fail fast if WEBAUTHN_RP_ID is set to a host the frontend origin cannot
    // use. A passkey registered under the wrong RP ID is silently unusable.
    opn_onl_backend::handlers::passkeys::validate_webauthn_rp_id();

    // Initialize structured logging
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "logs".to_string());
    let log_max_files = env_parse("LOG_MAX_FILES", DEFAULT_LOG_MAX_FILES);
    let file_appender = daily_log_appender(&log_dir, log_max_files)
        .expect("failed to open the log file; check LOG_DIR is writable");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    // Database connection. Required — fail fast rather than silently falling back
    // to an insecure hardcoded dev credential in production. Compose passes
    // discrete POSTGRES_* pieces; we assemble (and percent-encode userinfo) here
    // so a password containing `@` / `:` / `/` does not split the URL.
    let database_url = utils::database_url::resolve_database_url();

    let db = Database::connect(utils::database_url::connect_options(database_url))
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
    warn_on_email_case_collisions(&db).await;

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
    let flush_task = click_buffer.clone().start_flush_task(db.clone());
    tracing::info!("Click buffer initialized");

    // Daily sweep anonymizing per-visitor click identifiers past the
    // retention window (ANALYTICS_PII_RETENTION_DAYS, default ~13 months).
    utils::privacy::spawn_retention_task(db.clone());

    // Read once. Blank or unrecognised stays on, the same default `env_flag`
    // gives every other switch; missing SMTP must not imply this is off.
    let require_email_verification = env_flag("REQUIRE_EMAIL_VERIFICATION", true);
    opn_onl_backend::warn_accounts_cannot_be_verified(
        require_email_verification,
        email_service.is_some(),
    );

    // Daily reminder to owners whose links expire within three days
    // (LINK_EXPIRY_REMINDERS, default on; needs SMTP).
    utils::link_expiry::spawn_link_expiry_task(
        db.clone(),
        email_service.clone(),
        require_email_verification,
    );

    // Initialize backup service
    let backup = Arc::new(BackupService::new().await);
    if backup.is_configured() {
        tracing::info!("Backup service enabled");
    } else {
        tracing::info!("Backup service disabled (S3/R2 not configured)");
    }
    spawn_backup_task(backup.clone(), ws_state.shutdown_watch());

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
        require_email_verification,
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
    // On SIGTERM/Ctrl-C, drain connections first, then join the in-flight click
    // flush so a deploy cannot drop a batch the background task already took.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(ws_state.shutdown_after(shutdown_signal()))
    .await
    .unwrap();

    // Clicks first: they are the data a deploy would otherwise lose, and the
    // orchestrator's kill timer (compose `stop_grace_period`, 10 s by default)
    // is already running. Waiting for mail first let one slow SMTP exchange
    // use up that time and end the process before the flush.
    tracing::info!("Server stopped — draining click buffer");
    shutdown_buffer.request_stop();
    let _ = flush_task.await;
    shutdown_buffer.flush(&shutdown_db).await;
    tracing::info!("Click buffer flushed");

    // Mail a request promised before it was answered is sent from a task of its
    // own; let it finish before the runtime goes away.
    if !utils::email::wait_for_background_mail(std::time::Duration::from_secs(10)).await {
        tracing::warn!("Shutting down while mail was still being sent");
    }
    tracing::info!("Shutting down");
}

/// Wait for SIGTERM / Ctrl-C. The click buffer is drained after `serve` returns
/// so in-flight HTTP clicks and an in-flight flush are both persisted.
async fn shutdown_signal() {
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

    tracing::info!("Shutdown signal received");
}
