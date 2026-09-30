//! Folder create/access/move/count through the real router + Postgres.
//! Name and color format are not validated by the handler (stored as given).

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::org_members;
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

async fn add_member(db: &DatabaseConnection, org_id: i32, user_id: i32, role: &str) {
    org_members::ActiveModel {
        org_id: Set(org_id),
        user_id: Set(user_id),
        role: Set(role.to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("add org member");
}

async fn create_org(server: &axum_test::TestServer, token: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({
            "name": "Folder Org",
            "slug": format!("fld-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn create_link(server: &axum_test::TestServer, token: &str, url: &str) -> i32 {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

#[tokio::test]
async fn create_personal_folder_with_color_is_listed_and_fetched() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    let created = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "Important", "color": "#FF5733" }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    let body: Value = created.json();
    let folder_id = body["id"].as_i64().unwrap();
    assert_eq!(body["name"].as_str(), Some("Important"));
    assert_eq!(body["color"].as_str(), Some("#FF5733"));
    assert_eq!(body["user_id"].as_i64(), Some(user_id as i64));
    assert!(body["org_id"].is_null());
    assert_eq!(body["link_count"].as_i64(), Some(0));

    let listed: Value = server
        .get("/folders")
        .authorization_bearer(&token)
        .await
        .json();
    let found = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"].as_i64() == Some(folder_id))
        .expect("created folder in list");
    assert_eq!(found["color"].as_str(), Some("#FF5733"));

    let got = server
        .get(&format!("/folders/{folder_id}"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(got.status_code(), 200, "get: {}", got.text());
    assert_eq!(got.json::<Value>()["name"].as_str(), Some("Important"));
}

#[tokio::test]
async fn non_owner_cannot_access_personal_folder() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&server, &db).await;
    let (other_token, _) = register_verified(&server, &db).await;

    let created = server
        .post("/folders")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Private" }))
        .await;
    assert_eq!(created.status_code(), 201);
    let folder_id = created.json::<Value>()["id"].as_i64().unwrap();

    let (server, _) = spawn_real_app().await;
    let got = server
        .get(&format!("/folders/{folder_id}"))
        .authorization_bearer(&other_token)
        .await;
    assert_eq!(
        got.status_code(),
        403,
        "non-owner must not read a personal folder: {}",
        got.text()
    );
}

#[tokio::test]
async fn org_member_can_access_org_folder_non_member_cannot() {
    let (setup, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&setup, &db).await;
    let (member_token, member_id) = register_verified(&setup, &db).await;
    let (stranger_token, _) = register_verified(&setup, &db).await;
    let org_id = create_org(&setup, &owner_token).await;
    add_member(&db, org_id, member_id, "editor").await;

    let created = setup
        .post("/folders")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Team Links", "org_id": org_id }))
        .await;
    assert_eq!(
        created.status_code(),
        201,
        "create org folder: {}",
        created.text()
    );
    let body: Value = created.json();
    let folder_id = body["id"].as_i64().unwrap();
    assert_eq!(body["org_id"].as_i64(), Some(org_id as i64));
    assert!(
        body["user_id"].is_null(),
        "org folder must not keep a personal owner"
    );

    let (server, _) = spawn_real_app().await;
    let listed = server
        .get("/folders")
        .add_query_param("org_id", org_id.to_string())
        .authorization_bearer(&member_token)
        .await;
    assert_eq!(listed.status_code(), 200, "member list: {}", listed.text());
    assert!(
        listed
            .json::<Value>()
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"].as_i64() == Some(folder_id)),
        "org member must see the org folder"
    );

    let got = server
        .get(&format!("/folders/{folder_id}"))
        .authorization_bearer(&member_token)
        .await;
    assert_eq!(got.status_code(), 200, "member get: {}", got.text());

    let stranger_get = server
        .get(&format!("/folders/{folder_id}"))
        .authorization_bearer(&stranger_token)
        .await;
    assert_eq!(
        stranger_get.status_code(),
        403,
        "non-member must not read an org folder: {}",
        stranger_get.text()
    );

    let stranger_list = server
        .get("/folders")
        .add_query_param("org_id", org_id.to_string())
        .authorization_bearer(&stranger_token)
        .await;
    assert_eq!(
        stranger_list.status_code(),
        403,
        "non-member must not list org folders: {}",
        stranger_list.text()
    );
}

#[tokio::test]
async fn move_links_into_folder_updates_listing_and_count() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let folder = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "Inbox" }))
        .await;
    assert_eq!(
        folder.status_code(),
        201,
        "create folder: {}",
        folder.text()
    );
    let folder_id = folder.json::<Value>()["id"].as_i64().unwrap() as i32;

    let empty = server
        .get(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(empty.status_code(), 200);
    assert!(empty.json::<Value>().as_array().unwrap().is_empty());
    assert_eq!(
        server
            .get(&format!("/folders/{folder_id}"))
            .authorization_bearer(&token)
            .await
            .json::<Value>()["link_count"]
            .as_i64(),
        Some(0)
    );

    let a = create_link(&server, &token, "https://iana.org/a").await;
    let b = create_link(&server, &token, "https://iana.org/b").await;
    let c = create_link(&server, &token, "https://iana.org/c").await;

    let (server, _) = spawn_real_app().await;
    let moved = server
        .post(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .json(&json!({ "link_ids": [a, b, c] }))
        .await;
    assert_eq!(moved.status_code(), 200, "move: {}", moved.text());
    assert_eq!(moved.json::<Value>()["moved"].as_i64(), Some(3));

    let listed = server
        .get(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(listed.status_code(), 200, "folder links: {}", listed.text());
    let ids: Vec<i64> = listed
        .json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&(a as i64)));
    assert!(ids.contains(&(b as i64)));
    assert!(ids.contains(&(c as i64)));

    assert_eq!(
        server
            .get(&format!("/folders/{folder_id}"))
            .authorization_bearer(&token)
            .await
            .json::<Value>()["link_count"]
            .as_i64(),
        Some(3)
    );
}
