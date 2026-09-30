//! Regression coverage for organization resource authorization and account
//! deletion/restore invariants. Real router + real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::{
    api_keys, click_events, folders, link_tags, links, org_members, organizations, passkeys, tags,
    users,
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

/// Hard-deleting an editor must not destroy links they created inside someone
/// else's organization (or those links' click history). Organization links
/// belong to the team; only the editor's personal links die with the account.
#[tokio::test]
async fn admin_hard_delete_preserves_other_orgs_links_and_clicks() {
    let (setup_server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register_verified(&setup_server, &db).await;
    make_admin(&db, admin_id).await;
    let (owner_token, owner_id) = register_verified(&setup_server, &db).await;
    let (editor_token, editor_id) = register_verified(&setup_server, &db).await;

    let org_id = create_org(&setup_server, &owner_token).await;
    add_member(&db, org_id, editor_id, "editor").await;
    let personal_link_id = create_link(&setup_server, &editor_token, None).await;
    let org_link_id = create_link(&setup_server, &editor_token, Some(org_id)).await;

    let org_click = click_events::ActiveModel {
        link_id: Set(org_link_id),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ip_address: Set(Some("203.0.113.10".to_string())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert org click");
    click_events::ActiveModel {
        link_id: Set(personal_link_id),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ip_address: Set(Some("203.0.113.11".to_string())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert personal click");

    let org_link_before = links::Entity::find_by_id(org_link_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let org_code = org_link_before.code.clone();

    let (server, _) = spawn_real_app().await;
    let res = server
        .delete(&format!("/admin/users/{editor_id}/hard"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "hard delete: {}", res.text());

    assert!(
        users::Entity::find_by_id(editor_id)
            .one(&db)
            .await
            .unwrap()
            .is_none(),
        "target user row must be gone"
    );
    assert!(
        links::Entity::find_by_id(personal_link_id)
            .one(&db)
            .await
            .unwrap()
            .is_none(),
        "personal links must be permanently deleted"
    );

    let org_link = links::Entity::find_by_id(org_link_id)
        .one(&db)
        .await
        .unwrap()
        .expect("org-owned link must survive hard delete of its creator");
    assert!(
        org_link.deleted_at.is_none(),
        "surviving org link must stay live"
    );
    assert_eq!(org_link.org_id, Some(org_id));
    assert_eq!(
        org_link.user_id,
        Some(owner_id),
        "org link must be reassigned to the org owner so they can list and manage it"
    );

    assert!(
        click_events::Entity::find_by_id(org_click.id)
            .one(&db)
            .await
            .unwrap()
            .is_some(),
        "click history on the surviving org link must remain"
    );
    assert_eq!(
        click_events::Entity::find()
            .filter(click_events::Column::LinkId.eq(personal_link_id))
            .count(&db)
            .await
            .unwrap(),
        0,
        "clicks on deleted personal links must cascade away"
    );

    let res = server.get(&format!("/{org_code}")).await;
    assert!(
        res.status_code().is_redirection(),
        "org link must keep redirecting after creator hard-delete, got {}",
        res.status_code()
    );

    // A remaining org member (the owner) must still be able to list and
    // manage the surviving link. Clearing user_id to NULL without reassigning
    // leaves a live redirect that only an instance admin can take down.
    let listed: Value = server
        .get(&format!("/links?org_id={org_id}"))
        .authorization_bearer(&owner_token)
        .await
        .json();
    let listed_ids: Vec<i64> = listed
        .as_array()
        .expect("GET /links returns an array")
        .iter()
        .filter_map(|l| l["id"].as_i64())
        .collect();
    assert!(
        listed_ids.contains(&(org_link_id as i64)),
        "org owner must see the surviving link in GET /links, got {listed_ids:?}"
    );

    let upd = server
        .put(&format!("/links/{org_link_id}"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "original_url": "https://iana.org/retargeted" }))
        .await;
    assert_eq!(
        upd.status_code(),
        200,
        "org owner must be able to retarget the surviving link: {}",
        upd.text()
    );

    let del = server
        .delete(&format!("/links/{org_link_id}"))
        .authorization_bearer(&owner_token)
        .await;
    assert_eq!(
        del.status_code(),
        200,
        "org owner must be able to take down the surviving link: {}",
        del.text()
    );
}

fn org_link_count(orgs: &Value, org_id: i32) -> i64 {
    orgs.as_array()
        .unwrap()
        .iter()
        .find(|org| org["id"].as_i64() == Some(org_id as i64))
        .expect("org present")["link_count"]
        .as_i64()
        .unwrap()
}

/// Org-facing `link_count` must match the live-link listing (and admin
/// `links_count`), not raw `links.org_id` rows. Soft-deleted links used to
/// inflate GET /orgs, GET /orgs/{id}, PUT /orgs/{id}, and transfer.
#[tokio::test]
async fn org_link_count_excludes_soft_deleted_links() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&server, &db).await;
    let (_member_token, member_id) = register_verified(&server, &db).await;

    let org_id = create_org(&server, &owner_token).await;
    add_member(&db, org_id, member_id, "admin").await;
    let live_id = create_link(&server, &owner_token, Some(org_id)).await;
    let deleted_id = create_link(&server, &owner_token, Some(org_id)).await;
    let _ = live_id;

    let del = server
        .delete(&format!("/links/{deleted_id}"))
        .authorization_bearer(&owner_token)
        .await;
    assert_eq!(
        del.status_code(),
        200,
        "soft-delete org link: {}",
        del.text()
    );

    let list: Value = server
        .get("/orgs")
        .authorization_bearer(&owner_token)
        .await
        .json();
    assert_eq!(
        org_link_count(&list, org_id),
        1,
        "GET /orgs must not count the soft-deleted link"
    );

    let detail: Value = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner_token)
        .await
        .json();
    assert_eq!(
        detail["link_count"].as_i64(),
        Some(1),
        "GET /orgs/{{id}} must not count the soft-deleted link"
    );

    let updated: Value = server
        .put(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Audit Org Renamed" }))
        .await
        .json();
    assert_eq!(
        updated["link_count"].as_i64(),
        Some(1),
        "PUT /orgs/{{id}} must not count the soft-deleted link"
    );

    let transferred = server
        .post(&format!("/orgs/{org_id}/transfer-ownership"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "new_owner_user_id": member_id }))
        .await;
    assert_eq!(
        transferred.status_code(),
        200,
        "transfer: {}",
        transferred.text()
    );
    assert_eq!(
        transferred.json::<Value>()["link_count"].as_i64(),
        Some(1),
        "transfer-ownership must not count the soft-deleted link"
    );
}

#[tokio::test]
async fn invite_member_looks_up_normalized_email() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&server, &db).await;
    let local = format!("User_{}", uuid::Uuid::new_v4().simple());
    let res = server
        .post("/auth/register")
        .json(&json!({
            "email": format!("{local}@Users.OPN.ONL"),
            "password": "password123",
        }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let invitee_id = res.json::<Value>()["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(&db, invitee_id).await;

    let org_id = create_org(&server, &owner_token).await;
    let res = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .json(&json!({
            "email": format!(" {local}@USERS.opn.onl "),
            "role": "viewer",
        }))
        .await;
    assert_eq!(
        res.status_code(),
        201,
        "invite must match the stored normalized email: {}",
        res.text()
    );
    assert_eq!(
        res.json::<Value>()["user_id"].as_i64().unwrap() as i32,
        invitee_id
    );
}

#[tokio::test]
async fn invite_member_rejects_deleted_or_disabled_users() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&server, &db).await;
    let (admin_token, admin_id) = register_verified(&server, &db).await;
    make_admin(&db, admin_id).await;
    let org_id = create_org(&server, &owner_token).await;

    let (_, deleted_id) = register_verified(&server, &db).await;
    let deleted_email = users::Entity::find_by_id(deleted_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .email;
    let res = server
        .delete(&format!("/admin/users/{deleted_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "soft-delete invitee: {}",
        res.text()
    );

    let res = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "email": deleted_email, "role": "viewer" }))
        .await;
    assert_eq!(
        res.status_code(),
        404,
        "deleted user must not be invitable: {}",
        res.text()
    );
    assert!(
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(deleted_id))
            .one(&db)
            .await
            .unwrap()
            .is_none(),
        "must not insert membership for a deleted user"
    );

    let (_, disabled_id) = register_verified(&server, &db).await;
    let disabled = users::Entity::find_by_id(disabled_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let disabled_email = disabled.email.clone();
    let mut active: users::ActiveModel = disabled.into();
    active.disabled_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.unwrap();

    let res = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "email": disabled_email, "role": "viewer" }))
        .await;
    assert_eq!(
        res.status_code(),
        404,
        "disabled user must not be invitable: {}",
        res.text()
    );
    assert!(
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(disabled_id))
            .one(&db)
            .await
            .unwrap()
            .is_none(),
        "must not insert membership for a disabled user"
    );
}

#[tokio::test]
async fn transfer_ownership_rejects_disabled_member() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _) = register_verified(&server, &db).await;
    let (_, member_id) = register_verified(&server, &db).await;
    let org_id = create_org(&server, &owner_token).await;
    add_member(&db, org_id, member_id, "admin").await;

    let member = users::Entity::find_by_id(member_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = member.into();
    active.disabled_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.unwrap();

    let res = server
        .post(&format!("/orgs/{org_id}/transfer-ownership"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "new_owner_user_id": member_id }))
        .await;
    assert_eq!(
        res.status_code(),
        400,
        "disabled member must not become owner: {}",
        res.text()
    );
    assert_eq!(
        res.json::<Value>()["error"].as_str(),
        Some("New owner must be an active user")
    );

    let org = organizations::Entity::find_by_id(org_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        org.owner_id, member_id,
        "ownership must stay with the original owner"
    );
}

#[tokio::test]
async fn create_organization_duplicate_slug_returns_409() {
    let (server, db) = spawn_real_app().await;
    let (token_a, _) = register_verified(&server, &db).await;
    let (token_b, _) = register_verified(&server, &db).await;
    let slug = format!("slug-{}", uuid::Uuid::new_v4().simple());

    let res = server
        .post("/orgs")
        .authorization_bearer(&token_a)
        .json(&json!({ "name": "Org A", "slug": &slug }))
        .await;
    assert_eq!(res.status_code(), 201, "first create: {}", res.text());

    let listed = server.get("/orgs").authorization_bearer(&token_a).await;
    assert_eq!(listed.status_code(), 200);
    assert_eq!(
        listed.json::<Value>().as_array().unwrap().len(),
        1,
        "owner membership must commit with the org row"
    );

    let res = server
        .post("/orgs")
        .authorization_bearer(&token_b)
        .json(&json!({ "name": "Org B", "slug": &slug }))
        .await;
    assert_eq!(
        res.status_code(),
        409,
        "duplicate slug must conflict: {}",
        res.text()
    );
    assert_eq!(
        res.json::<Value>()["error"].as_str(),
        Some("Slug already exists")
    );

    let listed_b = server.get("/orgs").authorization_bearer(&token_b).await;
    assert_eq!(listed_b.status_code(), 200);
    assert!(
        listed_b.json::<Value>().as_array().unwrap().is_empty(),
        "losing create must not leave a membership-less org for the caller"
    );
}

#[tokio::test]
async fn update_organization_duplicate_slug_returns_409() {
    let (server, db) = spawn_real_app().await;
    let (token_a, _) = register_verified(&server, &db).await;
    let (token_b, _) = register_verified(&server, &db).await;
    let slug_a = format!("acme-{}", uuid::Uuid::new_v4().simple());
    let slug_b = format!("beta-{}", uuid::Uuid::new_v4().simple());

    let res = server
        .post("/orgs")
        .authorization_bearer(&token_a)
        .json(&json!({ "name": "Acme", "slug": &slug_a }))
        .await;
    assert_eq!(res.status_code(), 201, "create A: {}", res.text());

    let res = server
        .post("/orgs")
        .authorization_bearer(&token_b)
        .json(&json!({ "name": "Beta", "slug": &slug_b }))
        .await;
    assert_eq!(res.status_code(), 201, "create B: {}", res.text());
    let org_b_id = res.json::<Value>()["id"].as_i64().unwrap() as i32;

    let res = server
        .put(&format!("/orgs/{org_b_id}"))
        .authorization_bearer(&token_b)
        .json(&json!({ "slug": &slug_a }))
        .await;
    assert_eq!(
        res.status_code(),
        409,
        "taken slug must conflict: {}",
        res.text()
    );
    assert_eq!(
        res.json::<Value>()["error"].as_str(),
        Some("Slug already exists")
    );

    let org_b = organizations::Entity::find_by_id(org_b_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(org_b.slug, slug_b, "slug must stay unchanged on conflict");
}

#[tokio::test]
async fn deleted_members_are_omitted_and_do_not_block_owner_deletion() {
    std::env::set_var("ENABLE_ACCOUNT_DELETION", "true");
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id) = register_verified(&server, &db).await;
    let (admin_token, admin_id) = register_verified(&server, &db).await;
    make_admin(&db, admin_id).await;
    let (_, member_id) = register_verified(&server, &db).await;
    let org_id = create_org(&server, &owner_token).await;
    add_member(&db, org_id, member_id, "viewer").await;

    let res = server
        .delete(&format!("/admin/users/{member_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "soft-delete member: {}", res.text());

    let members = server
        .get(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .await;
    assert_eq!(
        members.status_code(),
        200,
        "list members: {}",
        members.text()
    );
    let member_ids: Vec<i32> = members
        .json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["user_id"].as_i64().unwrap() as i32)
        .collect();
    assert!(
        member_ids.contains(&owner_id),
        "owner must still appear in the member list"
    );
    assert!(
        !member_ids.contains(&member_id),
        "soft-deleted user must not appear in the member list: {member_ids:?}"
    );

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&owner_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "deleted members must not block owner account deletion: {}",
        res.text()
    );
}
