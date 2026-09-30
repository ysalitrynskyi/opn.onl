//! GET /links must return the right rows with the right tags attached, and
//! must not load an unbounded page. Real router + real Postgres.

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

fn find_link(links: &[Value], id: i32) -> &Value {
    links
        .iter()
        .find(|l| l["id"].as_i64() == Some(id as i64))
        .unwrap_or_else(|| panic!("expected link {id} in {links:?}"))
}

/// A user with several links must get each link back with exactly its tags,
/// including a link that has none and a tag that is attached to none.
#[tokio::test]
async fn list_returns_each_link_with_exactly_its_tags() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (_other_token, other_id) = register_verified(&server, &db).await;

    let tagged = insert_link(&db, user_id, "https://iana.org/tagged").await;
    let untagged = insert_link(&db, user_id, "https://iana.org/untagged").await;
    let shared = insert_link(&db, user_id, "https://iana.org/shared").await;
    let other_link = insert_link(&db, other_id, "https://iana.org/foreign").await;

    let alpha = insert_tag(&db, user_id, "alpha").await;
    let beta = insert_tag(&db, user_id, "beta").await;
    let _orphan = insert_tag(&db, user_id, "orphan").await;
    let foreign_tag = insert_tag(&db, other_id, "foreign-tag").await;

    attach_tag(&db, tagged, alpha).await;
    attach_tag(&db, tagged, beta).await;
    attach_tag(&db, shared, alpha).await;
    attach_tag(&db, other_link, foreign_tag).await;

    let listed: Value = server.get("/links").authorization_bearer(&token).await.json();
    let links = listed.as_array().expect("array");
    let ids: Vec<i64> = links.iter().filter_map(|l| l["id"].as_i64()).collect();
    assert_eq!(
        ids.len(),
        3,
        "must list only this user's three live links, got {ids:?}"
    );
    assert!(ids.contains(&(tagged as i64)) && ids.contains(&(untagged as i64)) && ids.contains(&(shared as i64)));
    assert!(!ids.contains(&(other_link as i64)));

    assert_eq!(
        tag_ids_for(find_link(links, tagged)),
        vec![alpha as i64, beta as i64]
    );
    assert_eq!(tag_ids_for(find_link(links, untagged)), vec![] as Vec<i64>);
    assert_eq!(tag_ids_for(find_link(links, shared)), vec![alpha as i64]);
}

/// Soft-deleted links stay out of the list even when they still have tag rows.
#[tokio::test]
async fn list_excludes_soft_deleted_links_and_their_tags() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    let live = insert_link(&db, user_id, "https://iana.org/live").await;
    let gone = insert_link(&db, user_id, "https://iana.org/gone").await;
    let tag = insert_tag(&db, user_id, "keep").await;
    attach_tag(&db, live, tag).await;
    attach_tag(&db, gone, tag).await;

    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [gone] }))
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    let listed: Value = server.get("/links").authorization_bearer(&token).await.json();
    let links = listed.as_array().expect("array");
    let ids: Vec<i64> = links.iter().filter_map(|l| l["id"].as_i64()).collect();
    assert_eq!(ids, vec![live as i64], "soft-deleted link must not appear");
    assert_eq!(tag_ids_for(&links[0]), vec![tag as i64]);
}

/// `limit` is clamped to 1..=1000. `limit=0` used to mean LIMIT 0 (empty page);
/// omitted `limit` used to mean every live row. Both must now return a bounded
/// page. Dashboard fetches with no `limit` and paginates in the browser, so the
/// default is 1000 rather than 25.
#[tokio::test]
async fn list_clamps_limit_and_honours_offset() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;

    for i in 0..5 {
        insert_link(&db, user_id, &format!("https://iana.org/p{i}")).await;
    }

    let limited: Value = server
        .get("/links?limit=2")
        .authorization_bearer(&token)
        .await
        .json();
    assert_eq!(
        limited.as_array().map(|a| a.len()),
        Some(2),
        "limit=2 must return two rows: {limited}"
    );

    let zero: Value = server
        .get("/links?limit=0")
        .authorization_bearer(&token)
        .await
        .json();
    assert_eq!(
        zero.as_array().map(|a| a.len()),
        Some(5),
        "limit=0 must fall back to the default (all five, under the cap), got {zero}"
    );

    let huge: Value = server
        .get("/links?limit=999999")
        .authorization_bearer(&token)
        .await
        .json();
    assert_eq!(
        huge.as_array().map(|a| a.len()),
        Some(5),
        "a huge limit must clamp, not error: {huge}"
    );

    let page: Value = server
        .get("/links?limit=2&offset=2")
        .authorization_bearer(&token)
        .await
        .json();
    let page_ids: Vec<i64> = page
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["id"].as_i64())
        .collect();
    let all_ids: Vec<i64> = huge
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["id"].as_i64())
        .collect();
    assert_eq!(
        page_ids,
        all_ids[2..4],
        "offset=2 limit=2 must be the third and fourth newest"
    );
}

#[tokio::test]
async fn openapi_documents_links_limit_default_and_max() {
    let (server, _db) = spawn_real_app().await;
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    let params = spec["paths"]["/links"]["get"]["parameters"]
        .as_array()
        .expect("GET /links parameters");
    let limit = params
        .iter()
        .find(|p| p["name"] == "limit")
        .expect("limit param");
    let description = limit["description"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase();
    assert!(
        description.contains("1000")
            && (description.contains("default") || description.contains("omit")),
        "limit must document the default and the 1000 maximum, got {limit}"
    );
}
