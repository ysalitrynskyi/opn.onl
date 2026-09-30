//! Soft-deleted emails must be reusable; two *live* accounts with the same
//! address must still be rejected. Real router + real Postgres.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use opn_onl_backend::entity::users;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};

async fn register(server: &axum_test::TestServer, email: &str) -> axum_test::TestResponse {
    server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await
}

async fn register_ok(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = register(server, email).await;
    assert_eq!(res.status_code(), 201, "register failed: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

#[tokio::test]
async fn soft_deleted_email_can_be_registered_again() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ENABLE_ACCOUNT_DELETION", "true") };

    let (server, db) = spawn_real_app().await;
    let email = unique_email();
    let (jwt, user_id) = register_ok(&server, &email).await;

    // Last-admin protection would refuse the delete if this signup bootstrapped
    // as admin on an empty live table.
    let registered = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    if registered.is_admin {
        let mut active: users::ActiveModel = registered.into();
        active.is_admin = sea_orm::ActiveValue::Set(false);
        active.update(&db).await.expect("demote bootstrap admin");
    }
    mark_email_verified(&db, user_id).await;

    let del = server
        .post("/auth/delete-account")
        .authorization_bearer(&jwt)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(del.status_code(), 200, "self delete: {}", del.text());

    let deleted = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(deleted.deleted_at.is_some(), "delete must be a soft delete");

    let (_new_jwt, new_id) = register_ok(&server, &email).await;
    assert_ne!(new_id, user_id, "reuse must create a new row");

    let live = users::Entity::find()
        .filter(users::Column::Email.eq(&email))
        .filter(users::Column::DeletedAt.is_null())
        .all(&db)
        .await
        .unwrap();
    assert_eq!(live.len(), 1, "exactly one live row for the reused address");
    assert_eq!(live[0].id, new_id);

    let all_rows = users::Entity::find()
        .filter(users::Column::Email.eq(&email))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(
        all_rows, 2,
        "deleted row must remain alongside the new live one"
    );

    let login = server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(
        login.status_code(),
        200,
        "new account must be able to log in"
    );
    let login_body: Value = login.json();
    assert_eq!(login_body["user_id"].as_i64().unwrap() as i32, new_id);

    let conflict = register(&server, &email).await;
    assert_eq!(
        conflict.status_code(),
        409,
        "two live accounts with the same email must still conflict: {}",
        conflict.text()
    );
}
