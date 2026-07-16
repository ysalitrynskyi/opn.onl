//! Integration tests for the abuse guards (dangerous file types, raw-IP hosts)
//! and the admin abuse-response tooling (suspicious filter, bulk delete/restore,
//! block-domain-from-link). Real router, real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{json, Value};

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register failed: {}", res.text());
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
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.unwrap();
}

async fn register_admin(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let (token, user_id) = register(server, &unique_email()).await;
    make_admin(db, user_id).await;
    token
}

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32) {
    let (token, user_id) = register(server, &unique_email()).await;
    mark_email_verified(db, user_id).await;
    (token, user_id)
}

/// Insert a link straight into the DB, bypassing the create-time guard — the way
/// a malicious link that predates the guard (or was made while it was off) would
/// exist. Returns the link id.
async fn insert_raw_link(db: &DatabaseConnection, user_id: i32, url: &str) -> (i32, String) {
    use opn_onl_backend::entity::links;
    let code = unique_code();
    let model = links::ActiveModel {
        code: Set(code.clone()),
        original_url: Set(url.to_string()),
        user_id: Set(Some(user_id)),
        created_at: Set(chrono::Utc::now().naive_utc()),
        click_count: Set(0),
        ..Default::default()
    };
    let inserted = model.insert(db).await.expect("insert link");
    (inserted.id, code)
}

#[tokio::test]
async fn create_rejects_dangerous_file_extension() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "http://malware.test/30/puregolds.hta" }))
        .await;
    assert_eq!(res.status_code(), 400, "{}", res.text());
    let body: Value = res.json();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("hta"),
        "error should name the extension: {body}"
    );
}

#[tokio::test]
async fn create_rejects_dangerous_extension_even_with_lure_query() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    // The real-world payload: .hta with a news-headline query-string lure.
    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "http://mal.test/31/goodbrainthings.hta?id=foxbusiness.com/media/story"
        }))
        .await;
    assert_eq!(res.status_code(), 400, "{}", res.text());
}

#[tokio::test]
async fn create_rejects_raw_ip_host() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "http://69.12.83.125/files/thing" }))
        .await;
    assert_eq!(res.status_code(), 400, "{}", res.text());
    let body: Value = res.json();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("ip"),
        "error should mention IP: {body}"
    );
}

#[tokio::test]
async fn create_allows_benign_link() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://www.rona.ca/en/product/lattice-trp3672br" }))
        .await;
    assert_eq!(res.status_code(), 201, "{}", res.text());
}

#[tokio::test]
async fn bulk_create_rejects_only_the_bad_urls_with_reasons() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = register_verified(&server, &db).await;

    let res = server
        .post("/links/bulk")
        .authorization_bearer(&token)
        .json(&json!({ "urls": [
            "https://example.com/ok",
            "http://107.173.143.45/15/givemebless.hta",
            "https://github.com/opn/repo"
        ] }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["links"].as_array().unwrap().len(),
        2,
        "two benign links created"
    );
    let errors = body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "one rejection");
    // The malicious URL trips the extension guard first; either the extension or
    // IP reason is acceptable, but it must reference the offending URL.
    assert!(errors[0].as_str().unwrap().contains("givemebless.hta"));
}

#[tokio::test]
async fn admin_suspicious_filter_finds_preexisting_malicious_links() {
    let (server, db) = spawn_real_app().await;
    let admin_token = register_admin(&server, &db).await;
    let (_, user_id) = register_verified(&server, &db).await;

    // Two malicious links inserted directly (as if pre-guard), one benign.
    let (hta_id, _) = insert_raw_link(&db, user_id, "http://185.99.1.7/x/payload.hta").await;
    let (ip_id, _) = insert_raw_link(&db, user_id, "http://69.12.83.125/30/file").await;
    let (benign_id, _) = insert_raw_link(&db, user_id, "https://good.example.com/article").await;

    let res = server
        .get("/admin/links")
        .add_query_param("user_id", user_id.to_string())
        .add_query_param("suspicious", "true")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();

    let ids: Vec<i64> = body["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&(hta_id as i64)), "hta link should be flagged");
    assert!(
        ids.contains(&(ip_id as i64)),
        "raw-IP link should be flagged"
    );
    assert!(
        !ids.contains(&(benign_id as i64)),
        "benign link must not be flagged"
    );

    // Every returned row carries a suspicion reason.
    for l in body["links"].as_array().unwrap() {
        assert_eq!(l["suspicious"].as_bool(), Some(true));
        assert!(l["suspicion_reason"].as_str().is_some());
    }
}

#[tokio::test]
async fn admin_bulk_delete_and_restore_links() {
    let (server, db) = spawn_real_app().await;
    let admin_token = register_admin(&server, &db).await;
    let (_, user_id) = register_verified(&server, &db).await;

    let (id1, code1) = insert_raw_link(&db, user_id, "http://1.2.3.4/a.hta").await;
    let (id2, code2) = insert_raw_link(&db, user_id, "http://5.6.7.8/b.hta").await;

    // Both redirect before takedown.
    assert!(server
        .get(&format!("/{code1}"))
        .await
        .status_code()
        .is_redirection());

    let res = server
        .post("/admin/links/bulk/delete")
        .authorization_bearer(&admin_token)
        .json(&json!({ "ids": [id1, id2] }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["affected"].as_u64(), Some(2));

    assert_eq!(server.get(&format!("/{code1}")).await.status_code(), 404);
    assert_eq!(server.get(&format!("/{code2}")).await.status_code(), 404);

    let res = server
        .post("/admin/links/bulk/restore")
        .authorization_bearer(&admin_token)
        .json(&json!({ "ids": [id1, id2] }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["affected"].as_u64(), Some(2));
    assert!(server
        .get(&format!("/{code1}"))
        .await
        .status_code()
        .is_redirection());
}

#[tokio::test]
async fn admin_bulk_delete_requires_ids() {
    let (server, db) = spawn_real_app().await;
    let admin_token = register_admin(&server, &db).await;

    let res = server
        .post("/admin/links/bulk/delete")
        .authorization_bearer(&admin_token)
        .json(&json!({ "ids": [] }))
        .await;
    assert_eq!(res.status_code(), 400);
}

#[tokio::test]
async fn admin_block_domain_from_link_blocks_host_and_deletes_link() {
    use opn_onl_backend::entity::blocked_domains;

    let (server, db) = spawn_real_app().await;
    let admin_token = register_admin(&server, &db).await;
    let (user_token, user_id) = register_verified(&server, &db).await;

    // Unique host per run: the test DB persists across runs and the redirect
    // handler refuses already-blocked domains, so a hardcoded host would break
    // on the second run. Extension guard would block .hta on create, so this
    // uses a plain path — the point is the host takedown.
    let domain = format!("evil-{}.test", unique_code().to_lowercase());
    let url = format!("https://{domain}/malware/page");
    let (link_id, code) = insert_raw_link(&db, user_id, &url).await;
    assert!(server
        .get(&format!("/{code}"))
        .await
        .status_code()
        .is_redirection());

    let res = server
        .post(&format!("/admin/links/{link_id}/block-domain"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(body["domain"].as_str(), Some(domain.as_str()));

    // Link is now gone.
    assert_eq!(server.get(&format!("/{code}")).await.status_code(), 404);

    // Domain is recorded as blocked.
    let blocked = blocked_domains::Entity::find()
        .filter(blocked_domains::Column::Domain.eq(&domain))
        .one(&db)
        .await
        .unwrap();
    assert!(blocked.is_some(), "domain should be blocked");

    // And nobody can shorten that host anymore.
    let res = server
        .post("/links")
        .authorization_bearer(&user_token)
        .json(&json!({ "original_url": format!("https://{domain}/another/page") }))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "blocked domain must reject new links: {}",
        res.text()
    );
}

#[tokio::test]
async fn admin_stats_report_suspicious_count() {
    let (server, db) = spawn_real_app().await;
    let admin_token = register_admin(&server, &db).await;
    let (_, user_id) = register_verified(&server, &db).await;

    insert_raw_link(&db, user_id, "http://45.9.9.9/22/thing.hta").await;

    let res = server
        .get("/admin/stats")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();
    assert!(
        body["suspicious_links_count"].as_i64().unwrap() >= 1,
        "at least one suspicious link should be counted: {body}"
    );
}
