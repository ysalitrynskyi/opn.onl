//! Domain-abuse hardening regressions: reserved URL/email domains, retroactive
//! URL-domain takedown, and non-destructive email-domain user disablement.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{api_keys, blocked_email_domains, links, passkeys, users};
use opn_onl_backend::handlers::links::hash_api_key;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register {email}: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user id") as i32,
    )
}

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, String) {
    let email = unique_email();
    let (token, user_id) = register(server, &email).await;
    mark_email_verified(db, user_id).await;
    (token, user_id, email)
}

async fn promote_admin(db: &DatabaseConnection, user_id: i32) {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("promote admin");
}

async fn create_link(server: &axum_test::TestServer, token: &str, url: &str) -> (i32, String) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

#[tokio::test]
async fn reserved_url_targets_are_rejected_on_create_update_routing_and_redirect() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id, _) = register_verified(&server, &db).await;

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://example.com/nope" }))
        .await;
    assert_eq!(res.status_code(), 400, "reserved create: {}", res.text());

    let res = server
        .post("/links/bulk")
        .authorization_bearer(&token)
        .json(&json!({ "urls": ["https://foo.test/bulk"] }))
        .await;
    assert_eq!(res.status_code(), 200, "bulk response: {}", res.text());
    let body: Value = res.json();
    assert!(body["links"].as_array().unwrap().is_empty());
    assert!(body["errors"][0].as_str().unwrap().contains("not allowed"));

    let (link_id, _) = create_link(&server, &token, "https://iana.org/allowed").await;
    let res = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://example.net/update" }))
        .await;
    assert_eq!(res.status_code(), 400, "reserved update: {}", res.text());

    let res = server
        .put(&format!("/links/{link_id}/rules"))
        .authorization_bearer(&token)
        .json(&json!({
            "rules": [{ "destination_url": "https://example.org/routed" }]
        }))
        .await;
    assert_eq!(res.status_code(), 400, "reserved route: {}", res.text());

    let legacy_code = format!("lg{}", unique_code());
    links::ActiveModel {
        code: Set(legacy_code.clone()),
        original_url: Set("https://example.com/legacy".to_string()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert legacy link");

    let res = server.get(&format!("/{legacy_code}")).await;
    assert_eq!(
        res.status_code(),
        410,
        "legacy reserved redirect must be gone: {}",
        res.text()
    );
}

#[tokio::test]
async fn admin_domain_block_soft_disables_exact_and_subdomain_links_only() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id, _) = register_verified(&server, &db).await;
    promote_admin(&db, admin_id).await;
    let (user_token, _, _) = register_verified(&server, &db).await;

    let domain = format!("abuse-{}.iana.org", unique_code().to_lowercase());
    let lookalike = format!("not-{domain}");
    let (exact_id, exact_code) =
        create_link(&server, &user_token, &format!("https://{domain}/a")).await;
    let (sub_id, sub_code) =
        create_link(&server, &user_token, &format!("https://sub.{domain}/b")).await;
    let (lookalike_id, lookalike_code) =
        create_link(&server, &user_token, &format!("https://{lookalike}/c")).await;

    let res = server
        .post("/admin/blocked/domains")
        .authorization_bearer(&admin_token)
        .json(&json!({
            "domain": format!("https://{domain}/ignored/path"),
            "reason": "abuse cluster"
        }))
        .await;
    assert_eq!(res.status_code(), 201, "block domain: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["domain"].as_str(), Some(domain.as_str()));
    assert_eq!(body["affected_links"].as_u64(), Some(2));

    let exact = links::Entity::find_by_id(exact_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let sub = links::Entity::find_by_id(sub_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let lookalike_link = links::Entity::find_by_id(lookalike_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(exact.deleted_at.is_some());
    assert!(sub.deleted_at.is_some());
    assert!(lookalike_link.deleted_at.is_none());

    assert_eq!(
        server.get(&format!("/{exact_code}")).await.status_code(),
        404
    );
    assert_eq!(server.get(&format!("/{sub_code}")).await.status_code(), 404);
    assert!(
        server
            .get(&format!("/{lookalike_code}"))
            .await
            .status_code()
            .is_redirection()
    );
}

#[tokio::test]
async fn email_domain_blocks_reject_registration_and_disable_existing_users_without_deleting_data()
{
    common::set_env("ENABLE_API_KEYS", "true");
    common::set_env("ENABLE_PASSKEYS", "true");

    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id, _) = register_verified(&server, &db).await;
    promote_admin(&db, admin_id).await;

    let domain = format!("mail-{}.iana.org", unique_code().to_lowercase());
    let user_email = format!("victim@{domain}");
    let (user_token, user_id) = register(&server, &user_email).await;
    mark_email_verified(&db, user_id).await;
    let (link_id, _) = create_link(&server, &user_token, "https://iana.org/survives").await;

    let api_key = format!("opn_{}", uuid::Uuid::new_v4().simple());
    api_keys::ActiveModel {
        user_id: Set(user_id),
        name: Set("kept key".to_string()),
        key_hash: Set(hash_api_key(&api_key)),
        key_prefix: Set(api_key.chars().take(12).collect()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert api key");
    passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(format!("domain-abuse-{}", uuid::Uuid::new_v4())),
        cred_public_key: Set("test-public-key".to_string()),
        counter: Set(0),
        name: Set(Some("kept passkey".to_string())),
        created_at: Set(chrono::Utc::now().naive_utc()),
        last_used: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert passkey");

    let res = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(&admin_token)
        .json(&json!({ "domain": &domain, "reason": "disposable mail" }))
        .await;
    assert_eq!(res.status_code(), 201, "block email domain: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["affected_users"].as_u64(), Some(1));

    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(user.deleted_at.is_none());
    assert!(user.disabled_at.is_some());
    assert_eq!(user.disabled_by, Some(admin_id));

    assert_eq!(
        links::Entity::find_by_id(link_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .deleted_at,
        None
    );
    assert_eq!(
        api_keys::Entity::find()
            .filter(api_keys::Column::UserId.eq(user_id))
            .count(&db)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(user_id))
            .count(&db)
            .await
            .unwrap(),
        1
    );

    let res = server
        .post("/auth/register")
        .json(&json!({ "email": format!("new@{domain}"), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 400, "blocked signup: {}", res.text());
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": "new@example.com", "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 400, "reserved signup: {}", res.text());

    let res = server
        .post("/auth/login")
        .json(&json!({ "email": &user_email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 401, "disabled login: {}", res.text());
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&user_token)
            .await
            .status_code(),
        401
    );
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&api_key)
            .await
            .status_code(),
        401
    );
    let passkey_start = server
        .post("/auth/passkey/login/start")
        .json(&json!({ "username": &user_email }))
        .await;
    assert_eq!(
        passkey_start.status_code(),
        200,
        "disabled-domain passkey start must not enumerate: {}",
        passkey_start.text()
    );
    let passkey_body: Value = passkey_start.json();
    assert!(
        passkey_body["options"]["publicKey"]["challenge"].is_string(),
        "decoy challenge: {passkey_body}"
    );
    assert_eq!(
        server
            .get("/sse")
            .add_query_param("token", &user_token)
            .await
            .status_code(),
        401
    );
}

/// Blocking the acting admin's own email domain must be refused. Otherwise the
/// request stamps `disabled_at` on that admin (and every other admin on the
/// domain), `require_admin` starts failing, and nobody can unblock it.
#[tokio::test]
async fn email_domain_block_refuses_to_disable_the_acting_admin() {
    let (server, db) = spawn_real_app().await;
    let domain = format!("adm-{}.iana.org", unique_code().to_lowercase());
    let admin_email = format!("root@{domain}");
    let (admin_token, admin_id) = register(&server, &admin_email).await;
    mark_email_verified(&db, admin_id).await;
    promote_admin(&db, admin_id).await;

    let res = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(&admin_token)
        .json(&json!({ "domain": &domain }))
        .await;
    assert_eq!(
        res.status_code(),
        400,
        "blocking own domain must be refused: {}",
        res.text()
    );
    let body: Value = res.json();
    let message = body["message"].as_str().unwrap_or("");
    assert!(
        message.contains(&admin_email),
        "refusal must name the admin account that would be disabled, got {message:?}"
    );

    let admin = users::Entity::find_by_id(admin_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        admin.disabled_at.is_none(),
        "acting admin must remain enabled"
    );
    assert_eq!(
        blocked_email_domains::Entity::find()
            .filter(blocked_email_domains::Column::Domain.eq(&domain))
            .count(&db)
            .await
            .unwrap(),
        0,
        "refused block must not insert the domain"
    );

    let res = server
        .get("/admin/stats")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(
        res.status_code(),
        200,
        "acting admin must still reach /admin/* after the refused block: {}",
        res.text()
    );
}

/// Admins on a blocked domain must stay able to sign in and unblock it.
/// Regular users on that domain are still disabled.
#[tokio::test]
async fn email_domain_block_skips_other_admins() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id, _) = register_verified(&server, &db).await;
    promote_admin(&db, admin_id).await;

    let domain = format!("peer-{}.iana.org", unique_code().to_lowercase());
    let (peer_token, peer_id) = register(&server, &format!("admin@{domain}")).await;
    mark_email_verified(&db, peer_id).await;
    promote_admin(&db, peer_id).await;

    let (victim_token, victim_id) = register(&server, &format!("user@{domain}")).await;
    mark_email_verified(&db, victim_id).await;

    let res = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(&admin_token)
        .json(&json!({ "domain": &domain }))
        .await;
    assert_eq!(res.status_code(), 201, "block email domain: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["affected_users"].as_u64(), Some(1));

    let peer = users::Entity::find_by_id(peer_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        peer.disabled_at.is_none(),
        "other admins on the blocked domain must stay enabled"
    );
    let victim = users::Entity::find_by_id(victim_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        victim.disabled_at.is_some(),
        "non-admin users on the blocked domain must still be disabled"
    );

    assert_eq!(
        server
            .get("/admin/stats")
            .authorization_bearer(&peer_token)
            .await
            .status_code(),
        200,
        "peer admin must still reach /admin/*"
    );
    assert_eq!(
        server
            .get("/links")
            .authorization_bearer(&victim_token)
            .await
            .status_code(),
        401,
        "disabled victim must not keep their session"
    );
}
