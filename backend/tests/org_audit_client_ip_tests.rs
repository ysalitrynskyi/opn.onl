//! The client address on organization audit entries: the one the rate limiter
//! derives, truncated like the address of a click event before it is stored.
//!
//! Its own file because proxy-header trust is read once per process: every test
//! here wants it on, and the shared `spawn_real_app` pins it off.

mod common;

use axum::http::{HeaderName, HeaderValue};
use axum_test::TestServer;
use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::users;
use opn_onl_backend::utils::jwt::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::{Value, json};

async fn app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "true");
    common::set_env("REAL_IP_HEADER", "cf-connecting-ip");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}

async fn account(db: &DatabaseConnection) -> (users::Model, String) {
    let user = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &user.email, user.token_version).expect("mint token");
    (user, token)
}

fn from_ip(ip: &str) -> (HeaderName, HeaderValue) {
    (
        HeaderName::from_static("cf-connecting-ip"),
        HeaderValue::from_str(ip).unwrap(),
    )
}

#[tokio::test]
async fn audit_entries_carry_the_truncated_client_address() {
    let (server, db) = app().await;
    let (_, owner_token) = account(&db).await;
    let (invitee, _) = account(&db).await;

    let (name, value) = from_ip("203.0.113.77");
    let created = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .add_header(name, value)
        .json(&json!({
            "name": "Audit IP Org",
            "slug": format!("aip-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(created.status_code(), 201, "create org: {}", created.text());
    let org_id = created.json::<Value>()["id"].as_i64().unwrap();

    let (name, value) = from_ip("2001:db8:85a3:8d3:1319:8a2e:370:7348");
    let invited = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .add_header(name, value)
        .json(&json!({ "email": &invitee.email, "role": "viewer" }))
        .await;
    assert_eq!(invited.status_code(), 201, "invite: {}", invited.text());

    // No forwarding header and no socket (the test transport has none): there
    // is no address to record, and nothing is invented.
    let renamed = server
        .put(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Audit IP Org 2" }))
        .await;
    assert_eq!(renamed.status_code(), 200, "rename: {}", renamed.text());

    let (name, value) = from_ip("198.51.100.9");
    let raw = server
        .get(&format!("/orgs/{org_id}/audit"))
        .authorization_bearer(&owner_token)
        .add_header(name, value)
        .await;
    assert_eq!(raw.status_code(), 200, "audit: {}", raw.text());
    let entries = raw.json::<Vec<Value>>();
    let ip_of = |action: &str| {
        entries
            .iter()
            .find(|entry| entry["action"] == action)
            .unwrap_or_else(|| panic!("no {action} entry in {entries:?}"))["ip_address"]
            .clone()
    };

    assert_eq!(ip_of("create"), "203.0.113.0", "IPv4 keeps the /24");
    assert_eq!(ip_of("invite"), "2001:db8:85a3::", "IPv6 keeps the /48");
    assert_eq!(ip_of("update"), Value::Null);

    let stored = raw.text();
    assert!(
        !stored.contains("203.0.113.77") && !stored.contains("370:7348"),
        "a full address must never be stored: {stored}"
    );
}
