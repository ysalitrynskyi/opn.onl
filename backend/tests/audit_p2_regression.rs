//! Regression test for the P2 access-control finding in docs/CODE_AUDIT_2026-07.md:
//! read-only org members (viewer role) could rewrite an org link's routing rules.
//! Real router + real Postgres via `common::spawn_real_app`.
//!
//! The other P2 items in this batch are configuration (nginx CSP/HSTS + header
//! inheritance, pgAdmin auth, default DB password) and are validated by
//! `docker compose config` / nginx rather than a Rust test.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::org_members;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{json, Value};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let token = body["token"].as_str().unwrap().to_string();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (token, user_id)
}

async fn add_member(db: &DatabaseConnection, org_id: i32, user_id: i32, role: &str) {
    org_members::ActiveModel {
        org_id: Set(org_id),
        user_id: Set(user_id),
        role: Set(role.to_string()),
        joined_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("add org member");
}

async fn put_rules(server: &axum_test::TestServer, token: &str, link_id: i64) -> u16 {
    server
        .put(&format!("/links/{link_id}/rules"))
        .authorization_bearer(token)
        .json(&json!({ "rules": [{ "destination_url": "https://example.com/routed" }] }))
        .await
        .status_code()
        .as_u16()
}

async fn post_org_link(server: &axum_test::TestServer, token: &str, org_id: i32) -> u16 {
    server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": "https://example.com/new", "org_id": org_id }))
        .await
        .status_code()
        .as_u16()
}

#[tokio::test]
async fn viewer_cannot_rewrite_org_link_routing() {
    let (server, db) = spawn_real_app().await;

    // Owner creates an organization and an org-owned link.
    let (owner_token, _owner_id) = register_verified(&server, &db).await;
    let slug = format!("org-{}", unique_email().replace(['@', '.'], "-"));
    let org = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Test Org", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap() as i32;

    let link = server
        .post("/links")
        .authorization_bearer(&owner_token)
        .json(&json!({ "original_url": "https://example.com/target", "org_id": org_id }))
        .await;
    assert_eq!(link.status_code(), 201, "create org link: {}", link.text());
    let link_id = link.json::<Value>()["id"].as_i64().unwrap();

    // A viewer and an editor join the org.
    let (viewer_token, viewer_id) = register_verified(&server, &db).await;
    let (editor_token, editor_id) = register_verified(&server, &db).await;
    add_member(&db, org_id, viewer_id, "viewer").await;
    add_member(&db, org_id, editor_id, "editor").await;

    // Regression: the viewer must be forbidden from rewriting routing rules.
    assert_eq!(
        put_rules(&server, &viewer_token, link_id).await,
        403,
        "viewer must not be able to rewrite org link routing"
    );

    // The editor (edit rights) and the owner still can.
    assert_ne!(
        put_rules(&server, &editor_token, link_id).await,
        403,
        "editor should be allowed to edit routing"
    );
    assert_eq!(
        put_rules(&server, &owner_token, link_id).await,
        200,
        "owner should be allowed to edit routing"
    );
}

#[tokio::test]
async fn viewer_cannot_create_org_links() {
    let (server, db) = spawn_real_app().await;

    let (owner_token, _owner_id) = register_verified(&server, &db).await;
    let slug = format!("org-{}", unique_email().replace(['@', '.'], "-"));
    let org = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Test Org", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap() as i32;

    let (viewer_token, viewer_id) = register_verified(&server, &db).await;
    let (editor_token, editor_id) = register_verified(&server, &db).await;
    add_member(&db, org_id, viewer_id, "viewer").await;
    add_member(&db, org_id, editor_id, "editor").await;

    assert_eq!(
        post_org_link(&server, &viewer_token, org_id).await,
        403,
        "viewer must not be able to create org links"
    );
    assert_eq!(
        post_org_link(&server, &editor_token, org_id).await,
        201,
        "editor should be able to create org links"
    );
}

#[tokio::test]
async fn viewer_cannot_bulk_create_org_links() {
    let (server, db) = spawn_real_app().await;

    let (owner_token, _owner_id) = register_verified(&server, &db).await;
    let slug = format!("org-{}", unique_email().replace(['@', '.'], "-"));
    let org = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Test Org", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap() as i32;

    let (viewer_token, viewer_id) = register_verified(&server, &db).await;
    add_member(&db, org_id, viewer_id, "viewer").await;

    let res = server
        .post("/links/bulk")
        .authorization_bearer(&viewer_token)
        .json(&json!({ "urls": ["https://example.com/bulk"], "org_id": org_id }))
        .await;
    assert_eq!(res.status_code(), 200, "bulk create: {}", res.text());
    let body = res.json::<Value>();
    assert_eq!(
        body["links"].as_array().unwrap().len(),
        0,
        "viewer bulk create must not create links"
    );
    assert_eq!(
        body["errors"].as_array().unwrap().len(),
        1,
        "viewer bulk create should report an authorization error"
    );
}
