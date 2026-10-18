//! `GET /links/export` streams the CSV a page at a time. Every listed link must
//! appear exactly once, in order, across page boundaries — including rows that
//! share a creation time. Real router + real Postgres.

use crate::common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde_json::{Value, json};
use std::collections::HashSet;

/// More than two export pages (1,000 rows each).
const LINKS: i32 = 2_150;

#[tokio::test]
async fn export_streams_every_link_once_in_order_across_pages() {
    let (server, db) = spawn_real_app().await;
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    let token = body["token"].as_str().unwrap().to_string();
    mark_email_verified(&db, user_id).await;

    // Half of the rows share one timestamp, so the pages must break ties on
    // id; the rest are a second apart. Codes are unique per run.
    let prefix = unique_code();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO links (code, original_url, user_id, created_at) \
         SELECT $1 || '-' || n, 'https://iana.org/export/' || n, $2, \
                CASE WHEN n % 2 = 0 THEN TIMESTAMP '2026-01-01 00:00:00' \
                     ELSE TIMESTAMP '2026-01-01 00:00:00' - make_interval(secs => n) END \
         FROM generate_series(1, $3) AS n",
        [prefix.clone().into(), user_id.into(), LINKS.into()],
    ))
    .await
    .expect("insert links");

    let res = server
        .get("/links/export")
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let text = res.text();
    let mut lines = text.lines();
    let header = lines.next().expect("header");
    assert!(
        header
            .trim_start_matches('\u{feff}')
            .starts_with("ID,Code,")
    );

    let ids: Vec<i64> = lines
        .map(|line| line.split(',').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids.len(), LINKS as usize, "every link exactly once");
    let unique: HashSet<i64> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "no row repeated across pages");

    // Same order the export promises: newest first, then the higher id.
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT id::bigint FROM links WHERE user_id = $1 AND deleted_at IS NULL \
             ORDER BY created_at DESC, id DESC",
            [user_id.into()],
        ))
        .await
        .unwrap();
    let expected: Vec<i64> = rows
        .into_iter()
        .map(|row| row.try_get_by_index::<i64>(0).unwrap())
        .collect();
    assert_eq!(ids, expected);
}
