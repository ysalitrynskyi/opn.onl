//! Admin endpoint integration tests: real router, real Postgres.
//!
//! Requirements: a Postgres reachable via `DATABASE_URL` (migrations are run
//! automatically; use a throwaway database).

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
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

/// Flip `is_admin` directly in the database. Tests share one database, so the
/// "first user becomes admin" bootstrap can't be relied on here.
async fn make_admin(db: &DatabaseConnection, user_id: i32) {
    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db error")
        .expect("user not found");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("failed to promote user");
}

/// Register a fresh admin: a normal registration plus a direct DB promotion.
async fn register_admin(server: &axum_test::TestServer, db: &DatabaseConnection) -> (String, i32) {
    let (token, user_id) = register(server, &unique_email()).await;
    make_admin(db, user_id).await;
    (token, user_id)
}

/// Register a verified regular user (link creation requires a verified email).
async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, String) {
    let email = unique_email();
    let (token, user_id) = register(server, &email).await;
    mark_email_verified(db, user_id).await;
    (token, user_id, email)
}

/// Create a link through the real handler; returns (id, code).
async fn create_link(server: &axum_test::TestServer, token: &str, url: &str) -> (i64, String) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await;
    assert_eq!(res.status_code(), 201, "create link failed: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("link id"),
        body["code"].as_str().expect("link code").to_string(),
    )
}

#[tokio::test]
async fn non_admin_and_anonymous_are_rejected() {
    let (server, db) = spawn_real_app().await;
    let (user_token, _, _) = register_verified(&server, &db).await;

    for path in [
        "/admin/stats",
        "/admin/activity",
        "/admin/users",
        "/admin/links",
        "/admin/orgs",
    ] {
        // Use a fresh app (fresh rate limiters) per endpoint. This test exercises
        // authorization, not rate limiting: checking five endpoints ×2 requests
        // against one app would share the 10/sec per-IP bucket and spuriously
        // return 429 (all test requests share the "unknown" IP key), which was a
        // machine-speed-dependent flake. The JWT is valid against any app on the
        // same DB/secret.
        let (server, _) = spawn_real_app().await;

        let res = server.get(path).await;
        assert_eq!(res.status_code(), 401, "anonymous {path} should be 401");

        let res = server.get(path).authorization_bearer(&user_token).await;
        assert_eq!(res.status_code(), 403, "non-admin {path} should be 403");
    }
}

#[tokio::test]
async fn admin_sees_other_users_links_with_owner_data() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, user_id, user_email) = register_verified(&server, &db).await;

    let (link_id, code) = create_link(&server, &user_token, "https://example.com/owned").await;

    let res = server
        .get("/admin/links")
        .add_query_param("search", &code)
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();

    assert_eq!(body["total"].as_u64(), Some(1));
    let link = &body["links"][0];
    assert_eq!(link["id"].as_i64(), Some(link_id));
    assert_eq!(link["code"].as_str(), Some(code.as_str()));
    assert_eq!(link["user_id"].as_i64(), Some(user_id as i64));
    assert_eq!(link["user_email"].as_str(), Some(user_email.as_str()));
    assert_eq!(
        link["original_url"].as_str(),
        Some("https://example.com/owned")
    );
    assert_eq!(link["is_active"].as_bool(), Some(true));
    assert_eq!(link["has_password"].as_bool(), Some(false));
    assert!(link["deleted_at"].is_null());
}

#[tokio::test]
async fn admin_links_support_pagination_and_user_filter() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, user_id, _) = register_verified(&server, &db).await;

    for i in 0..3 {
        create_link(
            &server,
            &user_token,
            &format!("https://example.com/page-{i}"),
        )
        .await;
    }

    let res = server
        .get("/admin/links")
        .add_query_param("user_id", user_id.to_string())
        .add_query_param("per_page", "2")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();
    assert_eq!(body["total"].as_u64(), Some(3));
    assert_eq!(body["links"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["per_page"].as_u64(), Some(2));

    let res = server
        .get("/admin/links")
        .add_query_param("user_id", user_id.to_string())
        .add_query_param("per_page", "2")
        .add_query_param("page", "2")
        .authorization_bearer(&admin_token)
        .await;
    let body: Value = res.json();
    assert_eq!(body["links"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["page"].as_u64(), Some(2));
}

#[tokio::test]
async fn admin_can_delete_and_restore_any_link() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, _, _) = register_verified(&server, &db).await;

    let (link_id, code) = create_link(&server, &user_token, "https://example.com/takedown").await;

    // Redirect works while the link is live.
    let res = server.get(&format!("/{code}")).await;
    assert!(
        res.status_code().is_redirection(),
        "expected redirect, got {}",
        res.status_code()
    );

    // Admin takes it down.
    let res = server
        .delete(&format!("/admin/links/{link_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());

    let res = server.get(&format!("/{code}")).await;
    assert_eq!(res.status_code(), 404, "deleted link must stop redirecting");

    // Deleting again is a 400, not a silent success.
    let res = server
        .delete(&format!("/admin/links/{link_id}"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 400);

    // It shows up under the deleted filter.
    let res = server
        .get("/admin/links")
        .add_query_param("search", &code)
        .add_query_param("status", "deleted")
        .authorization_bearer(&admin_token)
        .await;
    let body: Value = res.json();
    assert_eq!(body["total"].as_u64(), Some(1));
    assert!(!body["links"][0]["deleted_at"].is_null());

    // Restore brings the redirect back.
    let res = server
        .post(&format!("/admin/links/{link_id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());

    let res = server.get(&format!("/{code}")).await;
    assert!(
        res.status_code().is_redirection(),
        "restored link must redirect again, got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn admin_users_list_is_paginated_with_aggregates() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, user_id, user_email) = register_verified(&server, &db).await;

    create_link(&server, &user_token, "https://example.com/counted-1").await;
    create_link(&server, &user_token, "https://example.com/counted-2").await;

    let res = server
        .get("/admin/users")
        .add_query_param("search", &user_email)
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();

    assert_eq!(body["total"].as_u64(), Some(1));
    let user = &body["users"][0];
    assert_eq!(user["id"].as_i64(), Some(user_id as i64));
    assert_eq!(user["email"].as_str(), Some(user_email.as_str()));
    assert_eq!(user["links_count"].as_i64(), Some(2));
    assert_eq!(user["total_clicks"].as_i64(), Some(0));
    assert_eq!(user["email_verified"].as_bool(), Some(true));
    assert_eq!(user["is_admin"].as_bool(), Some(false));
    assert!(user["api_keys_count"].is_i64());
    assert!(user["passkeys_count"].is_i64());
    assert!(user["orgs_owned"].is_i64());
}

#[tokio::test]
async fn admin_users_status_filter_finds_unverified() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let email = unique_email();
    let (_, user_id) = register(&server, &email).await;

    let res = server
        .get("/admin/users")
        .add_query_param("search", &email)
        .add_query_param("status", "unverified")
        .authorization_bearer(&admin_token)
        .await;
    let body: Value = res.json();
    assert_eq!(body["total"].as_u64(), Some(1));
    assert_eq!(body["users"][0]["id"].as_i64(), Some(user_id as i64));

    // The same user disappears from the "admins" filter.
    let res = server
        .get("/admin/users")
        .add_query_param("search", &email)
        .add_query_param("status", "admins")
        .authorization_bearer(&admin_token)
        .await;
    let body: Value = res.json();
    assert_eq!(body["total"].as_u64(), Some(0));
}

#[tokio::test]
async fn admin_can_force_verify_email() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, user_id) = register(&server, &unique_email()).await;

    // Unverified users cannot create links.
    let res = server
        .post("/links")
        .authorization_bearer(&user_token)
        .json(&json!({ "original_url": "https://example.com/blocked" }))
        .await;
    assert_ne!(
        res.status_code(),
        201,
        "unverified user should not create links"
    );

    let res = server
        .post(&format!("/admin/users/{user_id}/verify-email"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());

    // Verifying twice is a 400.
    let res = server
        .post(&format!("/admin/users/{user_id}/verify-email"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 400);

    // Now link creation works.
    let res = server
        .post("/links")
        .authorization_bearer(&user_token)
        .json(&json!({ "original_url": "https://example.com/now-allowed" }))
        .await;
    assert_eq!(res.status_code(), 201, "{}", res.text());
}

#[tokio::test]
async fn admin_stats_have_full_shape() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, _, _) = register_verified(&server, &db).await;
    create_link(&server, &user_token, "https://example.com/for-stats").await;

    let res = server
        .get("/admin/stats")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();

    for field in [
        "total_users",
        "active_users",
        "verified_users",
        "admin_users",
        "total_links",
        "active_links",
        "total_clicks",
        "total_orgs",
        "users_today",
        "links_today",
        "clicks_today",
        "blocked_links_count",
        "blocked_domains_count",
    ] {
        assert!(
            body[field].as_i64().is_some_and(|v| v >= 0),
            "field {field} missing or negative: {body}"
        );
    }
    // Note: we deliberately do NOT assert total_users >= active_users. Those are
    // two independently-computed global counts, and the suite runs in parallel
    // against one shared database, so comparing them is a read-skew flake. The
    // shape checks above cover both fields. The >=1 checks below are safe: they
    // are monotonic lower bounds from this test's own admin + link.
    assert!(body["total_links"].as_i64().unwrap() >= 1);
    assert!(body["admin_users"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn admin_activity_returns_zero_filled_window() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;

    let res = server
        .get("/admin/activity")
        .add_query_param("days", "7")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();
    let days = body["days"].as_array().expect("days array");
    assert_eq!(days.len(), 7, "window must be zero-filled to 7 entries");

    let dates: Vec<&str> = days.iter().map(|d| d["date"].as_str().unwrap()).collect();
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(dates, sorted, "days must be ascending");

    // The admin registered moments ago, so today must show at least one signup.
    let today = days.last().unwrap();
    assert!(today["new_users"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn admin_orgs_list_shows_owner_and_member_count() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (user_token, _, user_email) = register_verified(&server, &db).await;

    let slug = format!("org-{}", common::unique_code().to_lowercase());
    let res = server
        .post("/orgs")
        .authorization_bearer(&user_token)
        .json(&json!({ "name": "Admin Test Org", "slug": slug }))
        .await;
    assert_eq!(res.status_code(), 201, "{}", res.text());

    let res = server
        .get("/admin/orgs")
        .add_query_param("search", &slug)
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();
    assert_eq!(body["total"].as_u64(), Some(1));
    let org = &body["orgs"][0];
    assert_eq!(org["slug"].as_str(), Some(slug.as_str()));
    assert_eq!(org["owner_email"].as_str(), Some(user_email.as_str()));
    assert!(org["member_count"].as_i64().unwrap() >= 1);
}
