//! A trailing slash on `FRONTEND_URL` or `BASE_URL` must not produce `//` in a
//! short link. This binary owns those variables, the way `cors_tests` owns
//! `FRONTEND_URL`, so setting them cannot race another test.

mod common;

use chrono::Utc;
use common::{set_env, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{folders, link_tags, links, tags, users};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::Value;

const FRONTEND: &str = "https://links.example";
const API: &str = "https://api.example";

fn location(res: &axum_test::TestResponse) -> String {
    res.headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string()
}

async fn person(db: &DatabaseConnection, username: &str) -> (i32, String) {
    let email = unique_email();
    let user = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        bio_username: Set(Some(username.to_string())),
        bio_enabled: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &email, user.token_version).expect("mint token");
    (user.id, token)
}

/// Every public URL built from the configured bases, with a trailing slash and
/// without one. The no-slash case is the control: stripping must not eat the host.
#[tokio::test]
async fn configured_bases_join_a_code_with_one_slash() {
    set_env("FRONTEND_URL", format!("{FRONTEND}/"));
    set_env("BASE_URL", format!("{API}/"));
    let (server, db) = spawn_real_app().await;

    let username = format!("b{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let (user_id, token) = person(&db, &username).await;
    let code = unique_code();
    let folder = folders::ActiveModel {
        name: Set("inbox".to_string()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert folder");
    let tag = tags::ActiveModel {
        name: Set("kept".to_string()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert tag");
    let link = links::ActiveModel {
        code: Set(code.clone()),
        original_url: Set("https://iana.org/base-slash".to_string()),
        user_id: Set(Some(user_id)),
        password_hash: Set(Some("not-checked".to_string())),
        folder_id: Set(Some(folder.id)),
        bio_visible: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert link");
    link_tags::ActiveModel {
        link_id: Set(link.id),
        tag_id: Set(tag.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("attach tag");

    let short = format!("{FRONTEND}/{code}");
    let api = format!("{API}/{code}");

    let listed = server.get("/links").authorization_bearer(&token).await;
    assert_eq!(listed.status_code(), 200, "list: {}", listed.text());
    let listed: Value = listed.json();
    assert_eq!(listed[0]["short_url"], short);
    assert_eq!(listed[0]["api_url"], api);

    let preview = server.get(&format!("/{code}/preview")).await;
    assert_eq!(preview.status_code(), 200, "preview: {}", preview.text());
    assert_eq!(preview.json::<Value>()["short_url"], short);

    let prompt = server.get(&format!("/{code}")).await;
    assert_eq!(
        prompt.status_code(),
        307,
        "password prompt: {}",
        prompt.text()
    );
    assert_eq!(location(&prompt), format!("{FRONTEND}/password/{code}"));

    let by_tag = server
        .get(&format!("/tags/{}/links", tag.id))
        .authorization_bearer(&token)
        .await;
    assert_eq!(by_tag.status_code(), 200, "tag links: {}", by_tag.text());
    let by_tag: Value = by_tag.json();
    assert_eq!(by_tag[0]["short_url"], short);
    assert_eq!(by_tag[0]["api_url"], api);

    let by_folder = server
        .get(&format!("/folders/{}/links", folder.id))
        .authorization_bearer(&token)
        .await;
    assert_eq!(
        by_folder.status_code(),
        200,
        "folder links: {}",
        by_folder.text()
    );
    assert_eq!(by_folder.json::<Value>()[0]["short_url"], short);

    let bio = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(bio.status_code(), 200, "bio: {}", bio.text());
    assert_eq!(bio.json::<Value>()["links"][0]["short_url"], short);

    // Control: the same link, with the slash already absent, stays one slash.
    set_env("FRONTEND_URL", FRONTEND);
    set_env("BASE_URL", API);
    let again = server.get("/links").authorization_bearer(&token).await;
    assert_eq!(
        again.status_code(),
        200,
        "list without slash: {}",
        again.text()
    );
    let again: Value = again.json();
    assert_eq!(again[0]["short_url"], short);
    assert_eq!(again[0]["api_url"], api);
}
