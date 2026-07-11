//! Regression tests for correctness findings in docs/CODE_AUDIT_2026-07.md.
//! Real router + real Postgres via `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    mark_email_verified(db, body["user_id"].as_i64().unwrap() as i32).await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_link(server: &axum_test::TestServer, token: &str, url: &str) -> i64 {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap()
}

fn tag_link_count(tags: &Value, tag_id: i64) -> i64 {
    tags.as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_i64() == Some(tag_id))
        .expect("tag present")["link_count"]
        .as_i64()
        .unwrap()
}

/// A tag's reported `link_count` must not include soft-deleted links. Counting
/// raw link_tags rows over-counted, disagreeing with the tag's link listing.
#[tokio::test]
async fn tag_link_count_excludes_soft_deleted_links() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let tag_id = server
        .post("/tags")
        .authorization_bearer(&token)
        .json(&json!({ "name": "campaign" }))
        .await
        .json::<Value>()["id"]
        .as_i64()
        .unwrap();

    let id1 = create_link(&server, &token, "https://example.com/1").await;
    let id2 = create_link(&server, &token, "https://example.com/2").await;
    for id in [id1, id2] {
        let res = server
            .post(&format!("/links/{id}/tags"))
            .authorization_bearer(&token)
            .json(&json!({ "tag_ids": [tag_id] }))
            .await;
        assert_eq!(res.status_code(), 200, "tag link {id}: {}", res.text());
    }

    let tags: Value = server.get("/tags").authorization_bearer(&token).await.json();
    assert_eq!(tag_link_count(&tags, tag_id), 2, "both tagged links should count");

    // Soft-delete one link; the tag count must drop to 1.
    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [id1] }))
        .await;
    assert_eq!(del.status_code(), 200, "bulk delete: {}", del.text());

    let tags: Value = server.get("/tags").authorization_bearer(&token).await.json();
    assert_eq!(
        tag_link_count(&tags, tag_id),
        1,
        "soft-deleted link must not be counted in the tag's link_count"
    );
}

/// A custom alias previously used by a now-deleted link cannot be reused: the
/// global UNIQUE on links.code still holds it. Reuse must be a clean 409 (the
/// old ALLOW_DELETED_SLUG_REUSE path 500'd), and check-code must report it taken.
#[tokio::test]
async fn deleted_alias_cannot_be_reused() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let alias = unique_code();

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://example.com/orig", "custom_alias": alias }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    let id = created.json::<Value>()["id"].as_i64().unwrap();

    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [id] }))
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    // The alias is still held by the deleted link's code — report it unavailable.
    let cc: Value = server
        .get(&format!("/links/check-code?code={alias}"))
        .await
        .json();
    assert_eq!(cc["available"].as_bool(), Some(false), "deleted alias must not be available");

    // Reuse must be a clean 409, never a 500 from the UNIQUE violation.
    let reuse = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://example.com/new", "custom_alias": alias }))
        .await;
    assert_eq!(
        reuse.status_code(),
        409,
        "reusing a deleted alias must be 409, got {}: {}",
        reuse.status_code(),
        reuse.text()
    );
}

/// Changing the password revokes the old JWT (token_version bump) but must hand
/// back a fresh token so the session survives — otherwise the client is silently
/// logged out on its next request.
#[tokio::test]
async fn change_password_rotates_token_without_logging_out() {
    let (server, _db) = spawn_real_app().await;
    let email = unique_email();
    let reg = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(reg.status_code(), 201, "register: {}", reg.text());
    let old_token = reg.json::<Value>()["token"].as_str().unwrap().to_string();

    let changed = server
        .post("/auth/change-password")
        .authorization_bearer(&old_token)
        .json(&json!({ "current_password": "password123", "new_password": "newpassword456" }))
        .await;
    assert_eq!(changed.status_code(), 200, "change password: {}", changed.text());
    let new_token = changed.json::<Value>()["token"]
        .as_str()
        .expect("a fresh token in the change-password response")
        .to_string();
    assert_ne!(new_token, old_token, "a new token must be issued");

    // Old token is revoked; new token works.
    let with_old = server.get("/auth/me").authorization_bearer(&old_token).await;
    assert_eq!(with_old.status_code(), 401, "old token must be revoked after password change");

    let with_new = server.get("/auth/me").authorization_bearer(&new_token).await;
    assert_eq!(with_new.status_code(), 200, "fresh token must keep the session: {}", with_new.text());
}
