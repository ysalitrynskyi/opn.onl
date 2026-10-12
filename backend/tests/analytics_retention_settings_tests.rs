//! `/auth/settings` publishes the analytics identifier retention window so the
//! privacy policy can quote the operator's number instead of a fixed one. Reads
//! and writes `ANALYTICS_PII_RETENTION_DAYS`, so it lives in its own test binary
//! and runs as one sequential test.

mod common;

use common::spawn_real_app;
use serde_json::Value;

#[tokio::test]
async fn the_retention_window_is_published() {
    common::remove_env("ANALYTICS_PII_RETENTION_DAYS");
    let (server, _db) = spawn_real_app().await;
    let settings = async || -> Value { server.get("/auth/settings").await.json() };

    // Default: 396 days, the 13 months the policy has always promised.
    assert_eq!(settings().await["analytics_pii_retention_days"], 396);

    // The operator's window is what is reported.
    common::set_env("ANALYTICS_PII_RETENTION_DAYS", "90");
    assert_eq!(settings().await["analytics_pii_retention_days"], 90);

    // Unusable values fall back to the default, as the sweep itself does.
    common::set_env("ANALYTICS_PII_RETENTION_DAYS", "soon");
    assert_eq!(settings().await["analytics_pii_retention_days"], 396);

    // Disabled: nothing is anonymized, so no window is promised.
    common::set_env("ANALYTICS_PII_RETENTION_DAYS", "0");
    assert!(settings().await["analytics_pii_retention_days"].is_null());

    common::remove_env("ANALYTICS_PII_RETENTION_DAYS");
}
