//! Links in an organization the user has left must not count against
//! `MAX_LINKS_PER_USER` or the profile counters. The user can no longer delete
//! them, so counting them locks the account at the cap. This file is its own
//! test binary: the cap is a process-wide variable, same as
//! `link_cap_settings_tests`.

mod common;

use std::sync::Arc;

use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::{click_events, users};
use opn_onl_backend::utils::jwt::create_jwt;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::{Value, json};

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

struct Person {
    email: String,
    token: String,
}

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
        email: user.email,
        token,
    }
}

async fn org_with_editor(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (Person, Person, i32, i32) {
    let owner = person(db).await;
    let editor = person(db).await;
    let created = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Cap Org",
            "slug": format!("cap-{}", uuid::Uuid::new_v4().simple()),
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
    let member_id = invited.json::<Value>()["id"].as_i64().unwrap() as i32;
    (owner, editor, org_id, member_id)
}

async fn create_org_link(
    server: &axum_test::TestServer,
    editor: &Person,
    org_id: i32,
    url: &str,
) -> i32 {
    let res = server
        .post("/links")
        .authorization_bearer(&editor.token)
        .json(&json!({ "original_url": url, "org_id": org_id }))
        .await;
    assert_eq!(res.status_code(), 201, "org link: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

fn counts(body: &Value) -> (i64, i64) {
    (
        body["link_count"].as_i64().expect("link_count"),
        body["total_clicks"].as_i64().expect("total_clicks"),
    )
}

#[tokio::test]
async fn links_in_an_org_the_user_left_do_not_count() {
    common::set_env("MAX_LINKS_PER_USER", "1");
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    // The per-IP per-second gate is 10. This test's control path is more
    // requests than that in well under a second.
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
        report: roomy(),
    });
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("server");

    let (owner, editor, org_id, member_id) = org_with_editor(&server, &db).await;
    let link_id = create_org_link(&server, &editor, org_id, "https://iana.org/left-org-cap").await;
    click_events::ActiveModel {
        link_id: Set(link_id),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert click");

    let me = server
        .get("/auth/me")
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(me.status_code(), 200, "me: {}", me.text());
    assert_eq!(
        counts(&me.json()),
        (1, 1),
        "while a member, the org link and its click count"
    );

    let blocked = server
        .post("/links")
        .authorization_bearer(&editor.token)
        .json(&json!({ "original_url": "https://iana.org/left-org-cap-next" }))
        .await;
    assert_eq!(
        blocked.status_code(),
        403,
        "control: the cap applies while they are still a member: {}",
        blocked.text()
    );

    let cloned = server
        .post(&format!("/links/{link_id}/clone"))
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(
        cloned.status_code(),
        403,
        "control: clone honours the same cap: {}",
        cloned.text()
    );

    let bulk_blocked = server
        .post("/links/bulk")
        .authorization_bearer(&editor.token)
        .json(&json!({ "urls": ["https://iana.org/left-org-cap-bulk"] }))
        .await;
    assert_eq!(
        bulk_blocked.status_code(),
        200,
        "bulk: {}",
        bulk_blocked.text()
    );
    let bulk_body: Value = bulk_blocked.json();
    assert!(
        bulk_body["links"].as_array().unwrap().is_empty(),
        "bulk creates nothing at the cap: {bulk_body}"
    );
    assert!(
        bulk_body["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|err| err.as_str().unwrap_or("").contains("account link limit")),
        "bulk reports the cap: {bulk_body}"
    );

    let removed = server
        .delete(&format!("/orgs/{org_id}/members/{member_id}"))
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(removed.status_code(), 204, "remove: {}", removed.text());

    let me = server
        .get("/auth/me")
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(me.status_code(), 200, "me after leave: {}", me.text());
    assert_eq!(
        counts(&me.json()),
        (0, 0),
        "a link they can no longer delete does not count"
    );

    let profile = server
        .put("/auth/profile")
        .authorization_bearer(&editor.token)
        .json(&json!({ "display_name": "Editor" }))
        .await;
    assert_eq!(profile.status_code(), 200, "profile: {}", profile.text());
    assert_eq!(counts(&profile.json()), (0, 0));

    let created = server
        .post("/links")
        .authorization_bearer(&editor.token)
        .json(&json!({ "original_url": "https://iana.org/left-org-cap-free" }))
        .await;
    assert_eq!(
        created.status_code(),
        201,
        "they can create again after leaving: {}",
        created.text()
    );

    // The personal link now fills the cap. Bulk must refuse it, which is the
    // same helper as create: the org link they left behind still does not count
    // (that would have refused the create above).
    let bulk_again = server
        .post("/links/bulk")
        .authorization_bearer(&editor.token)
        .json(&json!({ "urls": ["https://iana.org/left-org-cap-bulk-2"] }))
        .await;
    assert_eq!(
        bulk_again.status_code(),
        200,
        "bulk at cap: {}",
        bulk_again.text()
    );
    assert!(
        bulk_again.json::<Value>()["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|err| err.as_str().unwrap_or("").contains("account link limit")),
        "bulk still enforces the cap for links they own: {}",
        bulk_again.text()
    );

    // Cap 2, so a clone is allowed only when the abandoned org link is ignored.
    common::set_env("MAX_LINKS_PER_USER", "2");
    let (owner, editor, org_id, member_id) = org_with_editor(&server, &db).await;
    let org_link =
        create_org_link(&server, &editor, org_id, "https://iana.org/left-org-clone").await;
    let personal = server
        .post("/links")
        .authorization_bearer(&editor.token)
        .json(&json!({ "original_url": "https://iana.org/left-org-clone-own" }))
        .await;
    assert_eq!(personal.status_code(), 201, "personal: {}", personal.text());
    let personal_id = personal.json::<Value>()["id"].as_i64().unwrap();
    let at_cap = server
        .post(&format!("/links/{org_link}/clone"))
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(
        at_cap.status_code(),
        403,
        "control: clone is refused at the cap while they are a member: {}",
        at_cap.text()
    );

    let removed = server
        .delete(&format!("/orgs/{org_id}/members/{member_id}"))
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(removed.status_code(), 204, "remove: {}", removed.text());

    let me = server
        .get("/auth/me")
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(counts(&me.json()), (1, 0), "only the personal link remains");

    let cloned = server
        .post(&format!("/links/{personal_id}/clone"))
        .authorization_bearer(&editor.token)
        .await;
    assert_eq!(
        cloned.status_code(),
        201,
        "clone is allowed once the abandoned org link no longer counts: {}",
        cloned.text()
    );

    common::remove_env("MAX_LINKS_PER_USER");
}
