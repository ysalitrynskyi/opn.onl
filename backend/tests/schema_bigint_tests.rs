//! Schema type proofs for the integer→bigint migrations. Real Postgres via
//! `common::spawn_real_app()` so `up()` has already run.

mod common;

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

async fn column_data_type(db: &sea_orm::DatabaseConnection, table: &str, column: &str) -> String {
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT data_type FROM information_schema.columns \
             WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2",
            [table.into(), column.into()],
        ))
        .await
        .expect("information_schema query")
        .expect("column must exist");
    row.try_get::<String>("", "data_type")
        .expect("data_type column")
}

#[tokio::test]
async fn click_events_id_is_bigint() {
    let (_server, db) = common::spawn_real_app().await;
    let data_type = column_data_type(&db, "click_events", "id").await;
    assert_eq!(
        data_type, "bigint",
        "click_events.id must be bigint after m20220101_000032, got {data_type}"
    );

    let seq = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT data_type::text AS data_type FROM pg_sequences \
             WHERE schemaname = 'public' AND sequencename = 'click_events_id_seq'",
            [],
        ))
        .await
        .expect("pg_sequences query")
        .expect("click_events_id_seq");
    let seq_type: String = seq.try_get("", "data_type").expect("seq data_type");
    assert_eq!(
        seq_type, "bigint",
        "click_events_id_seq must be bigint, got {seq_type}"
    );
}
