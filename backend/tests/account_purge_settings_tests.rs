//! What operators and admins are told about the purge window: `/auth/settings`
//! publishes it (Settings shows it in the delete-account warning) and the admin
//! user list shows when each deleted account is removed for good. Reads and
//! writes `ACCOUNT_PURGE_DAYS`, so it lives in its own test binary and runs as
//! one sequential test.

mod common;

use chrono::{DateTime, Duration, Timelike, Utc};
use common::{spawn_real_app, unique_email};
use opn_onl_backend::entity::users;
use opn_onl_backend::utils::jwt::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;

#[tokio::test]
async fn the_window_is_published_to_users_and_admins() {
    common::remove_env("ACCOUNT_PURGE_DAYS");
    let (server, db) = spawn_real_app().await;

    let admin = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert admin");
    let token = create_jwt(admin.id, &admin.email, admin.token_version).expect("mint token");

    // Whole seconds: the column keeps microseconds, the clock may have more.
    let deleted_at = Utc::now().naive_utc().with_nanosecond(0).unwrap() - Duration::days(3);
    let deleted_email = unique_email();
    users::ActiveModel {
        email: Set(deleted_email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(false),
        deleted_at: Set(Some(deleted_at)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert deleted user");

    let settings = async || -> Value { server.get("/auth/settings").await.json() };
    let listed = async |email: &str| -> Value {
        let res = server
            .get("/admin/users")
            .add_query_param("search", email)
            .authorization_bearer(&token)
            .await;
        assert_eq!(res.status_code(), 200, "admin users: {}", res.text());
        res.json::<Value>()["users"][0].clone()
    };

    // Default: thirty days, counted from the deletion.
    assert_eq!(settings().await["account_purge_days"], 30);
    let row = listed(&deleted_email).await;
    let purge_at = DateTime::parse_from_rfc3339(row["purge_at"].as_str().expect("purge_at"))
        .expect("RFC 3339 with an offset")
        .naive_utc();
    assert_eq!(purge_at, deleted_at + Duration::days(30));
    assert!(
        row["purge_at"].as_str().unwrap().ends_with('Z'),
        "UTC with an explicit Z: {}",
        row["purge_at"]
    );
    let live = listed(&admin.email).await;
    assert!(live["purge_at"].is_null(), "a live account has no deadline");

    // The operator's window is what both report.
    common::set_env("ACCOUNT_PURGE_DAYS", "14");
    assert_eq!(settings().await["account_purge_days"], 14);
    let row = listed(&deleted_email).await;
    let purge_at = DateTime::parse_from_rfc3339(row["purge_at"].as_str().unwrap())
        .unwrap()
        .naive_utc();
    assert_eq!(purge_at, deleted_at + Duration::days(14));

    // Disabled: no promise to make and no deadline to show.
    common::set_env("ACCOUNT_PURGE_DAYS", "0");
    assert!(settings().await["account_purge_days"].is_null());
    assert!(listed(&deleted_email).await["purge_at"].is_null());

    common::remove_env("ACCOUNT_PURGE_DAYS");
}
