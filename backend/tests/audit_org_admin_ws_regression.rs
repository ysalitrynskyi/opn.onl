//! Regression coverage for organization resource authorization and account
//! deletion/restore invariants. Real router + real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{
    api_keys, folders, link_tags, links, org_members, passkeys, tags, users,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{json, Value};
use std::time::Duration;

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

async fn make_admin(db: &DatabaseConnection, user_id: i32) {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.unwrap();
}

async fn create_org(server: &axum_test::TestServer, token: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({
            "name": "Audit Org",
            "slug": format!("audit-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn add_member(db: &DatabaseConnection, org_id: i32, user_id: i32, role: &str) -> i32 {
    org_members::ActiveModel {
        org_id: Set(org_id),
        user_id: Set(user_id),
        role: Set(role.to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("add org member")
    .id
}

async fn create_link(server: &axum_test::TestServer, token: &str, org_id: Option<i32>) -> i32 {
    let mut payload = json!({ "original_url": "https://iana.org/audit" });
    if let Some(org_id) = org_id {
        payload["org_id"] = json!(org_id);
    }
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn create_folder(server: &axum_test::TestServer, token: &str, org_id: i32) -> i32 {
    let res = server
        .post("/folders")
        .authorization_bearer(token)
        .json(&json!({ "name": "Audit Folder", "org_id": org_id }))
        .await;
    assert_eq!(res.status_code(), 201, "create folder: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn create_tag(
    server: &axum_test::TestServer,
    token: &str,
    org_id: Option<i32>,
    name: &str,
) -> i32 {
    let mut payload = json!({ "name": name });
    if let Some(org_id) = org_id {
        payload["org_id"] = json!(org_id);
    }
    let res = server
        .post("/tags")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert_eq!(res.status_code(), 201, "create tag: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn seed_credentials(db: &DatabaseConnection, user_id: i32) {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    api_keys::ActiveModel {
        user_id: Set(user_id),
        name: Set("audit key".to_string()),
        key_hash: Set(format!("hash-{nonce}")),
        key_prefix: Set("opn_audit".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert API key");

    passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(format!("cred-{nonce}")),
        cred_public_key: Set("public-key".to_string()),
        counter: Set(0),
        name: Set(Some("audit passkey".to_string())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert passkey");
}

#[tokio::test]
async fn removed_org_creator_cannot_mutate_folder_or_tag_state() {
    let (setup_server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&setup_server, &db).await;
    let (creator_token, creator_id) = register_verified(&setup_server, &db).await;
    let org_id = create_org(&setup_server, &owner_token).await;
    let member_id = add_member(&db, org_id, creator_id, "editor").await;

    let folder_id = create_folder(&setup_server, &creator_token, org_id).await;
    let linked_tag_id = create_tag(&setup_server, &creator_token, Some(org_id), "linked-tag").await;
    let new_tag_id = create_tag(&setup_server, &creator_token, Some(org_id), "new-tag").await;
    let link_id = create_link(&setup_server, &creator_token, Some(org_id)).await;

    // Simulate legacy rows that retained their creator as well as org ownership.
    let folder = folders::Entity::find_by_id(folder_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active_folder: folders::ActiveModel = folder.into();
    active_folder.user_id = Set(Some(creator_id));
    active_folder.update(&db).await.unwrap();

    for tag_id in [linked_tag_id, new_tag_id] {
        let tag = tags::Entity::find_by_id(tag_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut active_tag: tags::ActiveModel = tag.into();
        active_tag.user_id = Set(Some(creator_id));
        active_tag.update(&db).await.unwrap();
    }

    link_tags::ActiveModel {
        link_id: Set(link_id),
        tag_id: Set(linked_tag_id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    org_members::Entity::delete_by_id(member_id)
        .exec(&db)
        .await
        .unwrap();

    // Fresh rate-limiters; credentials remain valid against the shared DB.
    let (server, _) = spawn_real_app().await;
    for path in [
        format!("/folders/{folder_id}"),
        format!("/tags/{linked_tag_id}"),
    ] {
        let res = server.get(&path).authorization_bearer(&creator_token).await;
        assert_eq!(
            res.status_code(),
            403,
            "removed creator read org resource {path}: {}",
            res.text()
        );
    }

    let moved = server
        .post(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&creator_token)
        .json(&json!({ "link_ids": [link_id] }))
        .await;
    assert_eq!(
        moved.status_code(),
        403,
        "removed creator moved link: {}",
        moved.text()
    );

    let added = server
        .post(&format!("/links/{link_id}/tags"))
        .authorization_bearer(&creator_token)
        .json(&json!({ "tag_ids": [new_tag_id] }))
        .await;
    assert_eq!(
        added.status_code(),
        403,
        "removed creator added tag: {}",
        added.text()
    );

    let removed = server
        .delete(&format!("/links/{link_id}/tags"))
        .authorization_bearer(&creator_token)
        .json(&json!({ "tag_ids": [linked_tag_id] }))
        .await;
    assert_eq!(
        removed.status_code(),
        403,
        "removed creator removed tag: {}",
        removed.text()
    );

    let link = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.folder_id, None);
    assert!(
        link_tags::Entity::find()
            .filter(link_tags::Column::LinkId.eq(link_id))
            .filter(link_tags::Column::TagId.eq(linked_tag_id))
            .one(&db)
            .await
            .unwrap()
            .is_some(),
        "removed creator must not remove org tag assignment"
    );
    assert!(
        link_tags::Entity::find()
            .filter(link_tags::Column::LinkId.eq(link_id))
            .filter(link_tags::Column::TagId.eq(new_tag_id))
            .one(&db)
            .await
            .unwrap()
            .is_none(),
        "removed creator must not add org tag assignment"
    );

    // Current membership alone is insufficient: viewers can read shared
    // resources but cannot mutate folders, tags, or tag assignments.
    let viewer_member_id = add_member(&db, org_id, creator_id, "viewer").await;
    let (server, _) = spawn_real_app().await;
    for (path, payload) in [
        (
            format!("/folders/{folder_id}"),
            json!({ "name": "Viewer edit" }),
        ),
        (
            format!("/tags/{linked_tag_id}"),
            json!({ "name": "Viewer edit" }),
        ),
    ] {
        let res = server
            .put(&path)
            .authorization_bearer(&creator_token)
            .json(&payload)
            .await;
        assert_eq!(
            res.status_code(),
            403,
            "viewer mutated {path}: {}",
            res.text()
        );
    }
    let res = server
        .post(&format!("/folders/{folder_id}/links"))
        .authorization_bearer(&creator_token)
        .json(&json!({ "link_ids": [link_id] }))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "viewer moved org link: {}",
        res.text()
    );
    let res = server
        .post(&format!("/links/{link_id}/tags"))
        .authorization_bearer(&creator_token)
        .json(&json!({ "tag_ids": [new_tag_id] }))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "viewer tagged org link: {}",
        res.text()
    );
    org_members::Entity::delete_by_id(viewer_member_id)
        .exec(&db)
        .await
        .unwrap();

    // Unknown roles are denied by the entity allowlist, not treated as editors.
    add_member(&db, org_id, creator_id, "future-role").await;
    let (server, _) = spawn_real_app().await;
    let res = server
        .post("/folders")
        .authorization_bearer(&creator_token)
        .json(&json!({ "name": "Nope", "org_id": org_id }))
        .await;
    assert_eq!(res.status_code(), 403, "unknown role must be read-only");
}

#[tokio::test]
async fn personal_tag_cannot_be_attached_to_another_users_link() {
    let (server, db) = spawn_real_app().await;
    let (victim_token, victim_id) = register_verified(&server, &db).await;
    let (attacker_token, attacker_id) = register_verified(&server, &db).await;
    let victim_tag_id = create_tag(&server, &victim_token, None, "victim-tag").await;
    let attacker_link_id = create_link(&server, &attacker_token, None).await;

    let res = server
        .post(&format!("/links/{attacker_link_id}/tags"))
        .authorization_bearer(&attacker_token)
        .json(&json!({ "tag_ids": [victim_tag_id] }))
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "batch endpoint response: {}",
        res.text()
    );
    assert_eq!(res.json::<Value>()["added"].as_u64(), Some(0));

    assert!(link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.eq(attacker_link_id))
        .filter(link_tags::Column::TagId.eq(victim_tag_id))
        .one(&db)
        .await
        .unwrap()
        .is_none());
    assert_ne!(victim_id, attacker_id);
}

#[tokio::test]
async fn admin_restore_only_reverses_its_personal_link_cascade() {
    let (setup_server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register_verified(&setup_server, &db).await;
    make_admin(&db, admin_id).await;
    let (owner_token, _) = register_verified(&setup_server, &db).await;
    let (target_token, target_id) = register_verified(&setup_server, &db).await;

    let org_id = create_org(&setup_server, &owner_token).await;
    add_member(&db, org_id, target_id, "editor").await;
    let personal_link_id = create_link(&setup_server, &target_token, None).await;
    let prior_takedown_id = create_link(&setup_server, &target_token, None).await;
    let org_link_id = create_link(&setup_server, &target_token, Some(org_id)).await;
    seed_credentials(&db, target_id).await;

    let before = users::Entity::find_by_id(target_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    let (server, _) = spawn_real_app().await;
    let res = server
        .delete(&format!("/admin/links/{prior_takedown_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "pre-delete takedown: {}",
        res.text()
    );
    let prior_deleted_at = links::Entity::find_by_id(prior_takedown_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .deleted_at
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2)).await;

    let res = server
        .delete(&format!("/admin/users/{target_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "delete user: {}", res.text());

    let deleted_user = users::Entity::find_by_id(target_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(deleted_user.deleted_at.is_some());
    assert_eq!(deleted_user.token_version, before + 1);
    assert!(links::Entity::find_by_id(personal_link_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .deleted_at
        .is_some());
    assert!(
        links::Entity::find_by_id(org_link_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none(),
        "admin deletion must preserve org-owned links"
    );
    assert_eq!(
        api_keys::Entity::find()
            .filter(api_keys::Column::UserId.eq(target_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(target_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );

    let res = server
        .post(&format!("/admin/users/{target_id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "restore user: {}", res.text());

    assert!(
        links::Entity::find_by_id(personal_link_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none(),
        "account-deletion cascade should be restored"
    );
    assert_eq!(
        links::Entity::find_by_id(prior_takedown_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .deleted_at,
        Some(prior_deleted_at),
        "pre-existing takedown must stay deleted"
    );

    let res = server
        .get("/auth/me")
        .authorization_bearer(&target_token)
        .await;
    assert_eq!(res.status_code(), 401, "restore must not revive old JWT");
}

#[tokio::test]
async fn self_delete_revokes_credentials_and_preserves_org_links() {
    std::env::set_var("ENABLE_ACCOUNT_DELETION", "true");
    let (setup_server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&setup_server, &db).await;
    let (target_token, target_id) = register_verified(&setup_server, &db).await;
    let org_id = create_org(&setup_server, &owner_token).await;
    add_member(&db, org_id, target_id, "editor").await;
    let personal_link_id = create_link(&setup_server, &target_token, None).await;
    let org_link_id = create_link(&setup_server, &target_token, Some(org_id)).await;
    seed_credentials(&db, target_id).await;

    let before = users::Entity::find_by_id(target_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    let (server, _) = spawn_real_app().await;
    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&target_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "self delete: {}", res.text());

    let user = users::Entity::find_by_id(target_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(user.deleted_at.is_some());
    assert_eq!(user.token_version, before + 1);
    assert!(links::Entity::find_by_id(personal_link_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .deleted_at
        .is_some());
    assert!(
        links::Entity::find_by_id(org_link_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none(),
        "self deletion must preserve org-owned links"
    );
    assert_eq!(
        api_keys::Entity::find()
            .filter(api_keys::Column::UserId.eq(target_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(target_id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
}
