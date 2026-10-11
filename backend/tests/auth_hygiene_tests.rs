//! Account-security hygiene around sign-up, recovery and admin accounts.
//!
//! - Requests that send mail must answer without waiting for SMTP. They used
//!   to await the send, so the response time told an address with an account
//!   from one without, and a slow mail server stalled sign-up.
//! - `ensure_admin_exists` promoted the oldest account even when it was
//!   disabled, and wrote the address to the log.
//! - Two admins deleting themselves at once could both be allowed, leaving
//!   none.
//! - An API key rewrote its `last_used_at` on every request.
//!
//! Mail is tested against the local SMTP server in `tests/common/smtp_stub.rs`
//! with delivery to one address held back, so "the response did not wait" is
//! decided by what the server did, not by a stopwatch.

//! Account hygiene: admin accounts and API-key bookkeeping.
//!
//! - `ensure_admin_exists` promoted the oldest account even when it was
//!   disabled, and wrote the address to the log.
//! - Two admins deleting themselves at once could both be allowed, leaving
//!   none.
//! - An API key rewrote its `last_used_at` on every request.

mod common;

use axum_test::TestServer;
use chrono::Utc;
use common::unique_email;
use migration::{Migrator, MigratorTrait};
use opn_onl_backend::entity::{api_keys, users};
use opn_onl_backend::handlers::links::hash_api_key;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, Database, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

fn pin_environment() {
    // Pin before dotenvy runs, which never overrides a variable that is set.
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
}

async fn account(db: &DatabaseConnection) -> users::Model {
    users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

// ------------------------------------------------------------ API key usage

#[tokio::test]
async fn api_key_last_used_is_refreshed_at_most_every_five_minutes() {
    let (server, db) = common::spawn_real_app().await;
    let owner = account(&db).await;
    let raw_key = format!("opn_{}", uuid::Uuid::new_v4().simple());
    let key = api_keys::ActiveModel {
        user_id: Set(owner.id),
        name: Set("hygiene".to_string()),
        key_hash: Set(hash_api_key(&raw_key)),
        key_prefix: Set(raw_key.chars().take(12).collect()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert API key");

    // The first use is recorded.
    assert!(last_used_at(&db, key.id).await.is_none());
    use_key(&server, &raw_key).await;
    assert!(
        last_used_at(&db, key.id).await.is_some(),
        "the first use is recorded"
    );

    // Used a minute ago: another request leaves the stamp alone.
    let recent = set_last_used_at(&db, key.id, 1).await;
    use_key(&server, &raw_key).await;
    assert_eq!(
        last_used_at(&db, key.id).await,
        recent,
        "a recent stamp is not rewritten"
    );

    // Used long ago: it is brought up to date.
    set_last_used_at(&db, key.id, 6).await;
    use_key(&server, &raw_key).await;
    let refreshed = last_used_at(&db, key.id).await.expect("stamped");
    assert!(
        refreshed > Utc::now().naive_utc() - chrono::Duration::minutes(1),
        "a stale stamp is refreshed: {refreshed}"
    );
}

async fn last_used_at(db: &DatabaseConnection, key_id: i32) -> Option<chrono::NaiveDateTime> {
    api_keys::Entity::find_by_id(key_id)
        .one(db)
        .await
        .expect("db")
        .expect("key row")
        .last_used_at
}

/// Pretend the key was last used `minutes_ago` minutes ago; returns what the
/// database stored.
async fn set_last_used_at(
    db: &DatabaseConnection,
    key_id: i32,
    minutes_ago: i64,
) -> Option<chrono::NaiveDateTime> {
    let mut row: api_keys::ActiveModel = api_keys::Entity::find_by_id(key_id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .into();
    row.last_used_at = Set(Some(
        Utc::now().naive_utc() - chrono::Duration::minutes(minutes_ago),
    ));
    row.update(db).await.expect("set last_used_at").last_used_at
}

async fn use_key(server: &TestServer, raw_key: &str) {
    let res = server.get("/links").authorization_bearer(raw_key).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
}

// ---------------------------------------------------- databases of their own

/// A throwaway database next to the suite's one, dropped (connections and all)
/// when the value goes out of scope, including when the test panics. These
/// tests are about the whole `users` table (who is the oldest account, how many
/// admins there are), which the shared database cannot give them.
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

    async fn app(&self) -> TestServer {
        pin_environment();
        let state = opn_onl_backend::AppState::for_tests(self.db.clone()).await;
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server")
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
    disabled: bool,
    deleted: bool,
) -> users::Model {
    let now = Utc::now().naive_utc();
    users::ActiveModel {
        email: Set(format!("{mailbox}@users.opn.onl")),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(now),
        email_verified: Set(true),
        disabled_at: Set(disabled.then_some(now)),
        deleted_at: Set(deleted.then_some(now)),
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

// ------------------------------------------------------ the boot-time admin

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

impl<S> Layer<S> for LogCapture
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields(event.metadata().target().to_string());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

/// With no admin left, the oldest account that can sign in is promoted: not a
/// disabled one (the instance would still have nobody who can sign in as an
/// admin) and not a deleted one. The log names the account by id, never by
/// address.
#[tokio::test(flavor = "current_thread")]
async fn the_boot_time_admin_is_an_account_that_can_sign_in() {
    let captured = LogCapture::default();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .with(captured.clone());
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();

    let scratch = ScratchDb::migrated().await;
    let db = &scratch.db;
    let mailbox = format!("mailbox-{}", uuid::Uuid::new_v4().simple());
    let disabled_oldest = insert_user(db, "disabled-oldest", true, false).await;
    let deleted = insert_user(db, "deleted", false, true).await;
    let chosen = insert_user(db, &mailbox, false, false).await;
    let later = insert_user(db, "later", false, false).await;

    opn_onl_backend::ensure_admin_exists(db).await;

    assert!(is_admin(db, chosen.id).await, "the oldest enabled account");
    for (name, other) in [
        ("disabled", &disabled_oldest),
        ("deleted", &deleted),
        ("later", &later),
    ] {
        assert!(!is_admin(db, other.id).await, "{name} was promoted");
    }

    let lines = captured.0.lock().unwrap().clone();
    assert!(
        lines.iter().any(|line| line.contains("promoted")),
        "the promotion is logged: {lines:#?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains(&mailbox)),
        "the account's address must not appear in the logs: {lines:#?}"
    );

    // Nobody who can sign in: nobody is promoted, and nothing panics.
    let mut demote: users::ActiveModel = users::Entity::find_by_id(chosen.id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .into();
    demote.is_admin = Set(false);
    demote.disabled_at = Set(Some(Utc::now().naive_utc()));
    demote.update(db).await.expect("disable");
    let mut disable_later: users::ActiveModel = later.clone().into();
    disable_later.disabled_at = Set(Some(Utc::now().naive_utc()));
    disable_later.update(db).await.expect("disable");

    opn_onl_backend::ensure_admin_exists(db).await;
    let admins = users::Entity::find()
        .filter(users::Column::IsAdmin.eq(true))
        .count(db)
        .await
        .expect("count admins");
    assert_eq!(admins, 0, "a disabled account was promoted");
}

// --------------------------------------------------------- the last admin

async fn register(server: &TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

/// Each admin may delete their account only while another live admin remains.
/// Two admins asking at the same moment each used to count the other, and both
/// were let through, leaving the instance with no admin until a restart.
#[tokio::test]
async fn two_admins_deleting_at_once_cannot_remove_the_last_admins() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let scratch = ScratchDb::migrated().await;
    let server = scratch.app().await;

    // The first account on an empty instance is its admin.
    let (first_token, first_id) = register(&server, &unique_email()).await;
    let (second_token, second_id) = register(&server, &unique_email()).await;
    assert!(is_admin(&scratch.db, first_id).await);
    let mut promote: users::ActiveModel = users::Entity::find_by_id(second_id)
        .one(&scratch.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    promote.is_admin = Set(true);
    promote.update(&scratch.db).await.expect("promote");

    let delete = |token: String| {
        let server = &server;
        async move {
            server
                .post("/auth/delete-account")
                .authorization_bearer(token)
                .json(&json!({ "password": "password123" }))
                .await
        }
    };
    let (a, b) = tokio::join!(delete(first_token), delete(second_token));

    let mut statuses = [a.status_code().as_u16(), b.status_code().as_u16()];
    statuses.sort_unstable();
    assert_eq!(
        statuses,
        [200, 409],
        "one deletion goes through, the other is refused: {} / {}",
        a.text(),
        b.text()
    );
    let live_admins = users::Entity::find()
        .filter(users::Column::IsAdmin.eq(true))
        .filter(users::Column::DeletedAt.is_null())
        .count(&scratch.db)
        .await
        .expect("count admins");
    assert_eq!(live_admins, 1, "an admin is left");
}
