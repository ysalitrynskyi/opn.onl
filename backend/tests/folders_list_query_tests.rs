//! GET /folders and GET /folders/{id}/links must keep counts and tags correct
//! after the per-row queries are batched. Real router + real Postgres.

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

async fn insert_link_in_folder(
    db: &DatabaseConnection,
    user_id: i32,
    folder_id: i32,
    url: &str,
) -> i32 {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set(url.to_string()),
        user_id: Set(Some(user_id)),
        folder_id: Set(Some(folder_id)),
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

fn tag_ids_for(link: &Value) -> Vec<i64> {
    let mut ids: Vec<i64> = link["tags"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|t| t["id"].as_i64().unwrap())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn folder_list_counts_live_links_only_including_empty_folders() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    let populated = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "populated" }))
        .await;
    assert_eq!(populated.status_code(), 201, "{}", populated.text());
    let populated_id = populated.json::<Value>()["id"].as_i64().unwrap() as i32;

    let empty = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "empty" }))
        .await;
    assert_eq!(empty.status_code(), 201, "{}", empty.text());
    let empty_id = empty.json::<Value>()["id"].as_i64().unwrap() as i32;

    let live_a = insert_link_in_folder(&db, user_id, populated_id, "https://iana.org/a").await;
    let _live_b = insert_link_in_folder(&db, user_id, populated_id, "https://iana.org/b").await;
    let gone = insert_link_in_folder(&db, user_id, populated_id, "https://iana.org/gone").await;
    let _ = live_a;

    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [gone] }))
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    let listed: Value = server
        .get("/folders")
        .authorization_bearer(&token)
        .await
        .json();
    let folders = listed.as_array().expect("array");
    let count_for = |id: i32| {
        folders
            .iter()
            .find(|f| f["id"].as_i64() == Some(id as i64))
            .and_then(|f| f["link_count"].as_i64())
    };
    assert_eq!(count_for(populated_id), Some(2), "got {listed}");
    assert_eq!(count_for(empty_id), Some(0), "empty folder must report 0");
}

#[tokio::test]
async fn folder_links_return_each_link_with_exactly_its_tags() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    let folder = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "tagged" }))
        .await;
    let folder_id = folder.json::<Value>()["id"].as_i64().unwrap() as i32;

    let tagged = insert_link_in_folder(&db, user_id, folder_id, "https://iana.org/tagged").await;
    let untagged = insert_link_in_folder(&db, user_id, folder_id, "https://iana.org/untagged").await;
    let gone = insert_link_in_folder(&db, user_id, folder_id, "https://iana.org/gone").await;
    let outside = insert_link(&db, user_id, "https://iana.org/outside").await;

    let alpha = insert_tag(&db, user_id, "alpha").await;
    let beta = insert_tag(&db, user_id, "beta").await;
    let _orphan = insert_tag(&db, user_id, "orphan").await;
    attach_tag(&db, tagged, alpha).await;
    attach_tag(&db, tagged, beta).await;
    attach_tag(&db, gone, alpha).await;
    attach_tag(&db, outside, beta).await;

    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [gone] }))
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    let listed: Value = server
        .get(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .await
        .json();
    let links = listed.as_array().expect("array");
    let ids: Vec<i64> = links.iter().filter_map(|l| l["id"].as_i64()).collect();
    assert_eq!(ids.len(), 2, "live folder links only, got {ids:?}");
    assert!(ids.contains(&(tagged as i64)) && ids.contains(&(untagged as i64)));
    assert!(!ids.contains(&(gone as i64)) && !ids.contains(&(outside as i64)));

    let by_id = |id: i32| {
        links
            .iter()
            .find(|l| l["id"].as_i64() == Some(id as i64))
            .unwrap()
    };
    assert_eq!(tag_ids_for(by_id(tagged)), vec![alpha as i64, beta as i64]);
    assert_eq!(tag_ids_for(by_id(untagged)), vec![] as Vec<i64>);
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
