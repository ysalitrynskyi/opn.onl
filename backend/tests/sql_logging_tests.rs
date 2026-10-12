//! SQL statements must not be logged at INFO.
//!
//! sqlx logs every statement at INFO unless told otherwise, so production with
//! `RUST_LOG=info` wrote a line per query. The pool the binary builds
//! (`connect_options`) logs them at DEBUG and only a statement slower than a
//! second at WARN.
//!
//! This is its own test binary. `tracing` caches callsite interest for the
//! process: another test that reaches the same callsite with no subscriber can
//! cache it as disabled, and the capture would then see nothing.

use std::sync::{Arc, Mutex};

use opn_onl_backend::utils::database_url::connect_options;
use sea_orm::{ConnectionTrait, Database};
use tracing::Level;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

/// Level and rendered fields of every event whose target is sqlx's statement
/// logger.
#[derive(Clone, Default)]
struct StatementLog(Arc<Mutex<Vec<(Level, String)>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

impl<S> Layer<S> for StatementLog
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap()
            .push((*event.metadata().level(), fields.0));
    }
}

fn database_url() -> String {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL").expect("DATABASE_URL must be set")
}

/// Run `body` with a subscriber that lets everything through, so what the
/// capture sees is decided by the pool and not by a filter.
async fn capture(body: impl AsyncFnOnce()) -> Vec<(Level, String)> {
    let log = StatementLog::default();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::TRACE)
        .with(log.clone());
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();
    body().await;
    log.0.lock().unwrap().clone()
}

#[tokio::test(flavor = "current_thread")]
async fn statements_are_logged_at_debug_and_slow_ones_at_warn() {
    let db = Database::connect(connect_options(database_url()))
        .await
        .expect("connect");
    let marker = format!("sqlx_logging_marker_{}", uuid::Uuid::new_v4().simple());

    let seen = capture(async || {
        db.execute_unprepared(&format!("SELECT 1 AS {marker}"))
            .await
            .expect("fast statement");
        db.execute_unprepared(&format!("SELECT pg_sleep(1.2) AS slow_{marker}"))
            .await
            .expect("slow statement");
    })
    .await;

    let level_of = |needle: &str| {
        seen.iter()
            .find(|(_, fields)| fields.contains(needle))
            .map(|(level, _)| *level)
    };
    assert_eq!(
        level_of(&format!("AS {marker}")),
        Some(Level::DEBUG),
        "an ordinary statement is a DEBUG line, not INFO: {seen:#?}"
    );
    assert_eq!(
        level_of(&format!("AS slow_{marker}")),
        Some(Level::WARN),
        "a statement over a second is worth a WARN: {seen:#?}"
    );
}

/// Control: the default options do log at INFO, so the test above is looking at
/// the thing that went wrong and not at a capture that sees nothing.
#[tokio::test(flavor = "current_thread")]
async fn the_default_options_log_every_statement_at_info() {
    let db = Database::connect(database_url()).await.expect("connect");
    let marker = format!("sqlx_default_marker_{}", uuid::Uuid::new_v4().simple());

    let seen = capture(async || {
        db.execute_unprepared(&format!("SELECT 1 AS {marker}"))
            .await
            .expect("statement");
    })
    .await;

    assert!(
        seen.iter()
            .any(|(level, fields)| *level == Level::INFO && fields.contains(&marker)),
        "{seen:#?}"
    );
}
