//! Two signups that both observe an empty `users` table must not both become
//! admin. The shared test database is never empty, so this creates one and
//! drops it (connections included) when the test ends, including on panic.
//!
//! `LOCK TABLE users IN EXCLUSIVE MODE` still allows the signup's `SELECT
//! COUNT` (`ACCESS SHARE`) and blocks its `INSERT`. Both requests therefore
//! finish the unlocked count before either insert lands. After the fix, the
//! second signup waits on the transaction advisory lock instead, and the
//! release condition accepts that shape too.

mod common;

use std::future::IntoFuture;
use std::pin::pin;
use std::time::{Duration, Instant};

use migration::{Migrator, MigratorTrait};
use sea_orm::{
    ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, EntityTrait, Statement,
    TransactionTrait,
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
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        let (url, name) = (self.admin_url.clone(), self.name.clone());
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

async fn relation_waiters(db: &DatabaseConnection) -> i64 {
    let row = db
        .query_one(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*)::bigint FROM pg_locks \
             WHERE relation = 'users'::regclass \
               AND locktype = 'relation' \
               AND NOT granted",
        ))
        .await
        .expect("relation waiters")
        .expect("row");
    row.try_get_by_index(0).expect("count")
}

async fn advisory_waiters(db: &DatabaseConnection) -> i64 {
    let row = db
        .query_one(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*)::bigint FROM pg_stat_activity \
             WHERE datname = current_database() \
               AND wait_event_type = 'Lock' \
               AND wait_event = 'advisory'",
        ))
        .await
        .expect("advisory waiters")
        .expect("row");
    row.try_get_by_index(0).expect("count")
}

/// Both signups have passed the unlocked count (two inserts waiting), or the
/// second is waiting on the signup advisory lock behind the first.
async fn signups_are_serialized(db: &DatabaseConnection) -> bool {
    let relations = relation_waiters(db).await;
    let advisory = advisory_waiters(db).await;
    relations >= 2 || (relations >= 1 && advisory >= 1)
}

#[tokio::test]
async fn two_simultaneous_first_signups_produce_one_admin() {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }

    let scratch = ScratchDb::migrated().await;
    let state = opn_onl_backend::AppState::for_tests(scratch.db.clone()).await;
    let server =
        axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("test server");

    let hold = scratch.db.begin().await.expect("begin");
    hold.execute_unprepared("LOCK TABLE users IN EXCLUSIVE MODE")
        .await
        .expect("lock users");

    let mut first = pin!(
        server
            .post("/auth/register")
            .json(&json!({
                "email": format!("first_{}@users.opn.onl", uuid::Uuid::new_v4().simple()),
                "password": "password123",
            }))
            .into_future()
    );
    let mut second = pin!(
        server
            .post("/auth/register")
            .json(&json!({
                "email": format!("second_{}@users.opn.onl", uuid::Uuid::new_v4().simple()),
                "password": "password123",
            }))
            .into_future()
    );

    let started = Instant::now();
    tokio::select! {
        res = &mut first => panic!("first signup finished before it blocked: {} {}", res.status_code(), res.text()),
        res = &mut second => panic!("second signup finished before it blocked: {} {}", res.status_code(), res.text()),
        _ = async {
            loop {
                if signups_are_serialized(&scratch.db).await {
                    break;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(15),
                    "signups never reached the users lock (relation waiters {}, advisory {})",
                    relation_waiters(&scratch.db).await,
                    advisory_waiters(&scratch.db).await
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        } => {}
    }
    hold.rollback().await.expect("release users lock");
    let (first, second) = tokio::join!(first, second);

    assert_eq!(first.status_code(), 201, "first: {}", first.text());
    assert_eq!(second.status_code(), 201, "second: {}", second.text());
    let admins = [first.json::<Value>(), second.json::<Value>()]
        .into_iter()
        .filter(|body| body["is_admin"] == true)
        .count();
    assert_eq!(admins, 1, "exactly one of the two first signups is admin");

    let rows = opn_onl_backend::entity::users::Entity::find()
        .all(&scratch.db)
        .await
        .expect("users");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|user| user.is_admin).count(), 1);

    let third = server
        .post("/auth/register")
        .json(&json!({
            "email": format!("third_{}@users.opn.onl", uuid::Uuid::new_v4().simple()),
            "password": "password123",
        }))
        .await;
    assert_eq!(third.status_code(), 201, "third: {}", third.text());
    assert_eq!(
        third.json::<Value>()["is_admin"],
        false,
        "a later signup is not admin"
    );
}
