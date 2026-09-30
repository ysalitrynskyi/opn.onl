//! Organization list/member/audit/purge results must stay the same after
//! the per-row queries are batched. Real router + real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{
    audit_log, click_events, link_tags, links, org_members, tags, users,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, String) {
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id, email)
}

async fn create_org(server: &axum_test::TestServer, token: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({
            "name": "Perf Org",
            "slug": format!("perf-{}", uuid::Uuid::new_v4().simple()),
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
    .expect("add member");
}

async fn insert_org_link(db: &DatabaseConnection, user_id: i32, org_id: i32) -> i32 {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set("https://iana.org/org".to_string()),
        user_id: Set(Some(user_id)),
        org_id: Set(Some(org_id)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert org link")
    .id
}

#[tokio::test]
async fn org_list_counts_members_and_live_links() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id, _) = register_verified(&server, &db).await;
    let (_member_token, member_id, _) = register_verified(&server, &db).await;

    let org_id = create_org(&server, &owner_token).await;
    add_member(&db, org_id, member_id, "editor").await;

    let live = insert_org_link(&db, owner_id, org_id).await;
    let gone = insert_org_link(&db, owner_id, org_id).await;
    let _ = live;
    let del = server
        .post("/links/bulk/delete")
        .authorization_bearer(&owner_token)
        .json(&json!({ "ids": [gone] }))
        .await;
    assert_eq!(del.status_code(), 200, "delete: {}", del.text());

    let listed: Value = server
        .get("/orgs")
        .authorization_bearer(&owner_token)
        .await
        .json();
    let org = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"].as_i64() == Some(org_id as i64))
        .expect("org");
    assert_eq!(
        org["member_count"].as_i64(),
        Some(2),
        "owner + editor, got {org}"
    );
    assert_eq!(
        org["link_count"].as_i64(),
        Some(1),
        "soft-deleted link excluded, got {org}"
    );
}

#[tokio::test]
async fn org_members_skip_soft_deleted_users_and_keep_emails() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id, owner_email) = register_verified(&server, &db).await;
    let (_live_token, live_id, live_email) = register_verified(&server, &db).await;
    let (_gone_token, gone_id, _) = register_verified(&server, &db).await;

    let org_id = create_org(&server, &owner_token).await;
    add_member(&db, org_id, live_id, "editor").await;
    add_member(&db, org_id, gone_id, "viewer").await;

    let gone = users::Entity::find_by_id(gone_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = gone.into();
    active.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.unwrap();

    let listed: Value = server
        .get(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .await
        .json();
    let members = listed.as_array().unwrap();
    let emails: Vec<&str> = members.iter().filter_map(|m| m["email"].as_str()).collect();
    assert_eq!(members.len(), 2, "deleted user omitted, got {listed}");
    assert!(emails.contains(&owner_email.as_str()));
    assert!(emails.contains(&live_email.as_str()));
    assert!(
        !members
            .iter()
            .any(|m| m["user_id"].as_i64() == Some(gone_id as i64)),
        "deleted user must not be listed, got {listed}"
    );
    let _ = owner_id;
}

#[tokio::test]
async fn org_audit_attaches_emails_and_honours_limit() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id, owner_email) = register_verified(&server, &db).await;
    let org_id = create_org(&server, &owner_token).await;

    for i in 0..5 {
        audit_log::ActiveModel {
            org_id: Set(Some(org_id)),
            user_id: Set(Some(owner_id)),
            action: Set(format!("act-{i}")),
            resource_type: Set("link".to_string()),
            resource_id: Set(Some(i)),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("audit row");
    }

    let all: Value = server
        .get(&format!("/orgs/{org_id}/audit"))
        .authorization_bearer(&owner_token)
        .await
        .json();
    let all_rows = all.as_array().unwrap();
    assert!(
        all_rows.len() >= 5,
        "default page includes the five rows, got {all}"
    );
    assert!(
        all_rows
            .iter()
            .all(|r| r["user_email"].as_str() == Some(owner_email.as_str()))
    );

    let page: Value = server
        .get(&format!("/orgs/{org_id}/audit?limit=2"))
        .authorization_bearer(&owner_token)
        .await
        .json();
    assert_eq!(
        page.as_array().map(|a| a.len()),
        Some(2),
        "limit=2, got {page}"
    );
}

#[tokio::test]
async fn org_purge_deletes_clicks_tags_and_links_for_the_org() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id, _) = register_verified(&server, &db).await;
    let org_id = create_org(&server, &owner_token).await;

    let a = insert_org_link(&db, owner_id, org_id).await;
    let b = insert_org_link(&db, owner_id, org_id).await;
    click_events::ActiveModel {
        link_id: Set(a),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    click_events::ActiveModel {
        link_id: Set(b),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let tag = tags::ActiveModel {
        name: Set("org-tag".to_string()),
        org_id: Set(Some(org_id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    link_tags::ActiveModel {
        link_id: Set(a),
        tag_id: Set(tag.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let del = server
        .delete(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner_token)
        .await;
    assert_eq!(del.status_code(), 204, "purge: {}", del.text());

    assert!(
        links::Entity::find()
            .filter(links::Column::OrgId.eq(org_id))
            .all(&db)
            .await
            .unwrap()
            .is_empty(),
        "org links must be gone"
    );
    assert_eq!(
        click_events::Entity::find()
            .filter(click_events::Column::LinkId.is_in([a, b]))
            .all(&db)
            .await
            .unwrap()
            .len(),
        0,
        "clicks for purged links must be gone"
    );
    assert_eq!(
        link_tags::Entity::find()
            .filter(link_tags::Column::LinkId.is_in([a, b]))
            .all(&db)
            .await
            .unwrap()
            .len(),
        0,
        "tag assignments must be gone"
    );
}
