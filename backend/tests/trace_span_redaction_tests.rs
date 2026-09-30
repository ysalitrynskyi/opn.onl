//! The HTTP trace span must record the redacted URI, not only the helper.
//!
//! This is its own test binary. `tracing` caches callsite interest for the
//! process: another test that hits the same request span with no subscriber
//! can cache that callsite as disabled, and this assertion then sees no `uri`.

mod common;

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

#[derive(Clone, Default)]
struct UriCapture(Arc<Mutex<Vec<String>>>);

struct UriVisitor(Option<String>);

impl Visit for UriVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "uri" {
            self.0 = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "uri" {
            self.0 = Some(value.to_string());
        }
    }
}

impl<S> Layer<S> for UriCapture
where
    S: tracing::Subscriber,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut visitor = UriVisitor(None);
        attrs.record(&mut visitor);
        // Only the HTTP request span carries `uri`. Recording every span would
        // bury a missed redaction under database noise.
        if attrs.metadata().name() == "request" {
            self.0.lock().unwrap().push(visitor.0.unwrap_or_default());
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn http_trace_span_records_redacted_token() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with(UriCapture(captured.clone()));
    let _guard = tracing::subscriber::set_default(subscriber);
    // Drop a disabled-interest cache if anything in startup already touched
    // this callsite before the subscriber existed.
    tracing::callsite::rebuild_interest_cache();

    let (server, _) = common::spawn_real_app().await;
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
    let _ = server.get(&format!("/health?token={jwt}")).await;

    let uris = captured.lock().unwrap().clone();
    assert!(
        uris.iter().any(|u| u.contains("token=REDACTED")),
        "trace span must record token=REDACTED, got {uris:?}"
    );
    assert!(
        uris.iter().all(|u| !u.contains(jwt)),
        "raw JWT must not appear in span uri, got {uris:?}"
    );
}
