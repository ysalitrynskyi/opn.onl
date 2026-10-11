//! Backup administration: listing, retention and the schedule an admin sees.
//!
//! The S3 side is a small in-process stand-in that keeps a list of keys, so
//! the real `BackupService` list and delete calls run end to end. Nothing here
//! starts `pg_dump` or talks to a real bucket.

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use common::unique_email;
use opn_onl_backend::{AppState, build_router};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

const BUCKET: &str = "test-bucket";

/// The tests set process-wide environment that `BackupService` reads when it
/// is built. Run them one at a time.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const BACKUP_VARS: &[&str] = &[
    "BACKUP_S3_ENDPOINT",
    "BACKUP_S3_BUCKET",
    "BACKUP_S3_ACCESS_KEY",
    "BACKUP_S3_SECRET_KEY",
    "BACKUP_INTERVAL_HOURS",
    "BACKUP_KEEP",
];

async fn lock_and_clear() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().await;
    for var in BACKUP_VARS {
        common::remove_env(var);
    }
    guard
}

/// Keys the stand-in bucket holds. Deletes remove from it.
type Keys = Arc<Mutex<Vec<String>>>;

async fn bucket_request(State(keys): State<Keys>, method: Method, uri: Uri) -> Response {
    let path = uri.path().trim_matches('/');
    if method == Method::GET && path == BUCKET {
        let prefix = uri
            .query()
            .unwrap_or_default()
            .split('&')
            .find_map(|pair| pair.strip_prefix("prefix="))
            .map(|p| urlencoding::decode(p).unwrap().into_owned())
            .unwrap_or_default();
        let listed: Vec<String> = keys
            .lock()
            .unwrap()
            .iter()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        let contents: String = listed
            .iter()
            .map(|key| {
                format!(
                    "<Contents><Key>{key}</Key><LastModified>2026-10-01T00:00:00.000Z</LastModified>\
                     <ETag>&quot;etag&quot;</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Contents>"
                )
            })
            .collect();
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Name>{BUCKET}</Name><Prefix>{prefix}</Prefix><KeyCount>{}</KeyCount>\
             <MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated>{contents}</ListBucketResult>",
            listed.len()
        );
        return ([(header::CONTENT_TYPE, "application/xml")], body).into_response();
    }
    if method == Method::DELETE
        && let Some(key) = path.strip_prefix(&format!("{BUCKET}/"))
    {
        let key = urlencoding::decode(key).unwrap().into_owned();
        keys.lock().unwrap().retain(|existing| *existing != key);
        return StatusCode::NO_CONTENT.into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

/// Starts the stand-in bucket and points the backup settings at it.
async fn start_bucket(initial: &[&str]) -> Keys {
    let keys: Keys = Arc::new(Mutex::new(initial.iter().map(|k| k.to_string()).collect()));
    let app = Router::new()
        .fallback(bucket_request)
        .with_state(keys.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stand-in bucket");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    common::set_env("BACKUP_S3_ENDPOINT", format!("http://{addr}"));
    common::set_env("BACKUP_S3_BUCKET", BUCKET);
    common::set_env("BACKUP_S3_ACCESS_KEY", "test-access");
    common::set_env("BACKUP_S3_SECRET_KEY", "test-secret");
    keys
}

/// The real router with a `BackupService` built from the environment as it is
/// now, the way `main` builds it.
async fn app_from_env() -> (axum_test::TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = common::setup_test_db().await;
    let state = AppState::for_tests(db.clone()).await;
    let server = axum_test::TestServer::new(build_router(state)).expect("test server");
    (server, db)
}

async fn register(server: &axum_test::TestServer) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register failed: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

/// Register a fresh admin: a normal registration plus a direct DB promotion.
async fn register_admin(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    use opn_onl_backend::entity::users;
    let (token, user_id) = register(server).await;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db error")
        .expect("user not found");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("failed to promote user");
    token
}

fn backup_name(day: u32) -> String {
    format!("backup_202610{day:02}_030000.sql.gz")
}

fn held(keys: &Keys) -> Vec<String> {
    let mut held = keys.lock().unwrap().clone();
    held.sort();
    held
}

#[tokio::test]
async fn cleanup_to_zero_is_refused_before_anything_is_listed_or_deleted() {
    let _env = lock_and_clear().await;
    let keys = start_bucket(&[&backup_name(1), &backup_name(2)]).await;
    let (server, db) = app_from_env().await;
    let admin = register_admin(&server, &db).await;

    let res = server
        .delete("/admin/backup/cleanup/0")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 400, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["message"], "keep must be at least 1");
    assert_eq!(
        held(&keys),
        vec![backup_name(1), backup_name(2)],
        "a refused cleanup must not delete anything"
    );
}

#[tokio::test]
async fn cleanup_to_zero_is_refused_even_when_backup_storage_is_not_configured() {
    let _env = lock_and_clear().await;
    let (server, db) = app_from_env().await;
    let admin = register_admin(&server, &db).await;

    let res = server
        .delete("/admin/backup/cleanup/0")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 400, "{}", res.text());

    // Control: a legitimate keep count gets past validation and reaches the
    // "not configured" answer.
    let res = server
        .delete("/admin/backup/cleanup/1")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 503, "{}", res.text());
}

#[tokio::test]
async fn cleanup_requires_an_admin() {
    let _env = lock_and_clear().await;
    let (server, _db) = app_from_env().await;
    let (token, _) = register(&server).await;

    let res = server
        .delete("/admin/backup/cleanup/0")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 403);
}

#[tokio::test]
async fn cleanup_keeps_the_newest_n_and_leaves_foreign_objects_alone() {
    let _env = lock_and_clear().await;
    let keys = start_bucket(&[
        &backup_name(3),
        &backup_name(1),
        // Not named the way backups are: not listed, not counted, not deleted.
        "backup_pre-migration.sql.gz",
        &backup_name(5),
        &backup_name(2),
        &backup_name(4),
    ])
    .await;
    let (server, db) = app_from_env().await;
    let admin = register_admin(&server, &db).await;

    let res = server
        .delete("/admin/backup/cleanup/2")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(body["message"], "Cleaned up 3 old backups");

    let mut expected = vec![
        backup_name(4),
        backup_name(5),
        "backup_pre-migration.sql.gz".to_string(),
    ];
    expected.sort();
    assert_eq!(held(&keys), expected);

    // Control: asking to keep more than exist deletes nothing.
    let res = server
        .delete("/admin/backup/cleanup/14")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200);
    assert_eq!(held(&keys), expected);
}

#[tokio::test]
async fn the_list_is_newest_first_without_foreign_objects_and_reports_the_schedule() {
    let _env = lock_and_clear().await;
    start_bucket(&[
        &backup_name(2),
        "backup_pre-migration.sql.gz",
        &backup_name(9),
        &backup_name(5),
    ])
    .await;
    common::set_env("BACKUP_INTERVAL_HOURS", "6");
    common::set_env("BACKUP_KEEP", "5");
    let (server, db) = app_from_env().await;
    let admin = register_admin(&server, &db).await;

    let res = server
        .get("/admin/backup")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let body: Value = res.json();
    assert_eq!(
        body["backups"],
        json!([backup_name(9), backup_name(5), backup_name(2)])
    );
    assert_eq!(body["schedule"], json!({ "interval_hours": 6, "keep": 5 }));
}

#[tokio::test]
async fn the_schedule_defaults_to_daily_keeping_fourteen_and_can_be_turned_off() {
    let _env = lock_and_clear().await;
    start_bucket(&[]).await;
    let (server, db) = app_from_env().await;
    let admin = register_admin(&server, &db).await;

    let res = server
        .get("/admin/backup")
        .authorization_bearer(&admin)
        .await;
    let body: Value = res.json();
    assert_eq!(
        body["schedule"],
        json!({ "interval_hours": 24, "keep": 14 })
    );

    // Compose passes an unset option as an empty string.
    common::set_env("BACKUP_INTERVAL_HOURS", "");
    common::set_env("BACKUP_KEEP", "");
    let (server, _db) = app_from_env().await;
    let res = server
        .get("/admin/backup")
        .authorization_bearer(&admin)
        .await;
    let body: Value = res.json();
    assert_eq!(
        body["schedule"],
        json!({ "interval_hours": 24, "keep": 14 })
    );

    common::set_env("BACKUP_INTERVAL_HOURS", "0");
    let (server, _db) = app_from_env().await;
    let res = server
        .get("/admin/backup")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200);
    let body: Value = res.json();
    assert_eq!(body["backups"], json!([]));
    assert!(body["schedule"].is_null(), "{body}");
}

#[tokio::test]
async fn listing_without_backup_storage_is_503() {
    let _env = lock_and_clear().await;
    let (server, db) = app_from_env().await;
    let admin = register_admin(&server, &db).await;

    let res = server
        .get("/admin/backup")
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 503);
}
