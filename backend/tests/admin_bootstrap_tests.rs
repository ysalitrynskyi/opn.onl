//! The boot-time admin check counts only admins who can sign in.
//!
//! `ensure_admin_exists` promotes the oldest enabled account when the instance
//! has no admin. It counted a disabled admin as an admin, so an instance whose
//! only admin had been disabled (an email-domain block, a takedown) found "an
//! admin", promoted nobody, and stayed locked out across every restart.
//!
//! The check looks at the whole `users` table, which the shared test database
//! cannot give it, so this runs against a database of its own, dropped (with
//! its connections) when the test ends, including when it panics. It is a
//! separate binary from `auth_hygiene_tests`, whose log capture depends on no
//! other test reaching the same log callsites.

use chrono::Utc;
use migration::{Migrator, MigratorTrait};
use opn_onl_backend::entity::users;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
};

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

async fn insert_user(
    db: &DatabaseConnection,
    mailbox: &str,
    admin: bool,
    disabled: bool,
) -> users::Model {
    let now = Utc::now().naive_utc();
    users::ActiveModel {
        email: Set(format!("{mailbox}@users.opn.onl")),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(now),
        email_verified: Set(true),
        is_admin: Set(admin),
        disabled_at: Set(disabled.then_some(now)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

async fn is_admin(db: &DatabaseConnection, id: i32) -> bool {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("db")
        .expect("user row")
        .is_admin
}

#[tokio::test]
async fn a_disabled_admin_does_not_count_as_the_instances_admin() {
    let scratch = ScratchDb::migrated().await;
    let db = &scratch.db;
    insert_user(db, "disabled-admin", true, true).await;
    let candidate = insert_user(db, "candidate", false, false).await;
    let later = insert_user(db, "later", false, false).await;

    opn_onl_backend::ensure_admin_exists(db).await;

    assert!(
        is_admin(db, candidate.id).await,
        "the oldest account that can sign in takes over"
    );
    assert!(!is_admin(db, later.id).await, "and only one is promoted");
}

/// Control: with an admin who can sign in, a restart promotes nobody.
#[tokio::test]
async fn an_enabled_admin_means_nobody_is_promoted() {
    let scratch = ScratchDb::migrated().await;
    let db = &scratch.db;
    insert_user(db, "disabled-admin", true, true).await;
    insert_user(db, "enabled-admin", true, false).await;
    let other = insert_user(db, "other", false, false).await;

    opn_onl_backend::ensure_admin_exists(db).await;

    assert!(!is_admin(db, other.id).await);
}
