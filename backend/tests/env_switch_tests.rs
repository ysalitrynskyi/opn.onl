//! On/off switches (`ENABLE_*`) were read three different ways: an exact
//! lower-case `false`, an exact lower-case `true`, and `parse::<bool>()`. So
//! `ENABLE_PASSKEYS=False` or `=0` left passkeys on while the operator believed
//! they were off, and `ENABLE_ACCOUNT_DELETION=True` or `=1` left deletion
//! off. Every switch now goes through one reader.

mod common;

use std::sync::Arc;

use common::{mark_email_verified, remove_env, set_env, unique_email};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use opn_onl_backend::{AppState, build_router};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

/// The real router with request limits out of the way: these tests read the
/// settings endpoint hundreds of times.
async fn spawn_real_app() -> (axum_test::TestServer, DatabaseConnection) {
    set_env("FORCE_HTTPS", "false");
    set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
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

/// The switches read per request, all in one place so a test can set and clear
/// them together. The environment is process-wide; one lock keeps the tests in
/// this file from interleaving.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const SWITCHES: &[(&str, &str, bool)] = &[
    // (variable, key in GET /auth/settings, default)
    ("ENABLE_ACCOUNT_DELETION", "account_deletion_enabled", false),
    ("ENABLE_CUSTOM_ALIASES", "custom_aliases_enabled", true),
    ("ENABLE_URL_SANITIZATION", "url_sanitization_enabled", true),
    ("ENABLE_QR_BRANDING", "qr_branding_enabled", true),
    (
        "ENABLE_BURN_AFTER_READING",
        "burn_after_reading_enabled",
        true,
    ),
    (
        "ENABLE_SAFE_LINK_INTERSTITIAL",
        "safe_link_interstitial_enabled",
        true,
    ),
    (
        "ENABLE_CONDITIONAL_ROUTING",
        "conditional_routing_enabled",
        true,
    ),
    ("ENABLE_LINK_IN_BIO", "link_in_bio_enabled", true),
    ("ENABLE_API_KEYS", "api_keys_enabled", true),
    ("ENABLE_PASSKEYS", "passkeys_enabled", true),
];

fn clear_switches() {
    for (var, _, _) in SWITCHES {
        remove_env(var);
    }
}

async fn settings(server: &axum_test::TestServer) -> Value {
    let res = server.get("/auth/settings").await;
    assert_eq!(res.status_code(), 200);
    res.json()
}

#[tokio::test]
async fn every_switch_honours_the_spellings_an_operator_types() {
    let _lock = ENV_LOCK.lock().await;
    let (server, _db) = spawn_real_app().await;

    for (var, key, default) in SWITCHES {
        // Unset and blank: the default, and the default is what the old
        // readers gave too.
        clear_switches();
        assert_eq!(settings(&server).await[key], *default, "{var} unset");
        set_env(var, "");
        assert_eq!(settings(&server).await[key], *default, "{var} blank");

        for off in ["false", "False", "FALSE", "0", "no", "off"] {
            set_env(var, off);
            assert_eq!(settings(&server).await[key], false, "{var}={off}");
        }
        for on in ["true", "True", "TRUE", "1", "yes", "on"] {
            set_env(var, on);
            assert_eq!(settings(&server).await[key], true, "{var}={on}");
        }

        // Garbage is not a guess at what the operator meant: the default
        // stays, whichever way it points.
        set_env(var, "perhaps");
        assert_eq!(settings(&server).await[key], *default, "{var}=perhaps");
    }
    clear_switches();
}

#[tokio::test]
async fn a_switched_off_custom_alias_is_refused_whatever_the_spelling() {
    let _lock = ENV_LOCK.lock().await;
    let (server, db) = spawn_real_app().await;
    clear_switches();

    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    let body: Value = res.json();
    let token = body["token"].as_str().unwrap().to_string();
    mark_email_verified(&db, body["user_id"].as_i64().unwrap() as i32).await;

    let create = |alias: String| {
        let server = &server;
        let token = token.clone();
        async move {
            server
                .post("/links")
                .authorization_bearer(&token)
                .json(&json!({
                    "original_url": "https://iana.org/switches",
                    "custom_alias": alias,
                }))
                .await
                .status_code()
                .as_u16()
        }
    };

    // On by default.
    assert_eq!(create(format!("sw{}", common::unique_code())).await, 201);

    for off in ["False", "0", "no", "off"] {
        set_env("ENABLE_CUSTOM_ALIASES", off);
        assert_eq!(
            create(format!("sw{}", common::unique_code())).await,
            403,
            "ENABLE_CUSTOM_ALIASES={off} must refuse custom aliases"
        );
    }
    clear_switches();
}
