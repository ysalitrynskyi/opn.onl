//! Real integration tests: import the actual application router from the
//! library crate and exercise real handlers against a real Postgres database.
//!
//! Requirements: a Postgres reachable via `DATABASE_URL` (migrations are run
//! automatically; use a throwaway database). Example:
//!
//! ```sh
//! createdb opn_test_harness
//! DATABASE_URL=postgres://localhost/opn_test_harness cargo test --test real_integration_tests
//! ```

mod common;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

/// Register a user through the real handler; returns (token, user_id).
async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(
        res.status_code(),
        201,
        "register failed for {email}: {}",
        res.text()
    );
    let body: Value = res.json();
    (
        body["token"]
            .as_str()
            .expect("token in response")
            .to_string(),
        body["user_id"].as_i64().expect("user_id in response") as i32,
    )
}

/// Create a link through the real handler; returns its (id, code).
async fn create_link(server: &axum_test::TestServer, token: &str, payload: Value) -> (i64, String) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&payload)
        .await;
    assert!(
        res.status_code().is_success(),
        "create link failed: {} {}",
        res.status_code(),
        res.text()
    );
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("link id"),
        body["code"].as_str().expect("link code").to_string(),
    )
}

/// register → login → create link → redirect: the core product flow, end to
/// end through the real router and Postgres.
#[tokio::test]
async fn register_login_create_link_redirect() {
    let (server, db) = common::spawn_real_app().await;
    let email = common::unique_email();

    let (_, user_id) = register(&server, &email).await;

    // Login with the same credentials issues a fresh token.
    let res = server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "login failed: {}", res.text());
    let token = res.json::<Value>()["token"]
        .as_str()
        .expect("token")
        .to_string();

    // Link creation requires a verified email.
    common::mark_email_verified(&db, user_id).await;

    let destination = "https://iana.org/integration-test-target";
    let (_, code) = create_link(&server, &token, json!({ "original_url": destination })).await;

    // The public redirect must send visitors to the destination.
    let res = server.get(&format!("/{code}")).await;
    assert_eq!(res.status_code(), 307, "redirect status: {}", res.text());
    assert_eq!(
        res.headers()
            .get("location")
            .expect("location header")
            .to_str()
            .unwrap(),
        destination
    );
}

/// Ownership / IDOR: user B must not be able to update or delete user A's
/// link, and A's link must keep working afterwards.
#[tokio::test]
async fn user_cannot_modify_another_users_link() {
    let (server, db) = common::spawn_real_app().await;

    let (token_a, user_a) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_a).await;
    let destination = "https://iana.org/owned-by-a";
    let (link_id, code) =
        create_link(&server, &token_a, json!({ "original_url": destination })).await;

    let (token_b, _) = register(&server, &common::unique_email()).await;

    // B tries to hijack A's link.
    let res = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&token_b)
        .json(&json!({ "original_url": "https://evil.iana.org" }))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "update as non-owner: {}",
        res.text()
    );

    // B tries to delete A's link.
    let res = server
        .delete(&format!("/links/{link_id}"))
        .authorization_bearer(&token_b)
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "delete as non-owner: {}",
        res.text()
    );

    // A's link is untouched and still redirects to the original destination.
    let res = server.get(&format!("/{code}")).await;
    assert_eq!(res.status_code(), 307);
    assert_eq!(
        res.headers().get("location").unwrap().to_str().unwrap(),
        destination
    );
}

#[tokio::test]
async fn deleted_link_cannot_be_updated() {
    let (server, db) = common::spawn_real_app().await;

    let (token, user_id) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_id).await;
    let (link_id, _code) = create_link(
        &server,
        &token,
        json!({ "original_url": "https://iana.org/deleted" }),
    )
    .await;

    let res = server
        .delete(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "delete link: {}", res.text());

    let res = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/revived" }))
        .await;
    assert_eq!(
        res.status_code(),
        404,
        "soft-deleted link must not be updateable: {}",
        res.text()
    );
}

/// bulk_delete_links skips rows with deleted_at set. bulk_update_links must
/// do the same: mutating a soft-deleted row would survive a later restore.
#[tokio::test]
async fn bulk_update_skips_soft_deleted_links() {
    let (server, db) = common::spawn_real_app().await;

    let (token, user_id) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_id).await;
    let (link_id, _code) = create_link(
        &server,
        &token,
        json!({ "original_url": "https://iana.org/bulk-update-deleted" }),
    )
    .await;

    let res = server
        .delete(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "delete link: {}", res.text());

    let expires = (Utc::now() + Duration::hours(2)).to_rfc3339();
    let res = server
        .post("/links/bulk/update")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [link_id], "expires_at": expires }))
        .await;
    assert_eq!(res.status_code(), 200, "bulk update: {}", res.text());
    assert_eq!(
        res.json::<Value>()["updated"],
        0,
        "soft-deleted link must not be bulk-updated"
    );

    use opn_onl_backend::entity::links;
    use sea_orm::EntityTrait;
    let stored = links::Entity::find_by_id(link_id as i32)
        .one(&db)
        .await
        .expect("db")
        .expect("link row");
    assert!(stored.deleted_at.is_some(), "row must stay soft-deleted");
    assert!(
        stored.expires_at.is_none(),
        "deleted row expiry must be unchanged: {:?}",
        stored.expires_at
    );
}

/// Uncapped clicks sit in the in-memory buffer. Adding max_clicks later must
/// count those pending clicks against the new cap, or consume_capped_click
/// will hand out extra redirects and flush will overshoot.
#[tokio::test]
async fn adding_click_cap_counts_unflushed_buffer_clicks() {
    let (server, db) = common::spawn_real_app().await;

    let (token, user_id) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_id).await;
    let (link_id, code) = create_link(
        &server,
        &token,
        json!({ "original_url": "https://iana.org/cap-overshoot" }),
    )
    .await;

    for i in 0..3 {
        let res = server.get(&format!("/{code}")).await;
        assert_eq!(
            res.status_code(),
            307,
            "uncapped click {i} must redirect: {}",
            res.text()
        );
    }

    let res = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .json(&json!({ "max_clicks": 5 }))
        .await;
    assert_eq!(res.status_code(), 200, "set max_clicks: {}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["click_count"], 3,
        "pending buffer clicks must fold into click_count when a cap is added: {body}"
    );
    assert_eq!(body["max_clicks"], 5);

    let mut extra_redirects = 0u32;
    let mut gone = 0u32;
    for i in 0..10 {
        let res = server.get(&format!("/{code}")).await;
        match res.status_code().as_u16() {
            307 => extra_redirects += 1,
            410 => gone += 1,
            other => panic!("click {i} after cap: {other} {}", res.text()),
        }
    }

    assert_eq!(
        extra_redirects, 2,
        "3 buffered + 2 new = cap 5; extra redirects were {extra_redirects} (gone={gone})"
    );
    assert_eq!(gone, 8, "remaining clicks after the cap must be 410");
}

/// Two creates can both pass the pre-insert alias lookup. The unique index
/// still rejects the loser; that must be 409 "Alias already taken", not 500.
#[tokio::test]
async fn custom_alias_unique_violation_is_conflict() {
    let (server, db) = common::spawn_real_app().await;
    let (token, user_id) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_id).await;

    let alias = common::unique_code();

    use opn_onl_backend::entity::links;
    use sea_orm::{ActiveModelTrait, Set, TransactionTrait};

    // Hold an uncommitted row with this code so the handler's existence check
    // (READ COMMITTED) misses it and the INSERT waits on the unique index.
    let txn = db.begin().await.expect("begin");
    links::ActiveModel {
        original_url: Set("https://iana.org/held-alias".to_string()),
        code: Set(alias.clone()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("hold unique code");

    let create_fut = server.post("/links").authorization_bearer(&token).json(
        &json!({ "original_url": "https://iana.org/racer-alias", "custom_alias": alias }),
    );
    let commit_fut = async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        txn.commit().await.expect("commit held alias");
    };
    let (res, _) = tokio::join!(create_fut, commit_fut);

    assert_eq!(
        res.status_code(),
        409,
        "unique alias race must be 409, got {}: {}",
        res.status_code(),
        res.text()
    );
    assert_eq!(
        res.json::<Value>()["error"],
        "Alias already taken",
        "loser must get the documented alias conflict: {}",
        res.text()
    );
}

/// Regression (account takeover, fixed in 5240b6a): passkey enrollment must
/// require authentication — knowing a victim's email must not be enough to
/// start registering an authenticator onto their account.
#[tokio::test]
async fn passkey_register_start_requires_auth() {
    let (server, _db) = common::spawn_real_app().await;

    let res = server
        .post("/auth/passkey/register/start")
        .json(&json!({ "username": "victim@iana.org" }))
        .await;
    assert_eq!(
        res.status_code(),
        401,
        "unauthenticated passkey enrollment must be rejected: {}",
        res.text()
    );
}

/// Regression (destination leak, fixed in 5240b6a): the public preview of a
/// password-protected link must not disclose the destination URL.
#[tokio::test]
async fn preview_hides_password_protected_destination() {
    let (server, db) = common::spawn_real_app().await;

    let (token, user_id) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_id).await;

    let secret_destination = "https://iana.org/secret-destination";
    let (_, protected_code) = create_link(
        &server,
        &token,
        json!({ "original_url": secret_destination, "password": "hunter2!" }),
    )
    .await;

    let res = server.get(&format!("/{protected_code}/preview")).await;
    assert_eq!(res.status_code(), 200, "preview: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["has_password"], true);
    assert_eq!(
        body["original_url"], "",
        "protected preview must not leak the destination: {body}"
    );

    // Control: a plain link's preview does show its destination.
    let plain_destination = "https://iana.org/plain-destination";
    let (_, plain_code) = create_link(
        &server,
        &token,
        json!({ "original_url": plain_destination }),
    )
    .await;
    let res = server.get(&format!("/{plain_code}/preview")).await;
    assert_eq!(res.status_code(), 200);
    assert_eq!(res.json::<Value>()["original_url"], plain_destination);
}

/// A not-yet-live scheduled link 410s on redirect; the public preview must
/// not leak the embargoed destination in the meantime.
#[tokio::test]
async fn preview_hides_scheduled_link_destination() {
    let (server, db) = common::spawn_real_app().await;

    let (token, user_id) = register(&server, &common::unique_email()).await;
    common::mark_email_verified(&db, user_id).await;

    let secret_destination = "https://iana.org/embargoed-destination";
    let starts_at = (Utc::now() + Duration::hours(1)).to_rfc3339();
    let (_, code) = create_link(
        &server,
        &token,
        json!({
            "original_url": secret_destination,
            "starts_at": starts_at,
        }),
    )
    .await;

    let redirect = server.get(&format!("/{code}")).await;
    assert_eq!(
        redirect.status_code(),
        410,
        "scheduled redirect must be 410: {}",
        redirect.text()
    );
    assert!(
        redirect.text().contains("scheduled to activate later"),
        "redirect body should explain the schedule: {}",
        redirect.text()
    );

    let res = server.get(&format!("/{code}/preview")).await;
    assert_eq!(res.status_code(), 200, "preview: {}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["original_url"], "",
        "scheduled preview must not leak the destination: {body}"
    );
    assert_eq!(
        body["domain"], "",
        "scheduled preview must not leak the destination host: {body}"
    );
    assert_eq!(
        body["is_active"], false,
        "scheduled preview must surface that the link is not yet active: {body}"
    );
}

/// The real /health endpoint reports a healthy database through the real
/// router (replaces the old stub-router "health" tests).
#[tokio::test]
async fn health_reports_database_connected() {
    let (server, _db) = common::spawn_real_app().await;

    let res = server.get("/health").await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["database"], "connected");
}
