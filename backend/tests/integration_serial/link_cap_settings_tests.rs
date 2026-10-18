//! `/auth/settings` publishes the per-user link cap (`MAX_LINKS_PER_USER`) the
//! same way link creation reads it.
//!
//! Creation treats 0, unset and anything unreadable as "no cap", but the
//! settings endpoint parsed the variable on its own and reported
//! `max_links_per_user: 0` for `MAX_LINKS_PER_USER=0`, which a client reads as
//! "you may create no links". It reports `null` for no cap now.
//!
//! The variable is process-wide, so this runs alone (`integration_serial`) as
//! one sequential test.

use crate::common;

use std::sync::Arc;

use common::{mark_email_verified, unique_email};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use opn_onl_backend::{AppState, build_router};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

/// The real router with request limits out of the way: the test reads the
/// settings endpoint and creates links a dozen times in a second.
async fn spawn_real_app() -> (axum_test::TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = common::setup_test_db().await;
    let mut state = AppState::for_tests(db.clone()).await;
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
        report: roomy(),
    });
    let server = axum_test::TestServer::new(build_router(state)).expect("test server");
    (server, db)
}

#[tokio::test]
async fn the_published_link_cap_is_the_one_creation_enforces() {
    let _serial = crate::common::serial_guard().await;
    common::remove_env("MAX_LINKS_PER_USER");
    let (server, db) = spawn_real_app().await;

    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    let body: Value = res.json();
    let token = body["token"].as_str().unwrap().to_string();
    mark_email_verified(&db, body["user_id"].as_i64().unwrap() as i32).await;

    let published = async || -> Value {
        let res = server.get("/auth/settings").await;
        assert_eq!(res.status_code(), 200);
        res.json::<Value>()["max_links_per_user"].clone()
    };
    let create = async |url: &str| -> u16 {
        server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": url }))
            .await
            .status_code()
            .as_u16()
    };

    // No cap: unset, zero, blank, negative and unreadable all read `null`.
    for (value, label) in [
        (None, "unset"),
        (Some("0"), "zero"),
        (Some(""), "blank"),
        (Some("-3"), "negative"),
        (Some("plenty"), "unreadable"),
    ] {
        match value {
            Some(value) => common::set_env("MAX_LINKS_PER_USER", value),
            None => common::remove_env("MAX_LINKS_PER_USER"),
        }
        assert_eq!(published().await, Value::Null, "{label}");
    }

    // Zero really is no cap: creation is not refused.
    common::set_env("MAX_LINKS_PER_USER", "0");
    assert_eq!(create("https://iana.org/cap-0-a").await, 201);
    assert_eq!(create("https://iana.org/cap-0-b").await, 201);

    // A positive cap is published as it is, and is what creation enforces.
    common::set_env("MAX_LINKS_PER_USER", "2");
    assert_eq!(published().await, 2);
    assert_eq!(
        create("https://iana.org/cap-2").await,
        403,
        "the account already holds two links"
    );

    // Too large for the field: still a cap, never `null`.
    common::set_env("MAX_LINKS_PER_USER", "99999999999");
    assert_eq!(published().await, i32::MAX);

    common::remove_env("MAX_LINKS_PER_USER");
}
