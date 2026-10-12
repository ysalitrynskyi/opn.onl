//! A link id that does not exist answers like one the caller may not touch.
//!
//! Reads already refused a missing, a foreign and a deleted link with the same
//! 403, so ids could not be probed. Update and delete answered 404 for a link
//! that was not there and 403 for one that belonged to someone else, which
//! told a caller which ids exist. All three now go through `live_link_for`.
//! Real router, real Postgres.

mod common;

use common::{spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{links, users};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

/// An id no sequence will reach.
const MISSING_ID: i32 = i32::MAX;

/// A fresh app for one request: the mock transport puts every caller in one
/// per-second bucket, so a test making several calls takes a new app for each.
async fn app() -> axum_test::TestServer {
    spawn_real_app().await.0
}

struct Person {
    id: i32,
    token: String,
}

async fn person(db: &DatabaseConnection) -> Person {
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
    let token = create_jwt(user.id, &email, user.token_version).expect("mint token");
    Person { id: user.id, token }
}

async fn insert_link(db: &DatabaseConnection, owner: i32, deleted: bool) -> links::Model {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set("https://iana.org/original".to_string()),
        user_id: Set(Some(owner)),
        deleted_at: Set(deleted.then(|| chrono::Utc::now().naive_utc())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
}

/// `(status, body)` of one call to `method /links/{id}` as `who`.
async fn call(method: &str, id: i32, who: &Person) -> (u16, Value) {
    let server = app().await;
    let url = format!("/links/{id}");
    let res = match method {
        "GET" => server.get(&url).authorization_bearer(&who.token).await,
        "PUT" => {
            server
                .put(&url)
                .authorization_bearer(&who.token)
                .json(&json!({ "title": "renamed" }))
                .await
        }
        "DELETE" => server.delete(&url).authorization_bearer(&who.token).await,
        other => panic!("unexpected method {other}"),
    };
    (res.status_code().as_u16(), res.json::<Value>())
}

#[tokio::test]
async fn update_and_delete_answer_a_missing_link_like_a_foreign_one() {
    let (_, db) = spawn_real_app().await;
    let caller = person(&db).await;
    let stranger = person(&db).await;
    let foreign = insert_link(&db, stranger.id, false).await;

    for method in ["PUT", "DELETE"] {
        let missing = call(method, MISSING_ID, &caller).await;
        let not_yours = call(method, foreign.id, &caller).await;
        assert_eq!(missing.0, 403, "{method} of a missing link: {}", missing.1);
        assert_eq!(
            missing, not_yours,
            "{method}: a missing link and someone else's must be indistinguishable"
        );
    }

    // The foreign link is untouched.
    let row = links::Entity::find_by_id(foreign.id)
        .one(&db)
        .await
        .unwrap()
        .expect("row");
    assert!(row.deleted_at.is_none());
    assert_eq!(row.title, None);
}

#[tokio::test]
async fn update_and_delete_answer_a_deleted_link_like_a_foreign_one() {
    let (_, db) = spawn_real_app().await;
    let caller = person(&db).await;
    let gone = insert_link(&db, caller.id, true).await;

    for method in ["GET", "PUT", "DELETE"] {
        let (status, body) = call(method, gone.id, &caller).await;
        assert_eq!(status, 403, "{method} of the caller's deleted link: {body}");
    }
}

/// Controls: the owner of a live link still updates and deletes it.
#[tokio::test]
async fn an_owner_still_updates_and_deletes_a_live_link() {
    let (_, db) = spawn_real_app().await;
    let owner = person(&db).await;
    let link = insert_link(&db, owner.id, false).await;

    let (status, body) = call("PUT", link.id, &owner).await;
    assert_eq!(status, 200, "update: {body}");
    assert_eq!(body["title"], "renamed");

    let (status, body) = call("DELETE", link.id, &owner).await;
    assert_eq!(status, 200, "delete: {body}");
    let row = links::Entity::find_by_id(link.id)
        .one(&db)
        .await
        .unwrap()
        .expect("soft delete keeps the row");
    assert!(row.deleted_at.is_some());
}
