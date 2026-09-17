//! Tag create/associate/filter/remove through the real router + Postgres.
//! Name format is not validated by the handler (stored as given). Multi-tag
//! AND/OR listing is not a product endpoint; filter is single `tag_id`.

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

async fn create_tag(
    server: &axum_test::TestServer,
    token: &str,
    name: &str,
    color: Option<&str>,
) -> i64 {
    let mut payload = json!({ "name": name });
    if let Some(color) = color {
        payload["color"] = json!(color);
    }
    let res = server
        .post("/tags")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create tag {name}: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap()
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

fn ids(body: &Value) -> Vec<i64> {
    body.as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn create_tag_with_color_is_listed_and_fetched() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;

    let created = server
        .post("/tags")
        .authorization_bearer(&token)
        .json(&json!({ "name": "urgent", "color": "#FF0000" }))
        .await;
    assert_eq!(created.status_code(), 201, "create: {}", created.text());
    let body: Value = created.json();
    let tag_id = body["id"].as_i64().unwrap();
    assert_eq!(body["name"].as_str(), Some("urgent"));
    assert_eq!(body["color"].as_str(), Some("#FF0000"));
    assert_eq!(body["link_count"].as_i64(), Some(0));

    let listed: Value = server
        .get("/tags")
        .authorization_bearer(&token)
        .await
        .json();
    let found = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_i64() == Some(tag_id))
        .expect("created tag in list");
    assert_eq!(found["color"].as_str(), Some("#FF0000"));

    let got = server
        .get(&format!("/tags/{tag_id}"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(got.status_code(), 200, "get: {}", got.text());
    assert_eq!(got.json::<Value>()["name"].as_str(), Some("urgent"));
}

#[tokio::test]
async fn attach_multiple_tags_and_list_links_by_tag() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let tag_a = create_tag(&server, &token, "work", None).await;
    let tag_b = create_tag(&server, &token, "personal", None).await;
    let tag_c = create_tag(&server, &token, "later", None).await;

    let empty = server
        .get(&format!("/tags/{tag_a}/links"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(empty.status_code(), 200);
    assert!(empty.json::<Value>().as_array().unwrap().is_empty());

    let link1 = create_link(&server, &token, "https://iana.org/1").await;
    let link2 = create_link(&server, &token, "https://iana.org/2").await;

    let (server, _) = spawn_real_app().await;
    let added = server
        .post(&format!("/links/{link1}/tags"))
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [tag_a, tag_b, tag_c] }))
        .await;
    assert_eq!(added.status_code(), 200, "add: {}", added.text());
    assert_eq!(added.json::<Value>()["added"].as_i64(), Some(3));

    let again = server
        .post(&format!("/links/{link1}/tags"))
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [tag_a] }))
        .await;
    assert_eq!(again.json::<Value>()["added"].as_i64(), Some(0));

    let added2 = server
        .post(&format!("/links/{link2}/tags"))
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [tag_a] }))
        .await;
    assert_eq!(added2.status_code(), 200);
    assert_eq!(added2.json::<Value>()["added"].as_i64(), Some(1));

    let a_links = server
        .get(&format!("/tags/{tag_a}/links"))
        .authorization_bearer(&token)
        .await
        .json::<Value>();
    let a_ids = ids(&a_links);
    assert_eq!(a_ids.len(), 2);
    assert!(a_ids.contains(&link1));
    assert!(a_ids.contains(&link2));

    let b_links = server
        .get(&format!("/tags/{tag_b}/links"))
        .authorization_bearer(&token)
        .await
        .json::<Value>();
    let b_ids = ids(&b_links);
    assert_eq!(b_ids, vec![link1]);
}

#[tokio::test]
async fn get_links_filters_by_single_tag() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let tag_a = create_tag(&server, &token, "alpha", None).await;
    let tag_b = create_tag(&server, &token, "beta", None).await;
    let link1 = create_link(&server, &token, "https://iana.org/a").await;
    let link2 = create_link(&server, &token, "https://iana.org/b").await;
    let link3 = create_link(&server, &token, "https://iana.org/c").await;

    for (id, tags) in [
        (link1, vec![tag_a, tag_b]),
        (link2, vec![tag_b]),
        (link3, vec![tag_a]),
    ] {
        let res = server
            .post(&format!("/links/{id}/tags"))
            .authorization_bearer(&token)
            .json(&json!({ "tag_ids": tags }))
            .await;
        assert_eq!(res.status_code(), 200, "tag {id}: {}", res.text());
    }

    let (server, _) = spawn_real_app().await;
    let filtered = server
        .get("/links")
        .add_query_param("tag_id", tag_a.to_string())
        .authorization_bearer(&token)
        .await;
    assert_eq!(filtered.status_code(), 200, "filter: {}", filtered.text());
    let filtered_ids = ids(&filtered.json::<Value>());
    assert!(filtered_ids.contains(&link1));
    assert!(filtered_ids.contains(&link3));
    assert!(!filtered_ids.contains(&link2));

    let unused = create_tag(&server, &token, "unused", None).await;
    let none = server
        .get("/links")
        .add_query_param("tag_id", unused.to_string())
        .authorization_bearer(&token)
        .await;
    assert_eq!(none.status_code(), 200);
    assert!(
        ids(&none.json::<Value>()).is_empty(),
        "a tag with no links must match nothing"
    );
}

#[tokio::test]
async fn bulk_add_and_remove_tags_on_a_link() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let tag_a = create_tag(&server, &token, "keep", None).await;
    let tag_b = create_tag(&server, &token, "drop", None).await;
    let link = create_link(&server, &token, "https://iana.org/bulk").await;

    let added = server
        .post(&format!("/links/{link}/tags"))
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [tag_a, tag_b] }))
        .await;
    assert_eq!(added.status_code(), 200, "add: {}", added.text());
    assert_eq!(added.json::<Value>()["added"].as_i64(), Some(2));

    let removed = server
        .delete(&format!("/links/{link}/tags"))
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [tag_b] }))
        .await;
    assert_eq!(removed.status_code(), 200, "remove: {}", removed.text());
    assert_eq!(removed.json::<Value>()["removed"].as_i64(), Some(1));

    let a_links = ids(&server
        .get(&format!("/tags/{tag_a}/links"))
        .authorization_bearer(&token)
        .await
        .json::<Value>());
    let b_links = ids(&server
        .get(&format!("/tags/{tag_b}/links"))
        .authorization_bearer(&token)
        .await
        .json::<Value>());
    assert!(a_links.contains(&link), "kept tag must still list the link");
    assert!(
        !b_links.contains(&link),
        "removed tag must not list the link"
    );
}
