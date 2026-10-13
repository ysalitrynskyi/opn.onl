//! A link id that does not exist answers like one the caller may not touch.
//!
//! Reads already refused a missing, a foreign and a deleted link with the same
//! 403, so ids could not be probed. Update, delete, QR, clone, pin, stats and
//! the realtime click count answered 404 for a link that was not there and 403
//! for one that belonged to someone else, which told a caller which ids exist.
//! All of them now go through `live_link_for`. Real router, real Postgres.

mod common;

use common::{spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{links, users};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
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

/// The endpoints that take a link id in the path and are not covered by
/// `call`: QR image, clone, pin, stats and the realtime click count.
fn by_id_endpoints(id: i32) -> [(&'static str, String); 5] {
    [
        ("GET", format!("/links/{id}/qr")),
        ("POST", format!("/links/{id}/clone")),
        ("POST", format!("/links/{id}/pin")),
        ("GET", format!("/links/{id}/stats")),
        ("GET", format!("/links/{id}/clicks/realtime")),
    ]
}

/// `(status, raw body)` of one call to `method path` as `who`.
async fn raw(method: &str, path: &str, who: &Person) -> (u16, String) {
    let server = app().await;
    let res = match method {
        "GET" => server.get(path).authorization_bearer(&who.token).await,
        "POST" => server.post(path).authorization_bearer(&who.token).await,
        other => panic!("unexpected method {other}"),
    };
    (res.status_code().as_u16(), res.text())
}

#[tokio::test]
async fn qr_clone_pin_and_analytics_answer_a_missing_link_like_a_foreign_one() {
    let (_, db) = spawn_real_app().await;
    let caller = person(&db).await;
    let stranger = person(&db).await;
    let foreign = insert_link(&db, stranger.id, false).await;

    for ((method, missing_path), (_, foreign_path)) in by_id_endpoints(MISSING_ID)
        .into_iter()
        .zip(by_id_endpoints(foreign.id))
    {
        let missing = raw(method, &missing_path, &caller).await;
        let not_yours = raw(method, &foreign_path, &caller).await;
        assert_eq!(missing.0, 403, "{method} {missing_path}: {}", missing.1);
        assert_eq!(
            missing, not_yours,
            "{method} {missing_path}: a missing link and someone else's must be indistinguishable"
        );
    }

    // The foreign link was neither pinned nor cloned.
    let row = links::Entity::find_by_id(foreign.id)
        .one(&db)
        .await
        .unwrap()
        .expect("row");
    assert!(!row.is_pinned);
    let copies = links::Entity::find()
        .filter(links::Column::OriginalUrl.eq(foreign.original_url.clone()))
        .filter(links::Column::UserId.eq(caller.id))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(copies, 0, "a refused clone must not create a link");
}

#[tokio::test]
async fn qr_clone_pin_and_analytics_answer_a_deleted_link_like_a_foreign_one() {
    let (_, db) = spawn_real_app().await;
    let caller = person(&db).await;
    let stranger = person(&db).await;
    let foreign = insert_link(&db, stranger.id, false).await;
    let gone = insert_link(&db, caller.id, true).await;

    for ((method, gone_path), (_, foreign_path)) in by_id_endpoints(gone.id)
        .into_iter()
        .zip(by_id_endpoints(foreign.id))
    {
        let deleted = raw(method, &gone_path, &caller).await;
        let not_yours = raw(method, &foreign_path, &caller).await;
        assert_eq!(deleted.0, 403, "{method} {gone_path}: {}", deleted.1);
        assert_eq!(
            deleted, not_yours,
            "{method} {gone_path}: a deleted link and someone else's must be indistinguishable"
        );
    }
}

/// Controls: the owner of a live link still gets every one of them.
#[tokio::test]
async fn an_owner_still_reaches_qr_clone_pin_and_analytics_of_a_live_link() {
    let (_, db) = spawn_real_app().await;
    let owner = person(&db).await;
    let link = insert_link(&db, owner.id, false).await;

    for (method, path) in by_id_endpoints(link.id) {
        let (status, body) = raw(method, &path, &owner).await;
        let expected = if path.ends_with("/clone") { 201 } else { 200 };
        assert_eq!(status, expected, "{method} {path}: {body}");
    }

    let row = links::Entity::find_by_id(link.id)
        .one(&db)
        .await
        .unwrap()
        .expect("row");
    assert!(row.is_pinned, "the pin call toggled the link on");
    let copies = links::Entity::find()
        .filter(links::Column::OriginalUrl.eq(link.original_url.clone()))
        .filter(links::Column::UserId.eq(owner.id))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(copies, 2, "the original and its clone");
}

/// `(status, raw body)` of attaching (`POST`) or detaching (`DELETE`) tags on
/// link `id` as `who`.
async fn tags_call(method: &str, id: i32, who: &Person) -> (u16, String) {
    let server = app().await;
    let path = format!("/links/{id}/tags");
    let body = json!({ "tag_ids": [] });
    let res = match method {
        "POST" => {
            server
                .post(&path)
                .authorization_bearer(&who.token)
                .json(&body)
                .await
        }
        "DELETE" => {
            server
                .delete(&path)
                .authorization_bearer(&who.token)
                .json(&body)
                .await
        }
        other => panic!("unexpected method {other}"),
    };
    (res.status_code().as_u16(), res.text())
}

#[tokio::test]
async fn link_tags_answer_a_missing_or_deleted_link_like_a_foreign_one() {
    let (_, db) = spawn_real_app().await;
    let caller = person(&db).await;
    let stranger = person(&db).await;
    let foreign = insert_link(&db, stranger.id, false).await;
    let gone = insert_link(&db, caller.id, true).await;

    for method in ["POST", "DELETE"] {
        let not_yours = tags_call(method, foreign.id, &caller).await;
        assert_eq!(not_yours.0, 403, "{method} foreign: {}", not_yours.1);
        assert_eq!(
            tags_call(method, MISSING_ID, &caller).await,
            not_yours,
            "{method}: a missing link and someone else's must be indistinguishable"
        );
        assert_eq!(
            tags_call(method, gone.id, &caller).await,
            not_yours,
            "{method}: a deleted link and someone else's must be indistinguishable"
        );
    }

    // Control: the owner of a live link still gets through.
    let own = insert_link(&db, caller.id, false).await;
    for method in ["POST", "DELETE"] {
        let (status, body) = tags_call(method, own.id, &caller).await;
        assert_eq!(status, 200, "{method} own link: {body}");
    }
}
