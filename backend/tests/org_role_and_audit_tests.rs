//! Organization role matrix and audit log, through the real router.
//! Product roles are owner / admin / editor / viewer (`org_members::Model`).

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::org_members;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::{json, Value};

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

async fn get(server: &axum_test::TestServer, path: &str, token: &str) -> u16 {
    server
        .get(path)
        .authorization_bearer(token)
        .await
        .status_code()
        .as_u16()
}

async fn put(server: &axum_test::TestServer, path: &str, token: &str, body: Value) -> u16 {
    server
        .put(path)
        .authorization_bearer(token)
        .json(&body)
        .await
        .status_code()
        .as_u16()
}

async fn post(server: &axum_test::TestServer, path: &str, token: &str, body: Value) -> u16 {
    server
        .post(path)
        .authorization_bearer(token)
        .json(&body)
        .await
        .status_code()
        .as_u16()
}

async fn delete(server: &axum_test::TestServer, path: &str, token: &str) -> u16 {
    server
        .delete(path)
        .authorization_bearer(token)
        .await
        .status_code()
        .as_u16()
}

/// Viewer can read; editor can read but not administer; admin can invite and
/// edit the org but cannot transfer or delete it; only the owner can.
#[tokio::test]
async fn org_role_matrix_on_org_endpoints() {
    let (setup, db) = spawn_real_app().await;
    let (owner_token, _, _) = register_verified(&setup, &db).await;
    let (admin_token, admin_id, _) = register_verified(&setup, &db).await;
    let (editor_token, editor_id, _) = register_verified(&setup, &db).await;
    let (viewer_token, viewer_id, _) = register_verified(&setup, &db).await;
    let (stranger_token, _, _) = register_verified(&setup, &db).await;
    let (_, _, invitee_email) = register_verified(&setup, &db).await;

    let slug = format!("role-{}", uuid::Uuid::new_v4().simple());
    let org = setup
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Role Matrix", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap() as i32;
    add_member(&db, org_id, admin_id, "admin").await;
    add_member(&db, org_id, editor_id, "editor").await;
    add_member(&db, org_id, viewer_id, "viewer").await;

    let org_path = format!("/orgs/{org_id}");
    let members_path = format!("/orgs/{org_id}/members");
    let audit_path = format!("/orgs/{org_id}/audit");
    let transfer_path = format!("/orgs/{org_id}/transfer-ownership");

    // Fresh limiters per block: the per-IP bucket is 10/sec and these
    // tests share the mock-transport "unknown" address.
    let (server, _) = spawn_real_app().await;
    assert_eq!(get(&server, &org_path, &viewer_token).await, 200);
    assert_eq!(get(&server, &org_path, &editor_token).await, 200);
    assert_eq!(get(&server, &members_path, &viewer_token).await, 200);
    assert_eq!(get(&server, &org_path, &stranger_token).await, 403);

    let (server, _) = spawn_real_app().await;
    assert_eq!(
        put(&server, &org_path, &viewer_token, json!({ "name": "Nope" })).await,
        403
    );
    assert_eq!(
        put(&server, &org_path, &editor_token, json!({ "name": "Nope" })).await,
        403
    );
    assert_eq!(
        put(
            &server,
            &org_path,
            &admin_token,
            json!({ "name": "Admin Rename" })
        )
        .await,
        200
    );
    assert_eq!(get(&server, &audit_path, &viewer_token).await, 403);
    assert_eq!(get(&server, &audit_path, &editor_token).await, 403);
    assert_eq!(get(&server, &audit_path, &admin_token).await, 200);
    assert_eq!(get(&server, &audit_path, &owner_token).await, 200);

    let (server, _) = spawn_real_app().await;
    assert_eq!(
        post(
            &server,
            &members_path,
            &viewer_token,
            json!({ "email": invitee_email, "role": "viewer" })
        )
        .await,
        403
    );
    assert_eq!(
        post(
            &server,
            &members_path,
            &editor_token,
            json!({ "email": invitee_email, "role": "viewer" })
        )
        .await,
        403
    );
    assert_eq!(
        post(
            &server,
            &members_path,
            &admin_token,
            json!({ "email": invitee_email, "role": "member" })
        )
        .await,
        400
    );
    let invited = server
        .post(&members_path)
        .authorization_bearer(&admin_token)
        .json(&json!({ "email": invitee_email, "role": "viewer" }))
        .await;
    assert_eq!(
        invited.status_code(),
        201,
        "admin invite: {}",
        invited.text()
    );
    let invited_member_id = invited.json::<Value>()["id"].as_i64().unwrap() as i32;

    let (server, _) = spawn_real_app().await;
    assert_eq!(
        put(
            &server,
            &format!("/orgs/{org_id}/members/{invited_member_id}"),
            &editor_token,
            json!({ "role": "editor" })
        )
        .await,
        403
    );
    assert_eq!(
        put(
            &server,
            &format!("/orgs/{org_id}/members/{invited_member_id}"),
            &admin_token,
            json!({ "role": "editor" })
        )
        .await,
        200
    );
    assert_eq!(
        put(
            &server,
            &format!("/orgs/{org_id}/members/{invited_member_id}"),
            &admin_token,
            json!({ "role": "owner" })
        )
        .await,
        400
    );

    let (server, _) = spawn_real_app().await;
    assert_eq!(
        post(
            &server,
            &transfer_path,
            &admin_token,
            json!({ "new_owner_user_id": admin_id })
        )
        .await,
        403
    );
    assert_eq!(delete(&server, &org_path, &admin_token).await, 403);
    assert_eq!(delete(&server, &org_path, &editor_token).await, 403);
    assert_eq!(delete(&server, &org_path, &viewer_token).await, 403);
    assert_eq!(delete(&server, &org_path, &owner_token).await, 204);

    let gone = server
        .get(&org_path)
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        gone.status_code(),
        403,
        "deleted org must not be readable: {}",
        gone.text()
    );
}

/// Creating an org and inviting a member writes the shipped audit actions
/// (`create`/`organization`, `invite`/`member`). There is no action-type
/// allowlist in the handler — the strings are hardcoded at the write site.
#[tokio::test]
async fn org_audit_log_records_create_and_invite() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _, _) = register_verified(&server, &db).await;
    let (_, _, invitee_email) = register_verified(&server, &db).await;

    let slug = format!("aud-{}", uuid::Uuid::new_v4().simple());
    let org = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Audit Org", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap() as i32;

    let invite = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "email": invitee_email, "role": "viewer" }))
        .await;
    assert_eq!(invite.status_code(), 201, "invite: {}", invite.text());

    let logs = server
        .get(&format!("/orgs/{org_id}/audit"))
        .authorization_bearer(&owner_token)
        .await;
    assert_eq!(logs.status_code(), 200, "audit: {}", logs.text());
    let entries = logs.json::<Value>();
    let pairs: Vec<(String, String)> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["action"].as_str().unwrap().to_string(),
                e["resource_type"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        pairs
            .iter()
            .any(|(a, r)| a == "create" && r == "organization"),
        "expected create/organization in {pairs:?}"
    );
    assert!(
        pairs.iter().any(|(a, r)| a == "invite" && r == "member"),
        "expected invite/member in {pairs:?}"
    );
}
