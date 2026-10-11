//! `GET /links/{id}`: one link, in the shape `GET /links` lists it in, behind
//! the same access rule as every other read of a link. Organization roles are
//! covered with the rest of the reads in `org_link_access_tests.rs`. Real
//! router, real Postgres.

mod common;

use common::{spawn_real_app, unique_email};
use opn_onl_backend::entity::users;
use opn_onl_backend::utils::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::{Value, json};

/// A fresh app for one request. Every app has its own rate limiters, and the
/// mock transport puts all callers in one per-second bucket, so a test that
/// makes a dozen calls takes a new app for each rather than tripping it.
async fn app() -> axum_test::TestServer {
    spawn_real_app().await.0
}

/// A verified user and a valid JWT, inserted directly (no HTTP, no bcrypt).
async fn person(db: &DatabaseConnection) -> String {
    let email = unique_email();
    let user = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    create_jwt(user.id, &email, user.token_version).expect("mint token")
}

async fn create_link(token: &str, body: Value) -> Value {
    let res = app()
        .await
        .post("/links")
        .authorization_bearer(token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 201, "create: {}", res.text());
    res.json()
}

async fn get_link(token: &str, id: i64) -> axum_test::TestResponse {
    app()
        .await
        .get(&format!("/links/{id}"))
        .authorization_bearer(token)
        .await
}

async fn list(token: &str) -> Vec<Value> {
    let res = app().await.get("/links").authorization_bearer(token).await;
    assert_eq!(res.status_code(), 200, "list: {}", res.text());
    res.json()
}

#[tokio::test]
async fn a_link_comes_back_exactly_as_the_list_shows_it() {
    let (setup, db) = spawn_real_app().await;
    let token = person(&db).await;

    let tag = setup
        .post("/tags")
        .authorization_bearer(&token)
        .json(&json!({ "name": "launch", "color": "#2563eb" }))
        .await;
    assert_eq!(tag.status_code(), 201, "create tag: {}", tag.text());
    let tag_id = tag.json::<Value>()["id"].as_i64().unwrap();

    // Every optional field set, so a field the single-link handler forgets to
    // map shows up as a difference rather than as a null on both sides.
    let created = create_link(
        &token,
        json!({
            "original_url": "https://iana.org/get-one",
            "custom_alias": format!("g{}", &uuid::Uuid::new_v4().simple().to_string()[..10]),
            "title": "Launch page",
            "notes": "for the spring campaign",
            "password": "secret123",
            "max_clicks": 50,
            "expires_at": "2099-01-01T00:00:00Z",
            "starts_at": "2020-01-01T00:00:00Z",
            "tag_ids": [tag_id],
        }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let res = get_link(&token, id).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let one: Value = res.json();

    let listed = list(&token)
        .await
        .into_iter()
        .find(|l| l["id"].as_i64() == Some(id))
        .expect("the link is in the list");
    assert_eq!(one, listed, "GET /links/{{id}} must match its list item");

    assert_eq!(one["title"], "Launch page");
    assert_eq!(one["notes"], "for the spring campaign");
    assert_eq!(one["has_password"], true);
    assert_eq!(one["max_clicks"], 50);
    assert_eq!(one["is_active"], true);
    assert_eq!(one["tags"][0]["name"], "launch", "tags are loaded: {one}");
    assert!(
        one["created_at"].as_str().unwrap().ends_with('Z'),
        "timestamps are RFC 3339 with a Z: {one}"
    );
    assert!(
        one.get("password_hash").is_none(),
        "the password hash is never part of a link: {one}"
    );
}

#[tokio::test]
async fn a_link_that_is_not_live_yet_still_reads_back_as_inactive() {
    let (_setup, db) = spawn_real_app().await;
    let token = person(&db).await;
    let created = create_link(
        &token,
        json!({
            "original_url": "https://iana.org/not-yet",
            "starts_at": "2099-01-01T00:00:00Z",
        }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let res = get_link(&token, id).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["is_active"], false);
}

#[tokio::test]
async fn reading_a_link_needs_a_credential_that_works() {
    let (setup, db) = spawn_real_app().await;
    let token = person(&db).await;
    let id = create_link(&token, json!({ "original_url": "https://iana.org/auth" })).await["id"]
        .as_i64()
        .unwrap();

    let anonymous = app().await.get(&format!("/links/{id}")).await;
    assert_eq!(anonymous.status_code(), 401, "{}", anonymous.text());

    let garbage = get_link("not-a-token", id).await;
    assert_eq!(garbage.status_code(), 401, "{}", garbage.text());

    // An API key reads it too: the operation is documented for both schemes.
    let key = setup
        .post("/auth/api-keys")
        .authorization_bearer(&token)
        .json(&json!({ "name": "get one link" }))
        .await;
    assert_eq!(key.status_code(), 201, "create key: {}", key.text());
    let key = key.json::<Value>()["key"].as_str().unwrap().to_string();
    let res = get_link(&key, id).await;
    assert_eq!(res.status_code(), 200, "API key: {}", res.text());
    assert_eq!(res.json::<Value>()["id"].as_i64(), Some(id));
}

#[tokio::test]
async fn someone_elses_missing_and_deleted_links_are_one_and_the_same_403() {
    let (_setup, db) = spawn_real_app().await;
    let owner = person(&db).await;
    let stranger = person(&db).await;
    let id = create_link(
        &owner,
        json!({ "original_url": "https://iana.org/private" }),
    )
    .await["id"]
        .as_i64()
        .unwrap();

    // Control: the owner reads it.
    assert_eq!(get_link(&owner, id).await.status_code(), 200);

    let foreign = get_link(&stranger, id).await;
    assert_eq!(foreign.status_code(), 403, "{}", foreign.text());

    let missing = get_link(&stranger, i64::from(i32::MAX)).await;
    assert_eq!(missing.status_code(), 403, "{}", missing.text());
    assert_eq!(
        foreign.text(),
        missing.text(),
        "an id that exists and one that does not must be indistinguishable"
    );

    let deleted = app()
        .await
        .delete(&format!("/links/{id}"))
        .authorization_bearer(&owner)
        .await;
    assert_eq!(deleted.status_code(), 200, "delete: {}", deleted.text());

    // Even its owner can no longer read a deleted link, and the answer is the
    // one a stranger gets for an id that never existed.
    let after = get_link(&owner, id).await;
    assert_eq!(after.status_code(), 403, "{}", after.text());
    assert_eq!(after.text(), missing.text());
}
