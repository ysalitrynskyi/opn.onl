//! Owner-deletion policy: an org with other live members must survive its
//! owner's account being deleted. Real router + real Postgres.
//!
//! Replaces the unrun black-box script `scripts/test_owner_deletion_policy.sh`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::users;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, String) {
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": &email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id, email)
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

async fn create_org(server: &axum_test::TestServer, token: &str, slug: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({ "name": "Owner Policy Org", "slug": slug }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

fn unique_slug() -> String {
    format!("own-{}", uuid::Uuid::new_v4().simple())
}

/// Owner + live member + org link, plus an instance admin for the admin
/// delete paths. Fresh rate-limiters after setup so the policy assertions
/// are not 429s.
async fn org_with_live_member() -> (
    axum_test::TestServer,
    String,
    i32,
    String,
    i32,
    String,
    i32,
    String,
) {
    let (setup, db) = spawn_real_app().await;
    let (admin_token, admin_id, _) = register_verified(&setup, &db).await;
    make_admin(&db, admin_id).await;
    let (owner_token, owner_id, _) = register_verified(&setup, &db).await;
    let (member_token, member_id, member_email) = register_verified(&setup, &db).await;

    let slug = unique_slug();
    let org_id = create_org(&setup, &owner_token, &slug).await;
    let invite = setup
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "email": member_email, "role": "admin" }))
        .await;
    assert_eq!(invite.status_code(), 201, "invite: {}", invite.text());

    let link = setup
        .post("/links")
        .authorization_bearer(&member_token)
        .json(&json!({ "original_url": "https://iana.org/team-data", "org_id": org_id }))
        .await;
    assert_eq!(link.status_code(), 201, "org link: {}", link.text());
    let code = link.json::<Value>()["code"].as_str().unwrap().to_string();

    let (server, _) = spawn_real_app().await;
    (
        server,
        admin_token,
        owner_id,
        owner_token,
        org_id,
        member_token,
        member_id,
        code,
    )
}

#[tokio::test]
async fn admin_hard_delete_of_owner_with_members_is_409() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let (server, admin_token, owner_id, _, org_id, member_token, _, code) =
        org_with_live_member().await;

    let hard = server
        .delete(&format!("/admin/users/{owner_id}/hard"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        hard.status_code(),
        409,
        "hard-deleting an owner with live members must conflict: {}",
        hard.text()
    );

    let org = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&member_token)
        .await;
    assert_eq!(
        org.status_code(),
        200,
        "org must still be visible to the remaining member: {}",
        org.text()
    );

    let redirect = server.get(&format!("/{code}")).await;
    assert!(
        redirect.status_code().is_redirection(),
        "member org link must keep redirecting, got {}",
        redirect.status_code()
    );
}

#[tokio::test]
async fn owner_self_delete_with_members_is_409_and_lists_the_org() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let (server, _, _, owner_token, org_id, _, _, _) = org_with_live_member().await;

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&owner_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(
        res.status_code(),
        409,
        "owner self-delete with live members must conflict: {}",
        res.text()
    );
    let body: Value = res.json();
    assert_eq!(
        body["code"].as_str(),
        Some("ORG_OWNERSHIP_TRANSFER_REQUIRED")
    );
    assert_eq!(
        body["organizations"][0]["id"].as_i64(),
        Some(org_id as i64),
        "conflict must list the blocking org"
    );
}

#[tokio::test]
async fn admin_soft_delete_of_owner_with_members_is_409() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let (server, admin_token, owner_id, _, _, _, _, _) = org_with_live_member().await;

    let res = server
        .delete(&format!("/admin/users/{owner_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        res.status_code(),
        409,
        "soft-deleting an owner with live members must conflict: {}",
        res.text()
    );
}

#[tokio::test]
async fn transfer_then_self_delete_leaves_org_and_link_with_new_owner() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let (server, _, _, owner_token, org_id, member_token, member_id, code) =
        org_with_live_member().await;

    let transfer = server
        .post(&format!("/orgs/{org_id}/transfer-ownership"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "new_owner_user_id": member_id }))
        .await;
    assert_eq!(transfer.status_code(), 200, "transfer: {}", transfer.text());
    assert_eq!(
        transfer.json::<Value>()["owner_id"].as_i64(),
        Some(member_id as i64)
    );

    let deleted = server
        .post("/auth/delete-account")
        .authorization_bearer(&owner_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(
        deleted.status_code(),
        200,
        "ex-owner self-delete: {}",
        deleted.text()
    );

    let org = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&member_token)
        .await;
    assert_eq!(
        org.status_code(),
        200,
        "org must survive after the ex-owner is deleted: {}",
        org.text()
    );

    let redirect = server.get(&format!("/{code}")).await;
    assert!(
        redirect.status_code().is_redirection(),
        "org link must keep redirecting after the ex-owner left, got {}",
        redirect.status_code()
    );

    let members = server
        .get(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&member_token)
        .await;
    assert_eq!(members.status_code(), 200, "members: {}", members.text());
    let role = members
        .json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user_id"].as_i64() == Some(member_id as i64))
        .expect("new owner in member list")["role"]
        .as_str()
        .map(str::to_string);
    assert_eq!(role.as_deref(), Some("owner"));
}

#[tokio::test]
async fn hard_delete_of_solo_owner_purges_the_org_and_frees_the_slug() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };
    let (setup, db) = spawn_real_app().await;
    let (admin_token, admin_id, _) = register_verified(&setup, &db).await;
    make_admin(&db, admin_id).await;
    let (solo_token, solo_id, _) = register_verified(&setup, &db).await;
    let (other_token, _, _) = register_verified(&setup, &db).await;
    let slug = unique_slug();
    create_org(&setup, &solo_token, &slug).await;

    let (server, _) = spawn_real_app().await;
    let hard = server
        .delete(&format!("/admin/users/{solo_id}/hard"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        hard.status_code(),
        200,
        "solo owner hard delete: {}",
        hard.text()
    );

    let reuse = server
        .post("/orgs")
        .authorization_bearer(&other_token)
        .json(&json!({ "name": "Solo reuse", "slug": slug }))
        .await;
    assert_eq!(
        reuse.status_code(),
        201,
        "solo org row must be gone so the slug is reusable: {}",
        reuse.text()
    );
}
