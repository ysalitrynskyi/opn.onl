//! The live-list and membership indexes added for the request-path review
//! must exist after migrations. Real Postgres via `common::spawn_real_app`.

mod common;

use common::spawn_real_app;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

async fn index_exists(db: &sea_orm::DatabaseConnection, name: &str) -> bool {
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT 1 FROM pg_indexes WHERE indexname = $1",
            [name.into()],
        ))
        .await
        .expect("pg_indexes");
    row.is_some()
}

#[tokio::test]
async fn membership_and_live_list_indexes_exist() {
    let (_server, db) = spawn_real_app().await;
    for name in [
        "idx_org_members_user_id",
        "idx_links_user_created_live",
        "idx_links_folder_live",
        "idx_links_created_at",
        "idx_users_created_at",
    ] {
        assert!(
            index_exists(&db, name).await,
            "expected index {name} after migrations"
        );
    }
}
