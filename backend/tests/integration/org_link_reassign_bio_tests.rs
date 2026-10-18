//! Hard-deleting a member reassigns their organization links to the org owner.
//! The bio opt-in is the member's, so those links must not appear on the
//! owner's public bio afterwards.

use crate::common;

use common::{setup_test_db, unique_code, unique_email};
use opn_onl_backend::entity::{links, users};
use opn_onl_backend::utils::jwt::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

struct Person {
    id: i32,
    email: String,
    token: String,
}

async fn person(db: &DatabaseConnection, admin: bool) -> Person {
    let user = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(admin),
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

fn username() -> String {
    format!("bio{}", unique_code().to_lowercase())
}

async fn enable_bio(server: &axum_test::TestServer, who: &Person) -> String {
    let name = username();
    let res = server
        .put("/auth/bio")
        .authorization_bearer(&who.token)
        .json(&json!({ "bio_username": &name, "bio_enabled": true }))
        .await;
    assert_eq!(res.status_code(), 200, "enable bio: {}", res.text());
    name
}

fn bio_codes(body: &Value) -> Vec<String> {
    body["links"]
        .as_array()
        .expect("links")
        .iter()
        .map(|link| link["code"].as_str().expect("code").to_string())
        .collect()
}

#[tokio::test]
async fn reassigned_org_links_do_not_stay_on_the_owners_bio() {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("server");

    let owner = person(&db, true).await;
    let editor = person(&db, false).await;
    let owner_bio = enable_bio(&server, &owner).await;
    let editor_bio = enable_bio(&server, &editor).await;

    let created = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Bio Org",
            "slug": format!("bio-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(created.status_code(), 201, "create org: {}", created.text());
    let org_id = created.json::<Value>()["id"].as_i64().unwrap() as i32;

    let invited = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "email": &editor.email, "role": "editor" }))
        .await;
    assert_eq!(invited.status_code(), 201, "invite: {}", invited.text());

    let link = server
        .post("/links")
        .authorization_bearer(&editor.token)
        .json(&json!({
            "original_url": "https://iana.org/reassign-bio",
            "org_id": org_id,
            "title": "Team link",
        }))
        .await;
    assert_eq!(link.status_code(), 201, "create org link: {}", link.text());
    let link_body: Value = link.json();
    let link_id = link_body["id"].as_i64().unwrap() as i32;
    let code = link_body["code"].as_str().unwrap().to_string();

    let visible = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&editor.token)
        .json(&json!({ "bio_visible": true }))
        .await;
    assert_eq!(
        visible.status_code(),
        200,
        "bio visible: {}",
        visible.text()
    );

    let stored = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    let mut active: links::ActiveModel = stored.into();
    active.bio_label = Set(Some("Team link".to_string()));
    active.bio_position = Set(Some(3));
    active.update(&db).await.expect("set bio label");

    let editor_page = server.get(&format!("/api/bio/{editor_bio}")).await;
    assert_eq!(
        editor_page.status_code(),
        200,
        "editor bio: {}",
        editor_page.text()
    );
    assert!(
        bio_codes(&editor_page.json()).contains(&code),
        "control: the link is on the editor's bio before the delete"
    );

    let deleted = server
        .delete(&format!("/admin/users/{}/hard", editor.id))
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(
        deleted.status_code(),
        200,
        "hard delete editor: {}",
        deleted.text()
    );

    let owner_page = server.get(&format!("/api/bio/{owner_bio}")).await;
    assert_eq!(
        owner_page.status_code(),
        200,
        "owner bio: {}",
        owner_page.text()
    );
    assert!(
        !bio_codes(&owner_page.json()).contains(&code),
        "the deleted member's link must not publish on the owner's bio: {}",
        owner_page.text()
    );

    let row = links::Entity::find()
        .filter(links::Column::Id.eq(link_id))
        .one(&db)
        .await
        .expect("db")
        .expect("the org link is reassigned, not deleted");
    assert_eq!(row.user_id, Some(owner.id));
    assert_eq!(row.org_id, Some(org_id));
    assert!(!row.bio_visible);
    assert_eq!(row.bio_label, None);
    assert_eq!(row.bio_position, None);
}
