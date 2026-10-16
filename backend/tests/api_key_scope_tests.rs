//! API key scope and expiry, against the real router and Postgres.
//!
//! A read-only key is refused in the router middleware, so a write handler
//! cannot forget the check. An expired key answers 401, the same as an
//! unknown key, and does not move `last_used_at`. A failed lookup on a
//! write does not fall through: the handler would authenticate the key on
//! its own and perform the write.
//!
//! Each test stays under the per-IP limit of 10 requests a second.

mod common;

use axum_test::TestServer;
use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::api_keys;
use opn_onl_backend::handlers::links::{
    ApiKeyMiddlewareFault, ApiKeyMiddlewareFaultGuard, hash_api_key,
};
use opn_onl_backend::utils::time::utc_rfc3339;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};

const PASSWORD: &str = "password123";

struct Account {
    token: String,
    id: i32,
}

async fn register(server: &TestServer, db: &DatabaseConnection) -> Account {
    common::set_env("ENABLE_API_KEYS", "true");
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let id = body["user_id"].as_i64().expect("user id") as i32;
    mark_email_verified(db, id).await;
    Account {
        token: body["token"].as_str().expect("token").to_string(),
        id,
    }
}

async fn create_key(server: &TestServer, token: &str, body: Value) -> axum_test::TestResponse {
    server
        .post("/auth/api-keys")
        .authorization_bearer(token)
        .json(&body)
        .await
}

fn key_secret(body: &Value) -> String {
    body["key"].as_str().expect("raw key").to_string()
}

fn assert_read_only(res: axum_test::TestResponse) {
    assert_eq!(res.status_code(), 403, "{}", res.text());
    assert_eq!(res.json::<Value>()["error"], "This API key is read-only");
}

fn unique_url(tag: &str) -> String {
    format!(
        "https://iana.org/api-key-scope-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    )
}

async fn create_link(server: &TestServer, token: &str) -> i64 {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": unique_url("link") }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["id"].as_i64().expect("link id")
}

#[tokio::test]
async fn read_only_key_can_list_get_and_read_stats() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let link_id = create_link(&server, &account.token).await;
    let created = create_key(
        &server,
        &account.token,
        json!({ "name": "reader", "scope": "read_only" }),
    )
    .await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    let created: Value = created.json();
    assert_eq!(created["scope"], "read_only");
    assert!(created["expires_at"].is_null());
    assert_eq!(created["expired"], false);
    let key = key_secret(&created);

    let listed = server.get("/links").authorization_bearer(&key).await;
    assert_eq!(listed.status_code(), 200, "{}", listed.text());
    let ids: Vec<i64> = listed
        .json::<Value>()
        .as_array()
        .expect("link list")
        .iter()
        .map(|link| link["id"].as_i64().expect("id"))
        .collect();
    assert!(ids.contains(&link_id));

    let one = server
        .get(&format!("/links/{link_id}"))
        .authorization_bearer(&key)
        .await;
    assert_eq!(one.status_code(), 200, "{}", one.text());
    assert_eq!(one.json::<Value>()["id"], link_id);

    let stats = server
        .get(&format!("/links/{link_id}/stats"))
        .authorization_bearer(&key)
        .await;
    assert_eq!(stats.status_code(), 200, "{}", stats.text());
}

#[tokio::test]
async fn read_only_key_cannot_create_or_change_links_full_key_can() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let link_id = create_link(&server, &account.token).await;
    let read_only = create_key(
        &server,
        &account.token,
        json!({ "name": "reader", "scope": "read_only" }),
    )
    .await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());
    let full = create_key(&server, &account.token, json!({ "name": "writer" })).await;
    assert_eq!(full.status_code(), 201, "{}", full.text());
    let full = key_secret(&full.json());

    assert_read_only(
        server
            .post("/links")
            .authorization_bearer(&read_only)
            .json(&json!({ "original_url": unique_url("denied") }))
            .await,
    );
    let created = server
        .post("/links")
        .authorization_bearer(&full)
        .json(&json!({ "original_url": unique_url("allowed") }))
        .await;
    assert_eq!(created.status_code(), 201, "{}", created.text());

    assert_read_only(
        server
            .put(&format!("/links/{link_id}"))
            .authorization_bearer(&read_only)
            .json(&json!({ "title": "nope" }))
            .await,
    );
    let updated = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&full)
        .json(&json!({ "title": "renamed" }))
        .await;
    assert_eq!(updated.status_code(), 200, "{}", updated.text());
    assert_eq!(updated.json::<Value>()["title"], "renamed");
}

#[tokio::test]
async fn read_only_key_cannot_delete_or_bulk_create_full_key_can() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let link_id = create_link(&server, &account.token).await;
    let read_only = create_key(&server, &account.token, json!({ "scope": "read_only" })).await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());
    let full = create_key(&server, &account.token, json!({})).await;
    assert_eq!(full.status_code(), 201, "{}", full.text());
    let full = key_secret(&full.json());

    assert_read_only(
        server
            .delete(&format!("/links/{link_id}"))
            .authorization_bearer(&read_only)
            .await,
    );
    let deleted = server
        .delete(&format!("/links/{link_id}"))
        .authorization_bearer(&full)
        .await;
    assert_eq!(deleted.status_code(), 200, "{}", deleted.text());

    assert_read_only(
        server
            .post("/links/bulk")
            .authorization_bearer(&read_only)
            .json(&json!({ "urls": [unique_url("bulk-denied")] }))
            .await,
    );
    let bulk = server
        .post("/links/bulk")
        .authorization_bearer(&full)
        .json(&json!({ "urls": [unique_url("bulk-ok")] }))
        .await;
    assert_eq!(bulk.status_code(), 200, "{}", bulk.text());
    let bulk: Value = bulk.json();
    assert_eq!(bulk["links"].as_array().expect("links").len(), 1);
    assert!(bulk["errors"].as_array().expect("errors").is_empty());
}

#[tokio::test]
async fn read_only_key_cannot_write_folders_tags_or_orgs_full_key_can() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let read_only = create_key(&server, &account.token, json!({ "scope": "read_only" })).await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());
    let full = create_key(&server, &account.token, json!({ "scope": "full" })).await;
    assert_eq!(full.status_code(), 201, "{}", full.text());
    let full = key_secret(&full.json());

    assert_read_only(
        server
            .post("/folders")
            .authorization_bearer(&read_only)
            .json(&json!({ "name": "Denied folder" }))
            .await,
    );
    let folder = server
        .post("/folders")
        .authorization_bearer(&full)
        .json(&json!({ "name": "Allowed folder" }))
        .await;
    assert_eq!(folder.status_code(), 201, "{}", folder.text());

    assert_read_only(
        server
            .post("/tags")
            .authorization_bearer(&read_only)
            .json(&json!({ "name": "denied-tag" }))
            .await,
    );
    let tag = server
        .post("/tags")
        .authorization_bearer(&full)
        .json(&json!({ "name": "allowed-tag" }))
        .await;
    assert_eq!(tag.status_code(), 201, "{}", tag.text());

    let slug = format!("s{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    assert_read_only(
        server
            .post("/orgs")
            .authorization_bearer(&read_only)
            .json(&json!({ "name": "Denied org", "slug": format!("{slug}x") }))
            .await,
    );
    let org = server
        .post("/orgs")
        .authorization_bearer(&full)
        .json(&json!({ "name": "Allowed org", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "{}", org.text());
}

#[tokio::test]
async fn expired_key_is_unauthorized_and_does_not_refresh_last_used() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let raw = format!("opn_{}", uuid::Uuid::new_v4().simple());
    let stamped = chrono::Utc::now().naive_utc() - chrono::Duration::hours(2);
    let key_id = api_keys::ActiveModel {
        user_id: Set(account.id),
        name: Set("expired".to_string()),
        key_hash: Set(hash_api_key(&raw)),
        key_prefix: Set(raw.chars().take(12).collect()),
        // Read-only as well as expired: a write must be 401, not the
        // read-only 403, because the key no longer authenticates.
        scope: Set("read_only".to_string()),
        expires_at: Set(Some(
            chrono::Utc::now().naive_utc() - chrono::Duration::minutes(5),
        )),
        last_used_at: Set(Some(stamped)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert expired key")
    .id;
    let before = api_keys::Entity::find_by_id(key_id)
        .one(&db)
        .await
        .expect("db")
        .expect("key");

    let listed = server.get("/links").authorization_bearer(&raw).await;
    assert_eq!(listed.status_code(), 401, "{}", listed.text());
    assert_eq!(listed.json::<Value>()["error"], "Unauthorized");

    let write = server
        .post("/links")
        .authorization_bearer(&raw)
        .json(&json!({ "original_url": unique_url("expired") }))
        .await;
    assert_eq!(write.status_code(), 401, "{}", write.text());
    assert_eq!(
        write.json::<Value>()["error"],
        "Invalid or expired credentials"
    );

    let after = api_keys::Entity::find_by_id(key_id)
        .one(&db)
        .await
        .expect("db")
        .expect("key");
    assert_eq!(after.last_used_at, before.last_used_at);

    let mine = server
        .get("/auth/api-keys")
        .authorization_bearer(&account.token)
        .await;
    assert_eq!(mine.status_code(), 200, "{}", mine.text());
    let mine: Value = mine.json();
    let row = mine
        .as_array()
        .expect("key list")
        .iter()
        .find(|key| key["id"].as_i64() == Some(key_id as i64))
        .expect("expired key is listed");
    assert_eq!(row["scope"], "read_only");
    assert_eq!(row["expired"], true);
    let expires_at = row["expires_at"].as_str().expect("expires_at");
    assert!(expires_at.ends_with('Z'), "{expires_at}");
    assert_eq!(
        row["last_used_at"].as_str(),
        before.last_used_at.map(utc_rfc3339).as_deref()
    );
}

#[tokio::test]
async fn future_expiry_works_and_invalid_expiry_or_scope_is_rejected() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;

    let created = create_key(
        &server,
        &account.token,
        json!({ "name": "month", "scope": "read_only", "expires_in_days": 30 }),
    )
    .await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    let created: Value = created.json();
    assert_eq!(created["scope"], "read_only");
    assert_eq!(created["expired"], false);
    let expires_at = created["expires_at"].as_str().expect("expires_at");
    assert!(expires_at.ends_with('Z'), "{expires_at}");
    let when = chrono::DateTime::parse_from_rfc3339(expires_at).expect("rfc3339");
    let ahead = when.signed_duration_since(chrono::Utc::now());
    assert!(
        (29 * 24..31 * 24).contains(&ahead.num_hours()),
        "expected about 30 days ahead, got {ahead}"
    );
    let key = key_secret(&created);
    let key_id = created["id"].as_i64().expect("id") as i32;

    let read = server.get("/links").authorization_bearer(&key).await;
    assert_eq!(read.status_code(), 200, "{}", read.text());
    assert_read_only(
        server
            .post("/links")
            .authorization_bearer(&key)
            .json(&json!({ "original_url": unique_url("future-denied") }))
            .await,
    );
    let used = api_keys::Entity::find_by_id(key_id)
        .one(&db)
        .await
        .expect("db")
        .expect("key");
    assert!(
        used.last_used_at.is_some(),
        "a key that still works records that it was used"
    );

    for days in [0_i64, 731, -3] {
        let res = create_key(&server, &account.token, json!({ "expires_in_days": days })).await;
        assert_eq!(res.status_code(), 400, "days {days}: {}", res.text());
        assert_eq!(
            res.json::<Value>()["error"],
            "Expiry must be between 1 and 730 days"
        );
    }
    let bad_scope = create_key(&server, &account.token, json!({ "scope": "admin" })).await;
    assert_eq!(bad_scope.status_code(), 400, "{}", bad_scope.text());
    assert_eq!(
        bad_scope.json::<Value>()["error"],
        "Scope must be full or read_only"
    );

    let plain = create_key(&server, &account.token, json!({ "name": "plain" })).await;
    assert_eq!(plain.status_code(), 201, "{}", plain.text());
    let plain: Value = plain.json();
    assert_eq!(plain["scope"], "full");
    assert!(plain["expires_at"].is_null());
    assert_eq!(plain["expired"], false);

    let stored = api_keys::Entity::find()
        .filter(api_keys::Column::UserId.eq(account.id))
        .all(&db)
        .await
        .expect("db");
    assert_eq!(stored.len(), 2, "rejected creates must not insert a key");
}

#[tokio::test]
async fn jwt_only_endpoints_refuse_api_keys() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let full = create_key(&server, &account.token, json!({ "scope": "full" })).await;
    assert_eq!(full.status_code(), 201, "{}", full.text());
    let full = key_secret(&full.json());
    let read_only = create_key(&server, &account.token, json!({ "scope": "read_only" })).await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());

    for key in [&full, &read_only] {
        let listed = server.get("/auth/api-keys").authorization_bearer(key).await;
        assert_eq!(listed.status_code(), 401, "{}", listed.text());
        let passkeys = server.get("/auth/passkeys").authorization_bearer(key).await;
        assert_eq!(passkeys.status_code(), 401, "{}", passkeys.text());
    }

    let change = server
        .post("/auth/change-password")
        .authorization_bearer(&full)
        .json(&json!({
            "current_password": PASSWORD,
            "new_password": "newpassword123"
        }))
        .await;
    assert_eq!(change.status_code(), 401, "{}", change.text());

    // A write, so the read-only middleware refuses it before the JWT check.
    assert_read_only(
        server
            .post("/auth/change-password")
            .authorization_bearer(&read_only)
            .json(&json!({
                "current_password": PASSWORD,
                "new_password": "newpassword123"
            }))
            .await,
    );
}

async fn post_link(server: &TestServer, token: &str, url: &str) -> axum_test::TestResponse {
    server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await
}

async fn links_with_url(db: &DatabaseConnection, user_id: i32, url: &str) -> u64 {
    use opn_onl_backend::entity::links;
    links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::OriginalUrl.eq(url))
        .count(db)
        .await
        .expect("count links")
}

#[tokio::test]
async fn read_only_write_does_not_run_when_key_lookup_fails() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let read_only = create_key(&server, &account.token, json!({ "scope": "read_only" })).await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());
    let full = create_key(&server, &account.token, json!({ "scope": "full" })).await;
    assert_eq!(full.status_code(), 201, "{}", full.text());
    let full = key_secret(&full.json());

    let _read_only_fault =
        ApiKeyMiddlewareFaultGuard::arm(&read_only, ApiKeyMiddlewareFault::KeyLookup);

    // The fault is the write gate only. A read still resolves the key.
    let listed = server.get("/links").authorization_bearer(&read_only).await;
    assert_eq!(listed.status_code(), 200, "{}", listed.text());

    let allowed = unique_url("lookup-ok");
    let created = post_link(&server, &full, &allowed).await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    assert_eq!(links_with_url(&db, account.id, &allowed).await, 1);

    let denied = unique_url("lookup-denied");
    let refused = post_link(&server, &read_only, &denied).await;
    assert_eq!(refused.status_code(), 503, "{}", refused.text());
    assert_eq!(
        refused.json::<Value>()["error"],
        "Unable to verify credentials"
    );
    assert_eq!(
        links_with_url(&db, account.id, &denied).await,
        0,
        "a failed key lookup must not create the link"
    );

    // Same failure for a full key: the gate does not know the scope yet.
    let _full_fault = ApiKeyMiddlewareFaultGuard::arm(&full, ApiKeyMiddlewareFault::KeyLookup);
    let full_denied = unique_url("lookup-full-denied");
    let full_refused = post_link(&server, &full, &full_denied).await;
    assert_eq!(full_refused.status_code(), 503, "{}", full_refused.text());
    assert_eq!(
        full_refused.json::<Value>()["error"],
        "Unable to verify credentials"
    );
    assert_eq!(links_with_url(&db, account.id, &full_denied).await, 0);
    drop(_full_fault);
    drop(_read_only_fault);

    let still_denied = unique_url("lookup-still-denied");
    assert_read_only(post_link(&server, &read_only, &still_denied).await);
    assert_eq!(links_with_url(&db, account.id, &still_denied).await, 0);
}

#[tokio::test]
async fn read_only_write_is_refused_when_owner_lookup_fails() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let read_only = create_key(&server, &account.token, json!({ "scope": "read_only" })).await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());
    let full = create_key(&server, &account.token, json!({ "scope": "full" })).await;
    assert_eq!(full.status_code(), 201, "{}", full.text());
    let full = key_secret(&full.json());

    let allowed = unique_url("owner-ok");
    let created = post_link(&server, &full, &allowed).await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    assert_eq!(links_with_url(&db, account.id, &allowed).await, 1);

    let _read_only_fault =
        ApiKeyMiddlewareFaultGuard::arm(&read_only, ApiKeyMiddlewareFault::OwnerLookup);
    let denied = unique_url("owner-denied");
    let refused = post_link(&server, &read_only, &denied).await;
    assert_eq!(refused.status_code(), 403, "{}", refused.text());
    assert_eq!(
        refused.json::<Value>()["error"],
        "This API key is read-only"
    );
    assert_eq!(
        links_with_url(&db, account.id, &denied).await,
        0,
        "an owner-lookup error must not let a read-only key write"
    );
    drop(_read_only_fault);

    // The key row was not read-only, so an owner-lookup error cannot be a
    // 403. It is still a failed check, and the handler must not run.
    let _full_fault = ApiKeyMiddlewareFaultGuard::arm(&full, ApiKeyMiddlewareFault::OwnerLookup);
    let full_denied = unique_url("owner-full-denied");
    let full_refused = post_link(&server, &full, &full_denied).await;
    assert_eq!(full_refused.status_code(), 503, "{}", full_refused.text());
    assert_eq!(
        full_refused.json::<Value>()["error"],
        "Unable to verify credentials"
    );
    assert_eq!(links_with_url(&db, account.id, &full_denied).await, 0);
}

#[tokio::test]
async fn read_only_key_of_a_deleted_owner_falls_through_to_unauthorized() {
    let (server, db) = spawn_real_app().await;
    let account = register(&server, &db).await;
    let read_only = create_key(&server, &account.token, json!({ "scope": "read_only" })).await;
    assert_eq!(read_only.status_code(), 201, "{}", read_only.text());
    let read_only = key_secret(&read_only.json());

    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(account.id)
        .one(&db)
        .await
        .expect("db")
        .expect("user");
    let mut active: users::ActiveModel = user.into();
    active.deleted_at = Set(Some(chrono::Utc::now().naive_utc()));
    active.update(&db).await.expect("soft-delete owner");

    let denied = unique_url("deleted-owner");
    let write = post_link(&server, &read_only, &denied).await;
    assert_eq!(write.status_code(), 401, "{}", write.text());
    assert_eq!(
        write.json::<Value>()["error"],
        "Invalid or expired credentials"
    );
    assert_eq!(links_with_url(&db, account.id, &denied).await, 0);
}
