//! Link-in-bio handler tests. Real router + real Postgres via
//! `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
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

fn unique_bio_username() -> String {
    format!("bio{}", unique_code().to_lowercase())
}

/// Anonymous GET /api/bio/{username} must not include per-link click counts.
/// Traffic volume for a short link is owner analytics, not public profile data.
#[tokio::test]
async fn public_bio_omits_per_link_click_counts() {
    let (server, db) = spawn_real_app().await;
    let (token, _user_id) = register_verified(&server, &db).await;
    let username = unique_bio_username();

    let settings = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({
            "bio_username": username,
            "bio_enabled": true,
        }))
        .await;
    assert_eq!(
        settings.status_code(),
        200,
        "enable bio: {}",
        settings.text()
    );

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/bio-click-leak",
            "title": "Public link",
            "bio_visible": true,
        }))
        .await;
    assert_eq!(created.status_code(), 201, "create link: {}", created.text());
    let link_id = created.json::<Value>()["id"].as_i64().unwrap() as i32;
    let code = created.json::<Value>()["code"].as_str().unwrap().to_string();

    // Some create paths ignore bio_visible; force it on the stored row either way.
    let make_visible = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .json(&json!({ "bio_visible": true }))
        .await;
    assert!(
        make_visible.status_code().is_success(),
        "mark bio-visible: {}",
        make_visible.text()
    );

    let model = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    let mut active: links::ActiveModel = model.into();
    active.click_count = Set(42);
    active.update(&db).await.expect("set click_count");

    let res = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(res.status_code(), 200, "public bio: {}", res.text());
    let body: Value = res.json();
    let links = body["links"].as_array().expect("links array");
    assert_eq!(links.len(), 1, "one bio-visible link: {body}");
    assert_eq!(links[0]["code"].as_str(), Some(code.as_str()));
    assert_eq!(links[0]["label"].as_str(), Some("Public link"));
    assert!(
        links[0].get("click_count").is_none(),
        "public bio must not leak click_count, got {}",
        links[0]
    );
}
