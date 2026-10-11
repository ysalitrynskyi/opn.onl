//! Organization membership through the real router: leaving, operations that
//! need a session rather than an API key, live member counts, and what the
//! audit log records about a member who was changed or removed.

mod common;

use axum_test::TestServer;
use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::{links, org_members, users};
use opn_onl_backend::utils::jwt::create_jwt;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

async fn app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    common::set_env("ENABLE_API_KEYS", "true");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
    });
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}

struct Person {
    id: i32,
    email: String,
    token: String,
}

/// A verified account inserted directly (no HTTP, no bcrypt) with a session token.
async fn person(db: &DatabaseConnection) -> Person {
    let user = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &user.email, user.token_version).expect("mint token");
    Person {
        id: user.id,
        email: user.email,
        token,
    }
}

async fn create_org(server: &TestServer, owner: &Person) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Membership Org",
            "slug": format!("mem-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

/// Add `who` to the org through the invite endpoint; returns the member id.
async fn invite(server: &TestServer, by: &Person, org_id: i32, who: &Person, role: &str) -> i32 {
    let res = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&by.token)
        .json(&json!({ "email": &who.email, "role": role }))
        .await;
    assert_eq!(res.status_code(), 201, "invite: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn audit(server: &TestServer, viewer: &Person, org_id: i32) -> Vec<Value> {
    let res = server
        .get(&format!("/orgs/{org_id}/audit"))
        .authorization_bearer(&viewer.token)
        .await;
    assert_eq!(res.status_code(), 200, "audit: {}", res.text());
    res.json::<Vec<Value>>()
}

async fn api_key_for(server: &TestServer, who: &Person) -> String {
    let res = server
        .post("/auth/api-keys")
        .authorization_bearer(&who.token)
        .json(&json!({ "name": "automation" }))
        .await;
    assert_eq!(res.status_code(), 201, "create API key: {}", res.text());
    res.json::<Value>()["key"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn a_member_can_leave_and_their_links_stay_with_the_org() {
    let (server, db) = app().await;
    let owner = person(&db).await;
    let editor = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &editor, "editor").await;

    let created = server
        .post("/links")
        .authorization_bearer(&editor.token)
        .json(&json!({ "original_url": "https://iana.org/leaver", "org_id": org_id }))
        .await;
    assert_eq!(created.status_code(), 201, "org link: {}", created.text());
    let link_id = created.json::<Value>()["id"].as_i64().unwrap() as i32;

    let left = server
        .post(&format!("/orgs/{org_id}/leave"))
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(left.status_code(), 204, "leave: {}", left.text());

    // The membership is gone: no access, no row, and the owner's view agrees.
    assert_eq!(
        server
            .get(&format!("/orgs/{org_id}"))
            .authorization_bearer(&editor.token)
            .await
            .status_code(),
        403
    );
    assert_eq!(
        org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(editor.id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    let members = server
        .get(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner.token)
        .await
        .json::<Vec<Value>>();
    assert_eq!(members.len(), 1, "only the owner is left: {members:?}");

    // The link they created did not go with them.
    let link = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .unwrap()
        .expect("link row");
    assert_eq!(link.org_id, Some(org_id));
    assert!(link.deleted_at.is_none());
    let org = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner.token)
        .await
        .json::<Value>();
    assert_eq!(org["link_count"], 1);

    // Leaving twice is not possible: the second call comes from a non-member.
    assert_eq!(
        server
            .post(&format!("/orgs/{org_id}/leave"))
            .authorization_bearer(&editor.token)
            .await
            .status_code(),
        403
    );

    let entries = audit(&server, &owner, org_id).await;
    let leave = entries
        .iter()
        .find(|entry| entry["action"] == "leave")
        .unwrap_or_else(|| panic!("no leave entry in {entries:?}"));
    assert_eq!(leave["resource_type"], "member");
    assert_eq!(leave["user_id"], editor.id);
    assert_eq!(leave["details"]["target_user_id"], editor.id);
    assert_eq!(leave["details"]["old_role"], "editor");
}

#[tokio::test]
async fn the_owner_cannot_leave_until_ownership_moves() {
    let (server, db) = app().await;
    let owner = person(&db).await;
    let admin = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &admin, "admin").await;
    let leave_path = format!("/orgs/{org_id}/leave");

    let refused = server
        .post(&leave_path)
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(
        refused.status_code(),
        409,
        "owner leave: {}",
        refused.text()
    );
    let body: Value = refused.json();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("transfer-ownership"),
        "the error must say what to do: {body}"
    );
    assert_eq!(body["code"], "ORG_OWNERSHIP_TRANSFER_REQUIRED");
    let org = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner.token)
        .await
        .json::<Value>();
    assert_eq!(org["owner_id"], owner.id, "still the owner");
    assert_eq!(org["member_count"], 2, "still a member");

    // Once ownership moves, the former owner is an ordinary member who may go,
    // and the new owner is the one who cannot.
    let moved = server
        .post(&format!("/orgs/{org_id}/transfer-ownership"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "new_owner_user_id": admin.id }))
        .await;
    assert_eq!(moved.status_code(), 200, "transfer: {}", moved.text());
    assert_eq!(
        server
            .post(&leave_path)
            .authorization_bearer(&admin.token)
            .await
            .status_code(),
        409
    );
    assert_eq!(
        server
            .post(&leave_path)
            .authorization_bearer(&owner.token)
            .await
            .status_code(),
        204
    );
}

#[tokio::test]
async fn leaving_needs_a_signed_in_member() {
    let (server, db) = app().await;
    let owner = person(&db).await;
    let stranger = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    let leave_path = format!("/orgs/{org_id}/leave");

    assert_eq!(server.post(&leave_path).await.status_code(), 401);
    assert_eq!(
        server
            .post(&leave_path)
            .authorization_bearer("not-a-token")
            .await
            .status_code(),
        401
    );
    assert_eq!(
        server
            .post(&leave_path)
            .authorization_bearer(&stranger.token)
            .await
            .status_code(),
        403,
        "a non-member has nothing to leave"
    );
}

#[tokio::test]
async fn an_api_key_cannot_delete_an_org_or_transfer_its_ownership() {
    let (server, db) = app().await;
    let owner = person(&db).await;
    let member = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &member, "admin").await;
    let key = api_key_for(&server, &owner).await;
    let org_path = format!("/orgs/{org_id}");
    let transfer_path = format!("/orgs/{org_id}/transfer-ownership");

    let res = server.delete(&org_path).authorization_bearer(&key).await;
    assert_eq!(res.status_code(), 403, "key delete: {}", res.text());
    assert!(
        res.json::<Value>()["error"]
            .as_str()
            .unwrap()
            .contains("API keys"),
        "the refusal must name the cause: {}",
        res.text()
    );
    let res = server
        .post(&transfer_path)
        .authorization_bearer(&key)
        .json(&json!({ "new_owner_user_id": member.id }))
        .await;
    assert_eq!(res.status_code(), 403, "key transfer: {}", res.text());

    let org = server
        .get(&org_path)
        .authorization_bearer(&owner.token)
        .await
        .json::<Value>();
    assert_eq!(org["owner_id"], owner.id, "nothing moved");

    // Controls: the same key still does everything else on the org, and a
    // missing or bad credential is still a 401, not a 403.
    assert_eq!(
        server
            .get(&org_path)
            .authorization_bearer(&key)
            .await
            .status_code(),
        200
    );
    assert_eq!(
        server
            .put(&org_path)
            .authorization_bearer(&key)
            .json(&json!({ "name": "Renamed by key" }))
            .await
            .status_code(),
        200
    );
    assert_eq!(server.delete(&org_path).await.status_code(), 401);
    assert_eq!(
        server
            .delete(&org_path)
            .authorization_bearer("opn_not_a_real_key")
            .await
            .status_code(),
        401
    );

    // The session of the owner can do both.
    let moved = server
        .post(&transfer_path)
        .authorization_bearer(&owner.token)
        .json(&json!({ "new_owner_user_id": member.id }))
        .await;
    assert_eq!(
        moved.status_code(),
        200,
        "session transfer: {}",
        moved.text()
    );
    assert_eq!(
        server
            .delete(&org_path)
            .authorization_bearer(&owner.token)
            .await
            .status_code(),
        403,
        "no longer the owner"
    );
    assert_eq!(
        server
            .delete(&org_path)
            .authorization_bearer(&member.token)
            .await
            .status_code(),
        204
    );
}

#[tokio::test]
async fn member_count_ignores_deleted_accounts() {
    let (server, db) = app().await;
    let owner = person(&db).await;
    let viewer = person(&db).await;
    let editor = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &viewer, "viewer").await;
    invite(&server, &owner, org_id, &editor, "editor").await;

    let count_of_listing = |body: Vec<Value>| {
        body.iter()
            .find(|org| org["id"] == org_id)
            .map(|org| org["member_count"].as_i64().unwrap())
    };
    let before = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner.token)
        .await
        .json::<Value>();
    assert_eq!(before["member_count"], 3);

    // Soft delete keeps the org_members row; the member list already hides it.
    let account = users::Entity::find_by_id(viewer.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut account: users::ActiveModel = account.into();
    account.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));
    account.update(&db).await.unwrap();

    let members = server
        .get(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner.token)
        .await
        .json::<Vec<Value>>();
    assert_eq!(members.len(), 2, "the list hides the deleted account");

    let one = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner.token)
        .await
        .json::<Value>();
    assert_eq!(one["member_count"], 2, "GET /orgs/{{id}}");
    let listing = server
        .get("/orgs")
        .authorization_bearer(&owner.token)
        .await
        .json::<Vec<Value>>();
    assert_eq!(count_of_listing(listing), Some(2), "GET /orgs");
    let renamed = server
        .put(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "name": "Still two" }))
        .await
        .json::<Value>();
    assert_eq!(renamed["member_count"], 2, "PUT /orgs/{{id}}");
    let moved = server
        .post(&format!("/orgs/{org_id}/transfer-ownership"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "new_owner_user_id": editor.id }))
        .await
        .json::<Value>();
    assert_eq!(moved["member_count"], 2, "transfer-ownership");
}

#[tokio::test]
async fn audit_entries_say_who_was_changed_and_how() {
    let (server, db) = app().await;
    let owner = person(&db).await;
    let member = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    let member_id = invite(&server, &owner, org_id, &member, "viewer").await;
    let member_path = format!("/orgs/{org_id}/members/{member_id}");

    let res = server
        .put(&member_path)
        .authorization_bearer(&owner.token)
        .json(&json!({ "role": "editor" }))
        .await;
    assert_eq!(res.status_code(), 200, "role change: {}", res.text());
    let res = server
        .delete(&member_path)
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(res.status_code(), 204, "remove: {}", res.text());

    let entries = audit(&server, &owner, org_id).await;
    let find = |action: &str| {
        entries
            .iter()
            .find(|entry| entry["action"] == action)
            .unwrap_or_else(|| panic!("no {action} entry in {entries:?}"))
    };

    let role_change = find("update_role");
    assert_eq!(role_change["user_id"], owner.id, "who acted");
    assert_eq!(role_change["resource_id"], member_id);
    assert_eq!(role_change["details"]["target_user_id"], member.id);
    assert_eq!(role_change["details"]["old_role"], "viewer");
    assert_eq!(role_change["details"]["new_role"], "editor");

    // The membership row is gone, so the entry has to carry the user itself.
    let removal = find("remove");
    assert_eq!(removal["user_id"], owner.id, "who acted");
    assert_eq!(removal["details"]["target_user_id"], member.id);
    assert_eq!(removal["details"]["old_role"], "editor");
}
