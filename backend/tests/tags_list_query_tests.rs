//! GET /tags link_count and POST /links/{id}/tags must match the live
//! assignments. Real router + real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{link_tags, links, tags};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
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
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id)
}

async fn insert_link(db: &DatabaseConnection, user_id: i32, url: &str) -> i32 {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set(url.to_string()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
    .id
}

async fn insert_tag(db: &DatabaseConnection, user_id: i32, name: &str) -> i32 {
    tags::ActiveModel {
        name: Set(name.to_string()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert tag")
    .id
}

async fn attach_tag(db: &DatabaseConnection, link_id: i32, tag_id: i32) {
    link_tags::ActiveModel {
        link_id: Set(link_id),
        tag_id: Set(tag_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("attach tag");
}

fn count_for(tags: &Value, id: i32) -> i64 {
    tags.as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_i64() == Some(id as i64))
        .and_then(|t| t["link_count"].as_i64())
        .unwrap_or(-1)
}

#[tokio::test]
async fn tag_list_counts_live_links_per_tag_including_orphans() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    let shared = insert_tag(&db, user_id, "shared").await;
    let solo = insert_tag(&db, user_id, "solo").await;
    let orphan = insert_tag(&db, user_id, "orphan").await;

    let a = insert_link(&db, user_id, "https://iana.org/a").await;
    let b = insert_link(&db, user_id, "https://iana.org/b").await;
    let gone = insert_link(&db, user_id, "https://iana.org/gone").await;
    attach_tag(&db, a, shared).await;
    attach_tag(&db, b, shared).await;
    attach_tag(&db, a, solo).await;
    attach_tag(&db, gone, shared).await;
    attach_tag(&db, gone, orphan).await;

    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [gone] }))
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    let listed: Value = server.get("/tags").authorization_bearer(&token).await.json();
    assert_eq!(count_for(&listed, shared), 2, "got {listed}");
    assert_eq!(count_for(&listed, solo), 1, "got {listed}");
    assert_eq!(count_for(&listed, orphan), 0, "orphan must stay at 0, got {listed}");
}

#[tokio::test]
async fn attach_tags_inserts_only_new_in_scope_ids() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (_other, other_id) = register_verified(&server, &db).await;

    let link = insert_link(&db, user_id, "https://iana.org/tagged").await;
    let keep = insert_tag(&db, user_id, "keep").await;
    let extra = insert_tag(&db, user_id, "extra").await;
    let foreign = insert_tag(&db, other_id, "foreign").await;
    attach_tag(&db, link, keep).await;

    let res = server
        .post(&format!("/links/{link}/tags"))
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [keep, extra, foreign, extra, 9_999_999] }))
        .await;
    assert_eq!(res.status_code(), 200, "attach: {}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["added"].as_i64(),
        Some(1),
        "only the new in-scope tag is added, got {body}"
    );

    let listed: Value = server.get("/links").authorization_bearer(&token).await.json();
    let tags = listed.as_array().unwrap()[0]["tags"].as_array().unwrap();
    let mut ids: Vec<i64> = tags.iter().map(|t| t["id"].as_i64().unwrap()).collect();
    ids.sort();
    assert_eq!(ids, vec![keep as i64, extra as i64]);
}
