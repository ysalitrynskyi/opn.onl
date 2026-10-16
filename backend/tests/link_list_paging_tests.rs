//! The link lists page: `GET /links`, `/folders/{id}/links` and
//! `/tags/{id}/links` take `limit`/`offset`, answer with a bare JSON array, and
//! put the number of matching rows in `X-Total-Count`. A page that stops at the
//! 1000-row cap used to be indistinguishable from the whole list. Real router +
//! real Postgres; the large lists are seeded with one INSERT ... SELECT.

mod common;

use std::sync::Arc;

use axum_test::TestServer;
use common::{mark_email_verified, setup_test_db, unique_code, unique_email};
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde_json::{Value, json};

const SEEDED: i32 = 1050;
/// The default and maximum page size of the link lists.
const PAGE_CAP: usize = 1000;

/// The real router, with request budgets that walking a long list cannot
/// spend (the default allows ten requests a second).
async fn spawn_real_app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }

    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let roomy = || Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)));
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
        report: roomy(),
    });
    let server =
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server");
    (server, db)
}

async fn register_verified(server: &TestServer, db: &DatabaseConnection) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (body["token"].as_str().unwrap().to_string(), user_id)
}

async fn exec(db: &DatabaseConnection, sql: &str, values: Vec<sea_orm::Value>) {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .await
    .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// `count` links for `user_id` in one statement, so every row carries the same
/// `created_at` (the column default is the transaction's start time).
async fn seed_links(db: &DatabaseConnection, user_id: i32, folder_id: Option<i32>, count: i32) {
    exec(
        db,
        "INSERT INTO links (code, original_url, user_id, folder_id) \
         SELECT $1 || g::text, 'https://example.com/seeded/' || g::text, $2, $3 \
         FROM generate_series(1, $4::int) AS g",
        vec![
            unique_code().into(),
            user_id.into(),
            folder_id.into(),
            count.into(),
        ],
    )
    .await;
}

async fn purge(db: &DatabaseConnection, user_id: i32) {
    exec(
        db,
        "DELETE FROM link_tags WHERE link_id IN (SELECT id FROM links WHERE user_id = $1)",
        vec![user_id.into()],
    )
    .await;
    exec(
        db,
        "DELETE FROM links WHERE user_id = $1",
        vec![user_id.into()],
    )
    .await;
}

/// One page of a list: the `X-Total-Count` header and the ids of the rows.
struct Page {
    total: u64,
    ids: Vec<i64>,
    created_at: Vec<String>,
}

async fn page(server: &TestServer, token: &str, path: &str) -> Page {
    let res = server.get(path).authorization_bearer(token).await;
    assert_eq!(res.status_code(), 200, "GET {path}: {}", res.text());
    let total = res
        .headers()
        .get("x-total-count")
        .unwrap_or_else(|| panic!("GET {path} has no X-Total-Count"))
        .to_str()
        .unwrap()
        .parse()
        .expect("X-Total-Count is a number");
    let body: Value = res.json();
    let rows = body.as_array().expect("the body stays a bare array");
    Page {
        total,
        ids: rows.iter().map(|l| l["id"].as_i64().unwrap()).collect(),
        created_at: rows
            .iter()
            .map(|l| l["created_at"].as_str().unwrap().to_string())
            .collect(),
    }
}

/// `assert_eq!` for two long id lists, naming the first place they differ
/// instead of printing a thousand ids.
fn assert_same_order(actual: &[i64], expected: &[i64], what: &str) {
    let differs = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(
        differs.is_none() && actual.len() == expected.len(),
        "{what}: {} ids, expected {}; first difference at position {differs:?} \
         (got {:?}, expected {:?})",
        actual.len(),
        expected.len(),
        differs.map(|i| actual[i]),
        differs.map(|i| expected[i]),
    );
}

/// Every page of `path` in order, `size` rows at a time.
async fn walk(server: &TestServer, token: &str, path: &str, size: usize) -> Vec<Page> {
    let joiner = if path.contains('?') { '&' } else { '?' };
    let mut pages = Vec::new();
    let mut offset = 0;
    loop {
        let next = page(
            server,
            token,
            &format!("{path}{joiner}limit={size}&offset={offset}"),
        )
        .await;
        let done = next.ids.len() < size;
        offset += next.ids.len();
        pages.push(next);
        if done {
            return pages;
        }
    }
}

/// More links than the cap: the first page says how many there are, and
/// `offset` reaches every one of them exactly once.
#[tokio::test]
async fn links_list_reports_the_total_and_pages_to_the_last_link() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    seed_links(&db, user_id, None, SEEDED).await;

    let first = page(&server, &token, "/links").await;
    let rest = page(&server, &token, "/links?offset=1000").await;
    let past_the_end = page(&server, &token, "/links?offset=2000").await;
    let far_past_the_end = page(&server, &token, "/links?offset=18446744073709551615").await;
    // The total ignores the window.
    let small_window = page(&server, &token, "/links?limit=5&offset=1040").await;
    let walked = walk(&server, &token, "/links", 100).await;
    purge(&db, user_id).await;

    assert_eq!(first.total, SEEDED as u64);
    assert_eq!(
        first.ids.len(),
        PAGE_CAP,
        "the default page size is as before"
    );
    assert_eq!(rest.total, SEEDED as u64);
    assert_eq!(rest.ids.len(), SEEDED as usize - PAGE_CAP);
    assert!(past_the_end.ids.is_empty());
    assert!(far_past_the_end.ids.is_empty());
    assert_eq!(far_past_the_end.total, SEEDED as u64);
    assert_eq!(past_the_end.total, SEEDED as u64);
    assert_eq!(small_window.total, SEEDED as u64);
    assert_eq!(small_window.ids.len(), 5);

    let mut all: Vec<i64> = first.ids.iter().chain(&rest.ids).copied().collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), SEEDED as usize, "no link repeated or skipped");

    // Eleven requests of a hundred, the way the dashboard loads more.
    assert_eq!(walked.len(), 11);
    let walked_ids: Vec<i64> = walked.iter().flat_map(|p| p.ids.clone()).collect();
    let mut sorted = walked_ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), SEEDED as usize);
    let whole: Vec<i64> = first.ids.iter().chain(&rest.ids).copied().collect();
    assert_same_order(&walked_ids, &whole, "page size must not change the order");
}

/// Rows created in one instant (a bulk create does that) must keep one place
/// in the order, or paging repeats some rows and skips others. Pinned links
/// lead whatever their age.
#[tokio::test]
async fn links_list_order_is_stable_when_created_at_ties() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    seed_links(&db, user_id, None, SEEDED).await;
    // The oldest link, pinned.
    exec(
        &db,
        "UPDATE links SET is_pinned = true \
         WHERE id = (SELECT min(id) FROM links WHERE user_id = $1)",
        vec![user_id.into()],
    )
    .await;

    let walked = walk(&server, &token, "/links", 70).await;
    purge(&db, user_id).await;

    let created: Vec<&String> = walked.iter().flat_map(|p| &p.created_at).collect();
    assert_eq!(created.len(), SEEDED as usize);
    assert!(
        created.iter().all(|c| *c == created[0]),
        "the seed must tie on created_at for this test to mean anything"
    );

    let ids: Vec<i64> = walked.iter().flat_map(|p| p.ids.clone()).collect();
    let mut expected: Vec<i64> = ids.clone();
    expected.sort_unstable_by(|a, b| b.cmp(a));
    expected.sort_by_key(|id| *id != *ids.iter().min().unwrap());
    assert_same_order(
        &ids,
        &expected,
        "the pinned (oldest) link, then newest id first",
    );
}

/// The total counts what the filters match, not the page and not the account.
#[tokio::test]
async fn links_list_total_follows_the_filters() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (_other_token, other_id) = register_verified(&server, &db).await;
    seed_links(&db, user_id, None, 7).await;
    seed_links(&db, other_id, None, 3).await;
    exec(
        &db,
        "UPDATE links SET title = 'needle' \
         WHERE id IN (SELECT id FROM links WHERE user_id = $1 ORDER BY id LIMIT 3)",
        vec![user_id.into()],
    )
    .await;

    let all = page(&server, &token, "/links").await;
    let found = page(&server, &token, "/links?search=needle&limit=1").await;
    let none = page(&server, &token, "/links?search=absent").await;

    let victim = all.ids[0];
    let deleted = server
        .post("/links/bulk/delete")
        .authorization_bearer(&token)
        .json(&json!({ "ids": [victim] }))
        .await;
    assert_eq!(deleted.status_code(), 200, "delete: {}", deleted.text());
    let after_delete = page(&server, &token, "/links").await;
    purge(&db, user_id).await;
    purge(&db, other_id).await;

    assert_eq!(all.total, 7, "only the caller's links");
    assert_eq!(found.total, 3);
    assert_eq!(found.ids.len(), 1);
    assert_eq!(none.total, 0);
    assert_eq!(after_delete.total, 6, "soft-deleted links are not counted");
}

#[tokio::test]
async fn folder_links_page_with_a_total() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let folder = server
        .post("/folders")
        .authorization_bearer(&token)
        .json(&json!({ "name": "big" }))
        .await;
    assert_eq!(folder.status_code(), 201, "folder: {}", folder.text());
    let folder_id = folder.json::<Value>()["id"].as_i64().unwrap() as i32;
    seed_links(&db, user_id, Some(folder_id), SEEDED).await;
    // A link outside the folder is not part of the total.
    seed_links(&db, user_id, None, 4).await;

    let path = format!("/folders/{folder_id}/links");
    let first = page(&server, &token, &path).await;
    let rest = page(&server, &token, &format!("{path}?offset=1000")).await;
    let walked = walk(&server, &token, &path, 70).await;
    purge(&db, user_id).await;

    assert_eq!(first.total, SEEDED as u64);
    assert_eq!(first.ids.len(), PAGE_CAP);
    assert_eq!(rest.total, SEEDED as u64);
    assert_eq!(rest.ids.len(), SEEDED as usize - PAGE_CAP);

    let ids: Vec<i64> = walked.iter().flat_map(|p| p.ids.clone()).collect();
    let mut expected = ids.clone();
    expected.sort_unstable_by(|a, b| b.cmp(a));
    expected.dedup();
    assert_eq!(ids.len(), SEEDED as usize);
    assert_same_order(&ids, &expected, "newest first, every link exactly once");
}

#[tokio::test]
async fn tag_links_page_with_a_total() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let tag = server
        .post("/tags")
        .authorization_bearer(&token)
        .json(&json!({ "name": "big" }))
        .await;
    assert_eq!(tag.status_code(), 201, "tag: {}", tag.text());
    let tag_id = tag.json::<Value>()["id"].as_i64().unwrap() as i32;
    seed_links(&db, user_id, None, SEEDED).await;
    exec(
        &db,
        "INSERT INTO link_tags (link_id, tag_id) SELECT id, $2 FROM links WHERE user_id = $1",
        vec![user_id.into(), tag_id.into()],
    )
    .await;
    // A link without the tag is not part of the total.
    seed_links(&db, user_id, None, 4).await;
    exec(
        &db,
        "DELETE FROM link_tags WHERE tag_id = $1 AND link_id IN \
         (SELECT id FROM links WHERE user_id = $2 ORDER BY id DESC LIMIT 4)",
        vec![tag_id.into(), user_id.into()],
    )
    .await;

    let path = format!("/tags/{tag_id}/links");
    let first = page(&server, &token, &path).await;
    let rest = page(&server, &token, &format!("{path}?offset=1000")).await;
    let walked = walk(&server, &token, &path, 70).await;
    purge(&db, user_id).await;

    assert_eq!(first.total, SEEDED as u64);
    assert_eq!(first.ids.len(), PAGE_CAP, "a tag list is bounded now");
    assert_eq!(rest.total, SEEDED as u64);
    assert_eq!(rest.ids.len(), SEEDED as usize - PAGE_CAP);

    let ids: Vec<i64> = walked.iter().flat_map(|p| p.ids.clone()).collect();
    let mut expected = ids.clone();
    expected.sort_unstable_by(|a, b| b.cmp(a));
    expected.dedup();
    assert_eq!(ids.len(), SEEDED as usize);
    assert_same_order(&ids, &expected, "newest first, every link exactly once");
}

#[tokio::test]
async fn openapi_documents_the_total_count_header_on_the_link_lists() {
    let (server, _db) = spawn_real_app().await;
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    for path in [
        "/links",
        "/folders/{folder_id}/links",
        "/tags/{tag_id}/links",
    ] {
        let ok = &spec["paths"][path]["get"]["responses"]["200"];
        assert!(
            ok["headers"]["X-Total-Count"].is_object(),
            "{path} must document X-Total-Count, got {ok}"
        );
        let params = spec["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap_or_else(|| panic!("{path} parameters"));
        for name in ["limit", "offset"] {
            assert!(
                params.iter().any(|p| p["name"] == name),
                "{path} must document {name}"
            );
        }
    }
}
