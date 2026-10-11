//! Taking an account down must take its links down: soft-deleting a user also
//! kills the links of organizations nobody else is left in, and a user the
//! admin disabled (email-domain block) stops serving links and the bio page
//! until re-enabled. Real router + real Postgres; the cache-hit path runs only
//! when `REDIS_URL` points at a Redis (as in CI).

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::AppState;
use opn_onl_backend::entity::{links, org_members, users};
use opn_onl_backend::utils::cache::RedisCache;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::sync::Arc;

// ---------------------------------------------------------------- helpers

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
) -> (String, i32) {
    let (token, user_id) = register(server, &unique_email()).await;
    mark_email_verified(db, user_id).await;
    (token, user_id)
}

/// A user whose address sits on `domain`, so an email-domain block disables them.
async fn register_on_domain(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
    domain: &str,
) -> (String, i32) {
    let (token, user_id) = register(
        server,
        &format!("user-{}@{domain}", uuid::Uuid::new_v4().simple()),
    )
    .await;
    mark_email_verified(db, user_id).await;
    (token, user_id)
}

fn fresh_domain() -> String {
    format!("takedown-{}.iana.org", uuid::Uuid::new_v4().simple())
}

async fn register_admin(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let (token, admin_id) = register_verified(server, db).await;
    let user = users::Entity::find_by_id(admin_id)
        .one(db)
        .await
        .expect("db")
        .expect("admin user");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("promote admin");
    token
}

async fn create_link(
    server: &axum_test::TestServer,
    token: &str,
    org_id: Option<i32>,
) -> (i32, String) {
    let mut body = json!({ "original_url": "https://iana.org/takedown" });
    if let Some(org_id) = org_id {
        body["org_id"] = json!(org_id);
    }
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

async fn create_org(server: &axum_test::TestServer, token: &str) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(token)
        .json(&json!({
            "name": "Takedown Org",
            "slug": format!("takedown-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().expect("org id") as i32
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
    .expect("add org member");
}

async fn block_email_domain(
    server: &axum_test::TestServer,
    admin_token: &str,
    domain: &str,
) -> Value {
    let res = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(admin_token)
        .json(&json!({ "domain": domain, "reason": "takedown test" }))
        .await;
    assert_eq!(res.status_code(), 201, "block email domain: {}", res.text());
    res.json()
}

/// Lift the block and re-enable the user: the only way back from "disabled".
async fn unblock_and_enable(
    server: &axum_test::TestServer,
    admin_token: &str,
    block: &Value,
    user_id: i32,
) {
    let res = server
        .delete(&format!(
            "/admin/blocked/email-domains/{}",
            block["id"].as_i64().expect("block id")
        ))
        .authorization_bearer(admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "unblock: {}", res.text());
    let res = server
        .post(&format!("/admin/users/{user_id}/enable"))
        .authorization_bearer(admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "enable: {}", res.text());
}

async fn enable_bio(server: &axum_test::TestServer, token: &str) -> String {
    let username = format!("bio{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let res = server
        .put("/auth/bio")
        .authorization_bearer(token)
        .json(&json!({ "bio_username": &username, "bio_enabled": true }))
        .await;
    assert_eq!(res.status_code(), 200, "enable bio: {}", res.text());
    username
}

fn is_dead(status: u16) -> bool {
    status == 404 || status == 410
}

async fn redirect_status(server: &axum_test::TestServer, code: &str) -> u16 {
    server.get(&format!("/{code}")).await.status_code().as_u16()
}

async fn assert_redirects(server: &axum_test::TestServer, code: &str, what: &str) {
    assert_eq!(
        redirect_status(server, code).await,
        307,
        "{what} should redirect"
    );
}

async fn assert_stopped(server: &axum_test::TestServer, code: &str, what: &str) {
    let status = redirect_status(server, code).await;
    assert!(is_dead(status), "{what} should be 404/410, got {status}");
}

async fn link_row(db: &DatabaseConnection, code: &str) -> links::Model {
    links::Entity::find()
        .filter(links::Column::Code.eq(code))
        .one(db)
        .await
        .expect("db")
        .expect("link row")
}

async fn user_row(db: &DatabaseConnection, user_id: i32) -> users::Model {
    users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user row")
}

// ------------------------------------------- solo-org links on user soft delete

/// Fixture: an owner who is the only member of their org, with one personal
/// link and one org link; plus an unrelated team (owned by someone else) in
/// which the owner is just an editor, with one link they created there.
/// Returns (owner_token, owner_id, personal_code, solo_org_code, team_code).
async fn solo_owner_fixture(
    setup: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, String, String, String) {
    let (owner_token, owner_id) = register_verified(setup, db).await;
    let org_id = create_org(setup, &owner_token).await;
    let (_, personal_code) = create_link(setup, &owner_token, None).await;
    let (_, solo_org_code) = create_link(setup, &owner_token, Some(org_id)).await;

    let (team_owner_token, _) = register_verified(setup, db).await;
    let team_id = create_org(setup, &team_owner_token).await;
    add_member(db, team_id, owner_id, "editor").await;
    let (_, team_code) = create_link(setup, &owner_token, Some(team_id)).await;

    (
        owner_token,
        owner_id,
        personal_code,
        solo_org_code,
        team_code,
    )
}

#[tokio::test]
async fn admin_soft_delete_takes_down_links_of_solo_owned_org() {
    let (setup, db) = spawn_real_app().await;
    let admin_token = register_admin(&setup, &db).await;
    let (_, owner_id, personal_code, solo_org_code, team_code) =
        solo_owner_fixture(&setup, &db).await;

    let (server, _) = spawn_real_app().await;
    assert_redirects(&server, &personal_code, "personal link before").await;
    assert_redirects(&server, &solo_org_code, "solo-org link before").await;

    let res = server
        .delete(&format!("/admin/users/{owner_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "admin delete: {}", res.text());

    assert_stopped(&server, &personal_code, "personal link").await;
    assert_stopped(&server, &solo_org_code, "solo-org link").await;
    // The team's links belong to the team: its owner is still here.
    assert_redirects(&server, &team_code, "link in a team the user merely joined").await;

    // Same stamp as the account, so a restore can tell this cascade apart.
    let deleted_at = user_row(&db, owner_id).await.deleted_at;
    assert!(deleted_at.is_some());
    assert_eq!(link_row(&db, &solo_org_code).await.deleted_at, deleted_at);
    assert_eq!(link_row(&db, &personal_code).await.deleted_at, deleted_at);
    assert_eq!(link_row(&db, &team_code).await.deleted_at, None);
}

#[tokio::test]
async fn self_delete_takes_down_links_of_solo_owned_org() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (setup, db) = spawn_real_app().await;
    let (owner_token, owner_id, personal_code, solo_org_code, team_code) =
        solo_owner_fixture(&setup, &db).await;

    let (server, _) = spawn_real_app().await;
    assert_redirects(&server, &solo_org_code, "solo-org link before").await;

    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&owner_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "self delete: {}", res.text());

    assert_stopped(&server, &personal_code, "personal link").await;
    assert_stopped(&server, &solo_org_code, "solo-org link").await;
    assert_redirects(&server, &team_code, "link in a team the user merely joined").await;

    let deleted_at = user_row(&db, owner_id).await.deleted_at;
    assert!(deleted_at.is_some());
    assert_eq!(link_row(&db, &solo_org_code).await.deleted_at, deleted_at);
}

#[tokio::test]
async fn restore_revives_only_the_links_the_takedown_removed() {
    let (setup, db) = spawn_real_app().await;
    let admin_token = register_admin(&setup, &db).await;
    let (owner_token, owner_id, personal_code, solo_org_code, team_code) =
        solo_owner_fixture(&setup, &db).await;
    // A second solo-org link an admin took down on its own, before the account.
    let org_id = link_row(&db, &solo_org_code).await.org_id.expect("org id");
    let (taken_down_id, taken_down_code) = create_link(&setup, &owner_token, Some(org_id)).await;

    let (server, _) = spawn_real_app().await;
    let res = server
        .delete(&format!("/admin/links/{taken_down_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "link takedown: {}", res.text());
    let earlier_stamp = link_row(&db, &taken_down_code).await.deleted_at;
    assert!(earlier_stamp.is_some());
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    let res = server
        .delete(&format!("/admin/users/{owner_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "admin delete: {}", res.text());
    assert_stopped(&server, &solo_org_code, "solo-org link").await;

    let res = server
        .post(&format!("/admin/users/{owner_id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "restore: {}", res.text());

    assert_redirects(&server, &personal_code, "restored personal link").await;
    assert_redirects(&server, &solo_org_code, "restored solo-org link").await;
    assert_redirects(&server, &team_code, "team link").await;
    assert_eq!(
        link_row(&db, &taken_down_code).await.deleted_at,
        earlier_stamp,
        "a link taken down before the account was deleted must stay deleted"
    );
    assert_stopped(&server, &taken_down_code, "earlier takedown").await;
}

#[tokio::test]
async fn restore_after_self_delete_revives_solo_org_links() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (setup, db) = spawn_real_app().await;
    let admin_token = register_admin(&setup, &db).await;
    let (owner_token, owner_id, _personal_code, solo_org_code, _team_code) =
        solo_owner_fixture(&setup, &db).await;

    let (server, _) = spawn_real_app().await;
    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&owner_token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "self delete: {}", res.text());
    assert_stopped(&server, &solo_org_code, "solo-org link").await;

    let res = server
        .post(&format!("/admin/users/{owner_id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "restore: {}", res.text());
    assert_redirects(&server, &solo_org_code, "restored solo-org link").await;
}

#[tokio::test]
async fn restore_into_a_reregistered_email_is_a_conflict_not_a_500() {
    let (setup, db) = spawn_real_app().await;
    let admin_token = register_admin(&setup, &db).await;

    let email = unique_email();
    let (_, user_id) = register(&setup, &email).await;
    let res = setup
        .delete(&format!("/admin/users/{user_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "admin delete: {}", res.text());

    // Deleting frees the address: someone else may register it.
    let (_, second_id) = register(&setup, &email).await;
    assert_ne!(second_id, user_id);

    let (server, _) = spawn_real_app().await;
    let res = server
        .post(&format!("/admin/users/{user_id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 409, "restore: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["success"], json!(false));
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("email"),
        "409 must say why: {body}"
    );

    // Rolled back cleanly: the original stays deleted, the new holder is untouched.
    assert!(user_row(&db, user_id).await.deleted_at.is_some());
    assert!(user_row(&db, second_id).await.deleted_at.is_none());
}

// ------------------------------------------------ disabled accounts stop serving

/// Fixture: a user on a blockable domain with a redirecting personal link and
/// a public bio page, plus an admin. Returns
/// (setup, db, admin_token, domain, user_id, code, bio_username).
async fn disableable_user_fixture() -> (
    axum_test::TestServer,
    DatabaseConnection,
    String,
    String,
    i32,
    String,
    String,
) {
    let (setup, db) = spawn_real_app().await;
    let admin_token = register_admin(&setup, &db).await;
    let domain = fresh_domain();
    let (user_token, user_id) = register_on_domain(&setup, &db, &domain).await;
    let username = enable_bio(&setup, &user_token).await;
    let (link_id, code) = create_link(&setup, &user_token, None).await;
    let res = setup
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&user_token)
        .json(&json!({ "bio_visible": true }))
        .await;
    assert!(
        res.status_code().is_success(),
        "bio_visible: {}",
        res.text()
    );
    (setup, db, admin_token, domain, user_id, code, username)
}

#[tokio::test]
async fn disabled_user_links_stop_redirecting_until_reenabled() {
    let (setup, db, admin_token, domain, user_id, code, _) = disableable_user_fixture().await;
    let (other_token, _) = register_verified(&setup, &db).await;
    let (_, other_code) = create_link(&setup, &other_token, None).await;

    let (server, _) = spawn_real_app().await;
    assert_redirects(&server, &code, "link before the block").await;
    assert_eq!(
        server.get(&format!("/{code}/preview")).await.status_code(),
        200,
        "preview before the block"
    );

    let block = block_email_domain(&setup, &admin_token, &domain).await;
    assert_eq!(block["affected_users"].as_u64(), Some(1), "block: {block}");
    assert!(user_row(&db, user_id).await.disabled_at.is_some());

    assert_stopped(&server, &code, "disabled user's link").await;
    // Disabling is not deleting: the row is intact so the link can come back.
    assert_eq!(link_row(&db, &code).await.deleted_at, None);
    // The preview shows the destination, so it must not outlive the redirect.
    assert_eq!(
        server.get(&format!("/{code}/preview")).await.status_code(),
        404,
        "disabled user's link preview"
    );
    assert_redirects(&server, &other_code, "an unrelated user's link").await;

    unblock_and_enable(&setup, &admin_token, &block, user_id).await;
    assert_redirects(&server, &code, "link after re-enabling").await;
    assert_eq!(
        server.get(&format!("/{code}/preview")).await.status_code(),
        200,
        "preview after re-enabling"
    );
}

#[tokio::test]
async fn disabled_user_bio_page_and_avatar_are_not_found_until_reenabled() {
    let (setup, db, admin_token, domain, user_id, _code, username) =
        disableable_user_fixture().await;
    // Unique per run: the proxy accepts any live bio holding the exact URL.
    let avatar = format!(
        "https://iana.org/takedown-avatar-{}.png",
        uuid::Uuid::new_v4().simple()
    );
    let user = user_row(&db, user_id).await;
    let mut active: users::ActiveModel = user.into();
    active.avatar_url = Set(Some(avatar.clone()));
    active.update(&db).await.expect("set avatar");

    let (server, _) = spawn_real_app().await;
    let bio_path = format!("/api/bio/{username}");
    let avatar_path = format!("/api/bio/avatar?url={}", urlencoding::encode(&avatar));
    // The proxy's own refusal. Past it, the proxy goes on to fetch the image,
    // which ends in some other status.
    let avatar_refused = || async {
        let res = server.get(&avatar_path).await;
        let refused = res.status_code() == 404 && res.text() == "Avatar not found";
        (refused, format!("{} {}", res.status_code(), res.text()))
    };

    let bio = server.get(&bio_path).await;
    assert_eq!(bio.status_code(), 200, "bio before: {}", bio.text());
    let (refused, seen) = avatar_refused().await;
    assert!(
        !refused,
        "control: a live bio's avatar is accepted by the proxy, got {seen}"
    );

    let block = block_email_domain(&setup, &admin_token, &domain).await;
    assert_eq!(block["affected_users"].as_u64(), Some(1));

    let bio = server.get(&bio_path).await;
    assert_eq!(bio.status_code(), 404, "disabled bio: {}", bio.text());
    let (refused, seen) = avatar_refused().await;
    assert!(
        refused,
        "a disabled user's avatar must not be proxied, got {seen}"
    );

    unblock_and_enable(&setup, &admin_token, &block, user_id).await;
    let bio = server.get(&bio_path).await;
    assert_eq!(
        bio.status_code(),
        200,
        "bio after re-enabling: {}",
        bio.text()
    );
    let (refused, seen) = avatar_refused().await;
    assert!(!refused, "avatar after re-enabling, got {seen}");
}

#[tokio::test]
async fn org_links_follow_the_org_owner_not_the_member_who_made_them() {
    let (setup, db) = spawn_real_app().await;
    let admin_token = register_admin(&setup, &db).await;
    let owner_domain = fresh_domain();
    let member_domain = fresh_domain();
    let (owner_token, owner_id) = register_on_domain(&setup, &db, &owner_domain).await;
    let (member_token, member_id) = register_on_domain(&setup, &db, &member_domain).await;

    let org_id = create_org(&setup, &owner_token).await;
    add_member(&db, org_id, member_id, "editor").await;
    let (_, owner_org_code) = create_link(&setup, &owner_token, Some(org_id)).await;
    let (_, member_org_code) = create_link(&setup, &member_token, Some(org_id)).await;
    let (_, member_own_code) = create_link(&setup, &member_token, None).await;

    let (server, _) = spawn_real_app().await;
    assert_redirects(&server, &member_org_code, "team link before").await;

    // A disabled ordinary member takes down their own links, not the team's.
    let member_block = block_email_domain(&setup, &admin_token, &member_domain).await;
    assert_eq!(member_block["affected_users"].as_u64(), Some(1));
    assert!(user_row(&db, member_id).await.disabled_at.is_some());
    assert_stopped(&server, &member_own_code, "disabled member's personal link").await;
    assert_redirects(
        &server,
        &member_org_code,
        "team link the disabled member made",
    )
    .await;
    assert_redirects(&server, &owner_org_code, "team link the owner made").await;

    // A disabled owner takes the whole team's links down, whoever made them.
    let owner_block = block_email_domain(&setup, &admin_token, &owner_domain).await;
    assert_eq!(owner_block["affected_users"].as_u64(), Some(1));
    assert_stopped(&server, &owner_org_code, "link of a disabled owner's org").await;
    assert_stopped(
        &server,
        &member_org_code,
        "member-made link in a disabled owner's org",
    )
    .await;
    assert_eq!(link_row(&db, &owner_org_code).await.deleted_at, None);

    unblock_and_enable(&setup, &admin_token, &owner_block, owner_id).await;
    assert_redirects(
        &server,
        &owner_org_code,
        "owner's team link after re-enabling",
    )
    .await;
    assert_redirects(
        &server,
        &member_org_code,
        "member's team link after re-enabling",
    )
    .await;
    assert_stopped(
        &server,
        &member_own_code,
        "personal link of the member who is still disabled",
    )
    .await;
}

/// Data from before accounts took their links down with them: links that are
/// still live although the creator (personal) or org owner (org) is already
/// soft-deleted. They must not serve either.
#[tokio::test]
async fn links_of_an_already_soft_deleted_owner_do_not_serve() {
    let (setup, db) = spawn_real_app().await;
    let (owner_token, owner_id) = register_verified(&setup, &db).await;
    let org_id = create_org(&setup, &owner_token).await;
    let (_, personal_code) = create_link(&setup, &owner_token, None).await;
    let (_, org_code) = create_link(&setup, &owner_token, Some(org_id)).await;

    let (server, _) = spawn_real_app().await;
    assert_redirects(&server, &personal_code, "personal link before").await;
    assert_redirects(&server, &org_code, "org link before").await;

    let user = user_row(&db, owner_id).await;
    let mut active: users::ActiveModel = user.into();
    active.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.expect("soft-delete user");

    assert_stopped(
        &server,
        &personal_code,
        "personal link of a deleted creator",
    )
    .await;
    assert_stopped(&server, &org_code, "org link of a deleted owner").await;
}

// ------------------------------------------------------- the Redis cache paths

/// The real router with a live Redis cache, or `None` when `REDIS_URL` is unset
/// (the cache-hit path is then not reachable from a test).
async fn spawn_app_with_cache()
-> Option<(axum_test::TestServer, DatabaseConnection, Arc<RedisCache>)> {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let Some(cache) = RedisCache::new().await else {
        eprintln!("REDIS_URL not set or unreachable: skipping the cache-hit checks");
        return None;
    };
    let cache = Arc::new(cache);
    let db = common::setup_test_db().await;
    let mut state = AppState::for_tests(db.clone()).await;
    state.redis_cache = Some(cache.clone());
    let server = axum_test::TestServer::new(opn_onl_backend::build_router(state))
        .expect("failed to start test server");
    Some((server, db, cache))
}

async fn is_cached(cache: &RedisCache, code: &str) -> bool {
    cache
        .get_link_versioned(code)
        .await
        .expect("redis read")
        .0
        .is_some()
}

/// A cached redirect must not outlive its owner's standing even if the
/// invalidation never reached Redis: the cache-hit path re-reads the row.
#[tokio::test]
async fn cached_redirect_of_a_disabled_owner_is_not_served() {
    let Some((app, db, cache)) = spawn_app_with_cache().await else {
        return;
    };
    let (token, user_id) = register_verified(&app, &db).await;
    let (_, code) = create_link(&app, &token, None).await;

    assert_redirects(&app, &code, "link before").await;
    assert!(
        is_cached(&cache, &code).await,
        "the first hit fills the cache"
    );

    // Disable straight in the database: no invalidation happens.
    let user = user_row(&db, user_id).await;
    let mut active: users::ActiveModel = user.into();
    active.disabled_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.expect("disable user");
    assert!(
        is_cached(&cache, &code).await,
        "the stale entry is still there"
    );

    assert_stopped(&app, &code, "cached link of a disabled owner").await;
    assert!(
        !is_cached(&cache, &code).await,
        "the stale entry is dropped once it is found out"
    );
}

#[tokio::test]
async fn blocking_an_email_domain_drops_cached_redirects_of_the_disabled() {
    let Some((app, db, cache)) = spawn_app_with_cache().await else {
        return;
    };
    let admin_token = register_admin(&app, &db).await;
    let domain = fresh_domain();
    let (owner_token, owner_id) = register_on_domain(&app, &db, &domain).await;
    let (member_token, member_id) = register_verified(&app, &db).await;
    let org_id = create_org(&app, &owner_token).await;
    add_member(&db, org_id, member_id, "editor").await;
    let (_, personal_code) = create_link(&app, &owner_token, None).await;
    let (_, org_code) = create_link(&app, &owner_token, Some(org_id)).await;
    let (_, member_made_code) = create_link(&app, &member_token, Some(org_id)).await;
    let (_, member_own_code) = create_link(&app, &member_token, None).await;

    for code in [
        &personal_code,
        &org_code,
        &member_made_code,
        &member_own_code,
    ] {
        assert_redirects(&app, code, "link before").await;
        assert!(is_cached(&cache, code).await, "{code} should be cached");
    }

    let block = block_email_domain(&app, &admin_token, &domain).await;
    assert_eq!(block["affected_users"].as_u64(), Some(1));
    assert!(user_row(&db, owner_id).await.disabled_at.is_some());

    assert!(!is_cached(&cache, &personal_code).await);
    assert!(!is_cached(&cache, &org_code).await);
    assert!(
        !is_cached(&cache, &member_made_code).await,
        "an org link answers to the org's owner, whoever made it"
    );
    assert!(
        is_cached(&cache, &member_own_code).await,
        "a link of someone who was not disabled stays cached"
    );
    assert_stopped(&app, &personal_code, "owner's personal link").await;
    assert_stopped(
        &app,
        &member_made_code,
        "member-made link in the disabled owner's org",
    )
    .await;
    assert_redirects(&app, &member_own_code, "unaffected member's link").await;
}
