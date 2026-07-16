//! Regression test for the check_blocked domain matching (audit perf finding:
//! the redirect hot path scanned the whole blocked_domains table). The rewrite
//! queries host + parent-domain candidates against the indexed `domain` column;
//! this test pins the matching semantics (exact + subdomain block, lookalikes
//! allowed). Real router + real Postgres via `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{json, Value};

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().unwrap().to_string(),
        body["user_id"].as_i64().unwrap() as i32,
    )
}

async fn make_admin(db: &DatabaseConnection, user_id: i32) {
    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    let mut a: users::ActiveModel = user.into();
    a.is_admin = Set(true);
    a.update(db).await.unwrap();
}

async fn create_status(server: &axum_test::TestServer, token: &str, url: &str) -> u16 {
    server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await
        .status_code()
        .as_u16()
}

#[tokio::test]
async fn blocked_domain_matches_host_and_subdomains_only() {
    let (server, db) = spawn_real_app().await;

    let (admin_token, admin_id) = register(&server, &unique_email()).await;
    make_admin(&db, admin_id).await;
    let (user_token, user_id) = register(&server, &unique_email()).await;
    mark_email_verified(&db, user_id).await;

    // Use a unique blocked domain so parallel tests don't interfere.
    let suffix = format!("blk{}.example", admin_id);
    let blocked = format!("evil-{suffix}.com");
    let res = server
        .post("/admin/blocked/domains")
        .authorization_bearer(&admin_token)
        .json(&json!({ "domain": format!("HTTPS://{blocked}/") })) // exercises write-time normalization
        .await;
    assert_eq!(res.status_code(), 201, "block domain: {}", res.text());

    // Exact host and any subdomain are blocked (403 on create).
    assert_eq!(
        create_status(&server, &user_token, &format!("https://{blocked}/x")).await,
        403,
        "exact host must be blocked"
    );
    assert_eq!(
        create_status(
            &server,
            &user_token,
            &format!("https://deep.sub.{blocked}/y")
        )
        .await,
        403,
        "subdomain must be blocked"
    );

    // Lookalikes that are NOT the host or a parent domain are allowed (201).
    assert_eq!(
        create_status(&server, &user_token, &format!("https://not{blocked}/z")).await,
        201,
        "lookalike host must be allowed"
    );
    assert_eq!(
        create_status(
            &server,
            &user_token,
            &format!("https://evil-{suffix}.org/w")
        )
        .await,
        201,
        "different TLD must be allowed"
    );
}
