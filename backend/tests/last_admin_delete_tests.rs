//! The last-admin guard on account deletion counts only admins who can sign in.
//!
//! `POST /auth/delete-account` refuses an admin who is the last one. It counted
//! a disabled admin as an admin, so the only enabled admin could delete their
//! own account while a disabled one remained, leaving an instance nobody could
//! sign in to as an admin.
//!
//! The guard looks at the whole `users` table, which the shared test database
//! cannot give it, so each test runs against a database of its own, dropped
//! (with its connections) when the test ends, including when it panics.

mod common;

use chrono::Utc;
use migration::{Migrator, MigratorTrait};
use opn_onl_backend::entity::users;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
};
use serde_json::{Value, json};

struct ScratchDb {
    db: DatabaseConnection,
    admin_url: String,
    name: String,
}

impl ScratchDb {
    async fn migrated() -> Self {
        dotenvy::dotenv().ok();
        let admin_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let name = format!("opn_scratch_{}", uuid::Uuid::new_v4().simple());
        let admin = Database::connect(&admin_url).await.expect("connect");
        admin
            .execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create scratch database");
        let mut url = url::Url::parse(&admin_url).expect("DATABASE_URL");
        url.set_path(&format!("/{name}"));
        let db = Database::connect(url.as_str()).await.expect("connect");
        let scratch = Self {
            db,
            admin_url,
            name,
        };
        Migrator::up(&scratch.db, None).await.expect("migrate");
        scratch
    }

    /// A new app over the scratch database. The mock transport puts every
    /// caller in one per-second bucket, so each request takes its own.
    async fn app(&self) -> axum_test::TestServer {
        common::set_env("FORCE_HTTPS", "false");
        common::set_env("TRUST_PROXY_HEADERS", "false");
        common::set_env("ENABLE_ACCOUNT_DELETION", "true");
        if std::env::var("JWT_SECRET").is_err() {
            common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
        }
        let state = opn_onl_backend::AppState::for_tests(self.db.clone()).await;
        axum_test::TestServer::new(opn_onl_backend::build_router(state))
            .expect("failed to start test server")
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        let (url, name) = (self.admin_url.clone(), self.name.clone());
        // A fresh runtime on its own thread: Drop cannot await, and the test's
        // runtime may be the one running us.
        let _ = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                if let Ok(admin) = Database::connect(&url).await {
                    let _ = admin
                        .execute_unprepared(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
                        .await;
                }
            });
        })
        .join();
    }
}

const PASSWORD: &str = "password123";

/// Register through the API, then set the account's admin and disabled flags
/// directly. Returns the session token and the user id.
async fn account(scratch: &ScratchDb, admin: bool, disabled: bool) -> (String, i32) {
    let res = scratch
        .app()
        .await
        .post("/auth/register")
        .json(&json!({ "email": common::unique_email(), "password": PASSWORD }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let token = body["token"].as_str().expect("token").to_string();
    let id = body["user_id"].as_i64().expect("user_id") as i32;

    let mut row: users::ActiveModel = users::Entity::find_by_id(id)
        .one(&scratch.db)
        .await
        .expect("db")
        .expect("user")
        .into();
    row.is_admin = Set(admin);
    row.disabled_at = Set(disabled.then(|| Utc::now().naive_utc()));
    row.update(&scratch.db).await.expect("set flags");
    (token, id)
}

async fn delete_account(scratch: &ScratchDb, token: &str) -> (u16, String) {
    let res = scratch
        .app()
        .await
        .post("/auth/delete-account")
        .authorization_bearer(token)
        .json(&json!({ "password": PASSWORD }))
        .await;
    (res.status_code().as_u16(), res.text())
}

async fn is_deleted(scratch: &ScratchDb, id: i32) -> bool {
    users::Entity::find_by_id(id)
        .one(&scratch.db)
        .await
        .expect("db")
        .expect("user row")
        .deleted_at
        .is_some()
}

#[tokio::test]
async fn the_only_enabled_admin_cannot_delete_themselves_beside_a_disabled_one() {
    let scratch = ScratchDb::migrated().await;
    let (token, id) = account(&scratch, true, false).await;
    let (_, disabled_admin) = account(&scratch, true, true).await;

    let (status, body) = delete_account(&scratch, &token).await;

    assert_eq!(status, 409, "{body}");
    assert!(body.contains("last remaining admin"), "{body}");
    assert!(!is_deleted(&scratch, id).await);
    assert!(!is_deleted(&scratch, disabled_admin).await);
}

/// Controls: with another enabled admin, an admin can still leave, and an
/// ordinary account is not held back by the guard.
#[tokio::test]
async fn an_admin_beside_another_enabled_admin_can_still_delete_themselves() {
    let scratch = ScratchDb::migrated().await;
    let (token, id) = account(&scratch, true, false).await;
    let (_, other) = account(&scratch, true, false).await;
    // A disabled admin makes no difference either way.
    account(&scratch, true, true).await;

    let (status, body) = delete_account(&scratch, &token).await;

    assert_eq!(status, 200, "{body}");
    assert!(is_deleted(&scratch, id).await);
    assert!(!is_deleted(&scratch, other).await);
}

#[tokio::test]
async fn an_ordinary_account_can_delete_itself_beside_a_lone_admin() {
    let scratch = ScratchDb::migrated().await;
    account(&scratch, true, false).await;
    let (token, id) = account(&scratch, false, false).await;

    let (status, body) = delete_account(&scratch, &token).await;

    assert_eq!(status, 200, "{body}");
    assert!(is_deleted(&scratch, id).await);
}
