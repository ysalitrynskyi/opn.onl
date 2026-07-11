//! Regression tests for correctness findings in docs/CODE_AUDIT_2026-07.md.
//! Real router + real Postgres via `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
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
