//! Id lists in request bodies: capped at 500 like `/links/bulk/*`, deduplicated,
//! and answered with the number of rows actually changed.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{link_tags, links, org_members};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DbBackend, EntityTrait, PaginatorTrait, QueryFilter, Statement,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------- helpers

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

async fn create_org(server: &axum_test::TestServer, token: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({
            "name": "Id List Org",
            "slug": format!("ids-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
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

async fn create_folder(server: &axum_test::TestServer, token: &str, org_id: Option<i32>) -> i32 {
    let mut payload = json!({ "name": "Id List Folder" });
    if let Some(org_id) = org_id {
        payload["org_id"] = json!(org_id);
    }
    let res = server
        .post("/folders")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create folder: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn create_tag(server: &axum_test::TestServer, token: &str, name: &str) -> i32 {
    let res = server
        .post("/tags")
        .authorization_bearer(token)
        .json(&json!({ "name": name }))
        .await;
    assert_eq!(res.status_code(), 201, "create tag: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn insert_link(db: &DatabaseConnection, user_id: i32, org_id: Option<i32>) -> i32 {
    links::ActiveModel {
        code: Set(format!("il{}", uuid::Uuid::new_v4().simple())),
        original_url: Set("https://iana.org/id-list".to_string()),
        user_id: Set(Some(user_id)),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
    .id
}

async fn folder_of(db: &DatabaseConnection, link_id: i32) -> Option<i32> {
    links::Entity::find_by_id(link_id)
        .one(db)
        .await
        .unwrap()
        .expect("link row")
        .folder_id
}

async fn is_live(db: &DatabaseConnection, link_id: i32) -> bool {
    links::Entity::find_by_id(link_id)
        .one(db)
        .await
        .unwrap()
        .expect("link row")
        .deleted_at
        .is_none()
}

async fn tag_assignments(db: &DatabaseConnection, link_id: i32) -> u64 {
    link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.eq(link_id))
        .count(db)
        .await
        .unwrap()
}

async fn sql(db: &DatabaseConnection, stmt: String) {
    db.execute(Statement::from_string(DbBackend::Postgres, stmt))
        .await
        .expect("raw sql");
}

fn error_text(body: &Value) -> String {
    body["error"]
        .as_str()
        .or_else(|| body["message"].as_str())
        .unwrap_or_default()
        .to_string()
}

// ----------------------------------------------------------------- folders

#[tokio::test]
async fn moving_links_counts_each_distinct_link_that_actually_moved() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (_, other_id) = register_verified(&server, &db).await;
    let folder_id = create_folder(&server, &token, None).await;

    let mine_a = insert_link(&db, user_id, None).await;
    let mine_b = insert_link(&db, user_id, None).await;
    let someone_elses = insert_link(&db, other_id, None).await;
    let deleted = insert_link(&db, user_id, None).await;
    sql(
        &db,
        format!("UPDATE links SET deleted_at = now() WHERE id = {deleted}"),
    )
    .await;

    let res = server
        .post(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .json(&json!({
            "link_ids": [
                mine_a, mine_a, mine_a, mine_b, someone_elses, deleted, 2_000_000_000
            ]
        }))
        .await;
    assert_eq!(res.status_code(), 200, "move: {}", res.text());
    assert_eq!(
        res.json::<Value>()["moved"],
        2,
        "two distinct links of the caller moved; repeats, another user's link, a \
         deleted link and an unknown id do not count"
    );
    assert_eq!(folder_of(&db, mine_a).await, Some(folder_id));
    assert_eq!(folder_of(&db, mine_b).await, Some(folder_id));
    assert_eq!(folder_of(&db, someone_elses).await, None);
    assert_eq!(folder_of(&db, deleted).await, None);
}

#[tokio::test]
async fn moving_links_accepts_500_ids_and_rejects_501() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let folder_id = create_folder(&server, &token, None).await;
    let link_id = insert_link(&db, user_id, None).await;

    // Over the cap even though every entry is the same id.
    let too_many = server
        .post(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .json(&json!({ "link_ids": vec![link_id; 501] }))
        .await;
    assert_eq!(too_many.status_code(), 400, "501 ids: {}", too_many.text());
    assert!(
        error_text(&too_many.json::<Value>()).contains("max 500"),
        "{}",
        too_many.text()
    );
    assert_eq!(folder_of(&db, link_id).await, None, "nothing may move");

    // CONTROL: exactly 500 is fine.
    let at_the_cap = server
        .post(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&token)
        .json(&json!({ "link_ids": vec![link_id; 500] }))
        .await;
    assert_eq!(at_the_cap.status_code(), 200, "{}", at_the_cap.text());
    assert_eq!(at_the_cap.json::<Value>()["moved"], 1);
    assert_eq!(folder_of(&db, link_id).await, Some(folder_id));
}

#[tokio::test]
async fn moving_links_never_crosses_the_personal_organization_boundary() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id) = register_verified(&server, &db).await;
    let (editor_token, editor_id) = register_verified(&server, &db).await;
    let org_id = create_org(&server, &owner_token).await;
    add_member(&db, org_id, editor_id, "editor").await;

    let org_folder = create_folder(&server, &editor_token, Some(org_id)).await;
    let personal_folder = create_folder(&server, &editor_token, None).await;
    let org_link_by_owner = insert_link(&db, owner_id, Some(org_id)).await;
    let org_link_by_editor = insert_link(&db, editor_id, Some(org_id)).await;
    let personal_link = insert_link(&db, editor_id, None).await;

    // Org folder: any org link (whoever made it) moves, a personal link does not.
    let into_org = server
        .post(&format!("/folders/{org_folder}/links"))
        .authorization_bearer(&editor_token)
        .json(&json!({ "link_ids": [org_link_by_owner, org_link_by_editor, personal_link] }))
        .await;
    assert_eq!(into_org.status_code(), 200, "{}", into_org.text());
    assert_eq!(into_org.json::<Value>()["moved"], 2);
    assert_eq!(folder_of(&db, org_link_by_owner).await, Some(org_folder));
    assert_eq!(folder_of(&db, org_link_by_editor).await, Some(org_folder));
    assert_eq!(folder_of(&db, personal_link).await, None);

    // Personal folder: the editor's personal link moves, org links do not.
    let into_personal = server
        .post(&format!("/folders/{personal_folder}/links"))
        .authorization_bearer(&editor_token)
        .json(&json!({ "link_ids": [personal_link, org_link_by_editor] }))
        .await;
    assert_eq!(into_personal.status_code(), 200, "{}", into_personal.text());
    assert_eq!(into_personal.json::<Value>()["moved"], 1);
    assert_eq!(folder_of(&db, personal_link).await, Some(personal_folder));
    assert_eq!(folder_of(&db, org_link_by_editor).await, Some(org_folder));
}

// -------------------------------------------------------------------- tags

#[tokio::test]
async fn attaching_tags_counts_each_distinct_tag_and_takes_the_whole_list() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id, None).await;
    let tag_id = create_tag(&server, &token, "id-list-attach").await;
    let url = format!("/links/{link_id}/tags");

    // The real tag is far past the 50th entry, which an earlier version of this
    // endpoint silently dropped.
    let mut ids: Vec<i32> = (1..=299).map(|n| 2_000_000_000 + n).collect();
    ids.push(tag_id);
    ids.extend([tag_id, tag_id]);
    let res = server
        .post(&url)
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": ids }))
        .await;
    assert_eq!(res.status_code(), 200, "attach: {}", res.text());
    assert_eq!(res.json::<Value>()["added"], 1);
    assert_eq!(tag_assignments(&db, link_id).await, 1);

    // Attaching it again adds nothing.
    let again = server
        .post(&url)
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": [tag_id, tag_id] }))
        .await;
    assert_eq!(again.status_code(), 200);
    assert_eq!(again.json::<Value>()["added"], 0);
}

#[tokio::test]
async fn attaching_and_removing_tags_reject_more_than_500_ids() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id, None).await;
    let tag_id = create_tag(&server, &token, "id-list-cap").await;
    let url = format!("/links/{link_id}/tags");

    let attach = server
        .post(&url)
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": vec![tag_id; 501] }))
        .await;
    assert_eq!(attach.status_code(), 400, "attach 501: {}", attach.text());
    assert_eq!(tag_assignments(&db, link_id).await, 0, "nothing attached");

    // CONTROL: exactly 500 attaches.
    let attach = server
        .post(&url)
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": vec![tag_id; 500] }))
        .await;
    assert_eq!(attach.status_code(), 200, "attach 500: {}", attach.text());
    assert_eq!(attach.json::<Value>()["added"], 1);

    let remove = server
        .delete(&url)
        .authorization_bearer(&token)
        .json(&json!({ "tag_ids": vec![tag_id; 501] }))
        .await;
    assert_eq!(remove.status_code(), 400, "remove 501: {}", remove.text());
    assert_eq!(tag_assignments(&db, link_id).await, 1, "nothing detached");
}

#[tokio::test]
async fn removing_tags_counts_only_assignments_that_were_deleted() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (other_token, _) = register_verified(&server, &db).await;
    let link_id = insert_link(&db, user_id, None).await;
    let kept = create_tag(&server, &token, "id-list-kept").await;
    let dropped = create_tag(&server, &token, "id-list-dropped").await;
    let unattached = create_tag(&server, &token, "id-list-unattached").await;
    let foreign = create_tag(&server, &other_token, "id-list-foreign").await;
    let url = format!("/links/{link_id}/tags");

    for tag_id in [kept, dropped, foreign] {
        link_tags::ActiveModel {
            link_id: Set(link_id),
            tag_id: Set(tag_id),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    let res = server
        .delete(&url)
        .authorization_bearer(&token)
        .json(&json!({
            "tag_ids": [dropped, dropped, dropped, unattached, foreign, 2_000_000_000]
        }))
        .await;
    assert_eq!(res.status_code(), 200, "remove: {}", res.text());
    assert_eq!(
        res.json::<Value>()["removed"],
        1,
        "one assignment deleted; repeats, an unattached tag, an unknown id and \
         another user's tag do not count"
    );
    let left: Vec<i32> = link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.eq(link_id))
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|lt| lt.tag_id)
        .collect();
    assert!(left.contains(&kept), "an unlisted tag stays attached");
    assert!(
        left.contains(&foreign),
        "a tag out of scope is never touched"
    );
    assert!(!left.contains(&dropped));
}

// ------------------------------------------------------------------- admin

#[tokio::test]
async fn admin_bulk_link_actions_reject_more_than_500_ids() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register_verified(&server, &db).await;
    sql(
        &db,
        format!("UPDATE users SET is_admin = TRUE WHERE id = {admin_id}"),
    )
    .await;
    let link_id = insert_link(&db, admin_id, None).await;
    for action in ["delete", "restore"] {
        let res = server
            .post(&format!("/admin/links/bulk/{action}"))
            .authorization_bearer(&admin_token)
            .json(&json!({ "ids": vec![link_id; 501] }))
            .await;
        assert_eq!(res.status_code(), 400, "{action} 501: {}", res.text());
        assert!(
            is_live(&db, link_id).await,
            "{action}: the link must be untouched"
        );
    }

    // CONTROL: 500 ids is fine, and repeats still count the row once.
    let res = server
        .post("/admin/links/bulk/delete")
        .authorization_bearer(&admin_token)
        .json(&json!({ "ids": vec![link_id; 500] }))
        .await;
    assert_eq!(res.status_code(), 200, "delete 500: {}", res.text());
    assert_eq!(res.json::<Value>()["affected"], 1);
    assert!(!is_live(&db, link_id).await);
}
