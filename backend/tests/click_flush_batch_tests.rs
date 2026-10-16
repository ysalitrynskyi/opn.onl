//! The click flush writes many links per transaction.
//!
//! One transaction per link (a parent lock, an insert and a counter update
//! each) made a round with thousands of distinct links outlast the flush
//! interval, while the queue grew towards its cap and new clicks were dropped.
//! These tests pin what a batched round must still do: write every field of
//! every click, add exactly the buffered increments, skip a held row in the
//! background without losing its clicks, discard deleted parents, and keep one
//! failing link from costing the others their clicks.
//!
//! Statements are counted with sea-orm's metric callback on a connection only
//! the flush uses. It sees every statement sea-orm sends, inside transactions
//! too, but not BEGIN and COMMIT. Every flush transaction starts with exactly
//! one parent-lock statement (`... FOR SHARE`), so counting those counts the
//! transactions.

mod common;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use common::setup_test_db;
use opn_onl_backend::entity::{click_events, links};
use opn_onl_backend::utils::ClickBuffer;
use opn_onl_backend::utils::click_buffer::ClickData;
use sea_orm::{
    ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, Statement, TransactionTrait,
};

// ---------------------------------------------------------------- helpers

/// Statements one connection sent, by kind.
#[derive(Default)]
struct StatementCounts {
    total: AtomicUsize,
    parent_locks: AtomicUsize,
    inserts: AtomicUsize,
    updates: AtomicUsize,
}

impl StatementCounts {
    fn snapshot(&self) -> (usize, usize, usize, usize) {
        (
            self.total.load(Ordering::SeqCst),
            self.parent_locks.load(Ordering::SeqCst),
            self.inserts.load(Ordering::SeqCst),
            self.updates.load(Ordering::SeqCst),
        )
    }
}

/// A connection of its own for the flush, counting every statement it sends.
async fn counted_connection() -> (DatabaseConnection, Arc<StatementCounts>) {
    let counts = Arc::new(StatementCounts::default());
    let mut db = Database::connect(std::env::var("DATABASE_URL").expect("DATABASE_URL"))
        .await
        .expect("connect");
    let seen = counts.clone();
    db.set_metric_callback(move |info| {
        let sql = info.statement.sql.trim_start().to_ascii_uppercase();
        seen.total.fetch_add(1, Ordering::SeqCst);
        if sql.contains("FOR SHARE") {
            seen.parent_locks.fetch_add(1, Ordering::SeqCst);
        } else if sql.starts_with("INSERT") {
            seen.inserts.fetch_add(1, Ordering::SeqCst);
        } else if sql.starts_with("UPDATE") {
            seen.updates.fetch_add(1, Ordering::SeqCst);
        }
    });
    (db, counts)
}

/// `n` fresh links inserted with one statement, ids ascending.
async fn insert_links(db: &DatabaseConnection, n: i64) -> Vec<i32> {
    let prefix = format!("fb{}", uuid::Uuid::new_v4().simple());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "WITH ins AS ( \
                 INSERT INTO links (code, original_url) \
                 SELECT $1 || '-' || g, 'https://iana.org/flush-batch' \
                 FROM generate_series(1, $2::bigint) AS g \
                 RETURNING id \
             ) SELECT id FROM ins ORDER BY id",
            [prefix.into(), n.into()],
        ))
        .await
        .expect("insert links");
    rows.iter()
        .map(|row| row.try_get::<i32>("", "id").expect("id"))
        .collect()
}

async fn sql(db: &DatabaseConnection, stmt: String) {
    db.execute(Statement::from_string(DbBackend::Postgres, stmt))
        .await
        .expect("sql");
}

fn id_list(ids: &[i32]) -> String {
    ids.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
}

/// Removes the links (and, by cascade, their clicks) so the shared database
/// stays small.
async fn delete_links(db: &DatabaseConnection, ids: &[i32]) {
    sql(
        db,
        format!("DELETE FROM links WHERE id IN ({})", id_list(ids)),
    )
    .await;
}

async fn click_counts(db: &DatabaseConnection, ids: &[i32]) -> HashMap<i32, i64> {
    links::Entity::find()
        .filter(links::Column::Id.is_in(ids.to_vec()))
        .all(db)
        .await
        .expect("links")
        .into_iter()
        .map(|link| (link.id, link.click_count))
        .collect()
}

async fn stored_clicks(db: &DatabaseConnection, ids: &[i32]) -> Vec<click_events::Model> {
    click_events::Entity::find()
        .filter(click_events::Column::LinkId.is_in(ids.to_vec()))
        .order_by_asc(click_events::Column::Id)
        .all(db)
        .await
        .expect("click rows")
}

async fn stored_per_link(db: &DatabaseConnection, ids: &[i32]) -> HashMap<i32, usize> {
    let mut per_link = HashMap::new();
    for row in stored_clicks(db, ids).await {
        *per_link.entry(row.link_id).or_insert(0) += 1;
    }
    per_link
}

fn plain_click(link_id: i32) -> ClickData {
    ClickData {
        link_id,
        ip_address: None,
        user_agent: None,
        referer: None,
        country: None,
        city: None,
        region: None,
        latitude: None,
        longitude: None,
        device: None,
        browser: None,
        os: None,
        destination: None,
        created_at: None,
    }
}

fn base_time() -> NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
        .unwrap()
        .and_hms_milli_opt(12, 0, 0, 0)
        .unwrap()
}

/// Click `k` of the link at position `i`, every field set and distinct, so a
/// row written under the wrong link or with a field dropped is caught.
/// Every fourth link has no routing rules (`destination` NULL).
fn full_click(link_id: i32, i: usize, k: usize) -> ClickData {
    ClickData {
        link_id,
        ip_address: Some(format!("203.0.{}.0", i % 250)),
        user_agent: Some(format!("agent-{i}-{k}")),
        referer: Some(format!("ref{i}.example")),
        country: Some(if i.is_multiple_of(2) { "DE" } else { "US" }.to_string()),
        city: Some(format!("city-{i}")),
        region: Some(format!("region-{i}")),
        latitude: Some(i as f64 + 0.5),
        longitude: Some(-(i as f64) - 0.25),
        device: Some(format!("device-{}", k % 3)),
        browser: Some(format!("browser-{i}")),
        os: Some(format!("os-{k}")),
        destination: (!i.is_multiple_of(4)).then(|| format!("https://iana.org/dest/{i}")),
        created_at: Some(
            base_time()
                - chrono::Duration::seconds(i as i64)
                - chrono::Duration::milliseconds(k as i64),
        ),
    }
}

fn assert_row_matches(row: &click_events::Model, want: &ClickData) {
    let what = format!("link {} click {:?}", want.link_id, want.user_agent);
    assert_eq!(row.link_id, want.link_id, "{what}: link_id");
    assert_eq!(row.ip_address, want.ip_address, "{what}: ip_address");
    assert_eq!(row.user_agent, want.user_agent, "{what}: user_agent");
    assert_eq!(row.referer, want.referer, "{what}: referer");
    assert_eq!(row.country, want.country, "{what}: country");
    assert_eq!(row.city, want.city, "{what}: city");
    assert_eq!(row.region, want.region, "{what}: region");
    assert_eq!(row.latitude, want.latitude, "{what}: latitude");
    assert_eq!(row.longitude, want.longitude, "{what}: longitude");
    assert_eq!(row.device, want.device, "{what}: device");
    assert_eq!(row.browser, want.browser, "{what}: browser");
    assert_eq!(row.os, want.os, "{what}: os");
    assert_eq!(row.destination, want.destination, "{what}: destination");
    assert_eq!(Some(row.created_at), want.created_at, "{what}: created_at");
}

/// Holds `FOR UPDATE` on a link row from a connection of its own until the
/// returned transaction ends. Row-level, so parallel tests are unaffected.
async fn hold_row(link_id: i32) -> (DatabaseConnection, sea_orm::DatabaseTransaction) {
    let lock_db = Database::connect(std::env::var("DATABASE_URL").expect("DATABASE_URL"))
        .await
        .expect("lock conn");
    let txn = lock_db.begin().await.expect("begin");
    txn.execute(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id FROM links WHERE id = $1 FOR UPDATE",
        [link_id.into()],
    ))
    .await
    .expect("lock link");
    (lock_db, txn)
}

// ------------------------------------------------------------------ tests

/// 1,200 distinct links in one round: every click row with every field, every
/// counter exact, and a few transactions instead of one per link.
#[tokio::test]
async fn one_round_writes_many_links_in_a_few_transactions() {
    const LINKS: usize = 1200;
    let db = setup_test_db().await;
    let ids = insert_links(&db, LINKS as i64).await;
    assert_eq!(ids.len(), LINKS);

    let buffer = ClickBuffer::with_limits(100_000, 100_000, 60);
    let mut expected_rows: Vec<ClickData> = Vec::new();
    let mut expected_count: HashMap<i32, i64> = HashMap::new();
    for (i, &link_id) in ids.iter().enumerate() {
        // One to three counted clicks, so an increment of one per link (or
        // per chunk) instead of the link's own number shows.
        let counted = i % 3 + 1;
        for k in 0..counted {
            let click = full_click(link_id, i, k);
            expected_rows.push(click.clone());
            buffer.add_click(click);
        }
        let mut count = counted as i64;
        // A capped link's click: the redirect already counted it, so the
        // flush writes the event and adds nothing.
        if i.is_multiple_of(10) {
            let click = full_click(link_id, i, 9);
            expected_rows.push(click.clone());
            buffer.add_event_only(click);
        }
        // Increments without events, put back by a cap write that failed.
        if i.is_multiple_of(7) {
            buffer.add_pending_count(link_id, 5).await;
            count += 5;
        }
        expected_count.insert(link_id, count);
    }

    let (flush_db, counts) = counted_connection().await;
    let before = counts.snapshot();
    let started = Instant::now();
    let requeued = buffer.flush(&flush_db).await;
    let elapsed = started.elapsed();
    let after = counts.snapshot();
    let (total, transactions, inserts, updates) = (
        after.0 - before.0,
        after.1 - before.1,
        after.2 - before.2,
        after.3 - before.3,
    );
    eprintln!(
        "flush of {LINKS} links / {} events: {total} statements \
         ({transactions} transactions, {inserts} inserts, {updates} counter updates) in {elapsed:?}",
        expected_rows.len()
    );

    assert!(!requeued, "nothing may be requeued");
    assert_eq!(buffer.queued_event_count(), 0);
    for &link_id in &ids {
        assert_eq!(buffer.pending_count(link_id), 0);
    }

    let mut rows = stored_clicks(&db, &ids).await;
    assert_eq!(rows.len(), expected_rows.len(), "every click is one row");
    rows.sort_by(|a, b| (a.link_id, &a.user_agent).cmp(&(b.link_id, &b.user_agent)));
    expected_rows.sort_by(|a, b| (a.link_id, &a.user_agent).cmp(&(b.link_id, &b.user_agent)));
    for (row, want) in rows.iter().zip(&expected_rows) {
        assert_row_matches(row, want);
    }

    let stored = click_counts(&db, &ids).await;
    for &link_id in &ids {
        assert_eq!(
            stored.get(&link_id),
            expected_count.get(&link_id),
            "click_count of link {link_id}"
        );
    }

    // 1,200 links fit in three chunks of at most 500 links. Each chunk is one
    // transaction: one parent lock, one counter update, and its events in
    // inserts of at most 500 rows (the bind parameter limit).
    let chunks = LINKS.div_ceil(500);
    assert!(
        transactions <= chunks,
        "the round opened {transactions} transactions for {LINKS} links"
    );
    assert!(
        updates <= chunks,
        "{updates} counter updates for {LINKS} links"
    );
    assert!(
        inserts <= expected_rows.len().div_ceil(500) + chunks,
        "{inserts} inserts for {} events",
        expected_rows.len()
    );
    assert_eq!(
        total,
        transactions + inserts + updates,
        "no other statement: every parent row was there to lock"
    );

    delete_links(&db, &ids).await;
}

/// The background flush, with one row of a batch held by another transaction:
/// every other link is written at once, the held link's clicks stay queued and
/// are written once the row is free, and a deleted link in the same batch is
/// discarded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn background_flush_requeues_a_held_row_inside_a_batch() {
    const LINKS: usize = 40;
    let db = setup_test_db().await;
    let ids = insert_links(&db, LINKS as i64).await;
    let held = ids[17];
    let deleted = ids[31];
    sql(
        &db,
        format!("UPDATE links SET deleted_at = now() WHERE id = {deleted}"),
    )
    .await;
    let (_lock_db, txn) = hold_row(held).await;

    let buffer = Arc::new(ClickBuffer::with_limits(1, 10_000, 1));
    let mut per_link: HashMap<i32, usize> = HashMap::new();
    for (i, &link_id) in ids.iter().enumerate() {
        for k in 0..(i % 3 + 1) {
            buffer.add_click(full_click(link_id, i, k));
        }
        per_link.insert(link_id, i % 3 + 1);
    }
    let handle = buffer.clone().start_flush_task(db.clone());

    let free: Vec<i32> = ids
        .iter()
        .copied()
        .filter(|id| *id != held && *id != deleted)
        .collect();
    let started = Instant::now();
    loop {
        let stored = stored_per_link(&db, &free).await;
        if free.iter().all(|id| stored.get(id) == per_link.get(id)) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the free links waited for the held row: {stored:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let held_clicks = per_link[&held];
    assert!(
        stored_per_link(&db, &[held]).await.is_empty(),
        "the held link is not written yet"
    );
    assert_eq!(
        buffer.queued_event_count(),
        held_clicks,
        "only the held link's clicks stay queued; the deleted link's are dropped"
    );
    assert_eq!(buffer.pending_count(held), held_clicks as i64);
    assert_eq!(buffer.pending_count(deleted), 0);

    txn.rollback().await.expect("release the row");
    let started = Instant::now();
    while stored_per_link(&db, &[held]).await.get(&held) != Some(&held_clicks) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the held link's clicks were not retried"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    buffer.request_stop();
    let _ = handle.await;
    assert_eq!(buffer.queued_event_count(), 0);

    let stored = click_counts(&db, &ids).await;
    for &link_id in &ids {
        let want = if link_id == deleted {
            0
        } else {
            per_link[&link_id] as i64
        };
        assert_eq!(stored[&link_id], want, "click_count of link {link_id}");
    }
    assert!(stored_per_link(&db, &[deleted]).await.is_empty());

    delete_links(&db, &ids).await;
}

/// The shutdown flush writes a batch around a held row at once, then waits for
/// that row and writes it too: nothing is left in memory.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_flush_waits_for_a_held_row_inside_a_batch() {
    const LINKS: usize = 30;
    let db = setup_test_db().await;
    let ids = insert_links(&db, LINKS as i64).await;
    let held = ids[11];
    let (_lock_db, txn) = hold_row(held).await;

    let buffer = Arc::new(ClickBuffer::with_limits(10_000, 10_000, 60));
    for (i, &link_id) in ids.iter().enumerate() {
        buffer.add_click(full_click(link_id, i, 0));
        buffer.add_click(full_click(link_id, i, 1));
    }
    let flushing = {
        let buffer = buffer.clone();
        let db = db.clone();
        tokio::spawn(async move { buffer.flush(&db).await })
    };

    let free: Vec<i32> = ids.iter().copied().filter(|id| *id != held).collect();
    let started = Instant::now();
    while stored_per_link(&db, &free).await.values().sum::<usize>() != free.len() * 2 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the other links were not written while the flush waited for the held row"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !flushing.is_finished(),
        "the shutdown flush waits for the row"
    );

    txn.rollback().await.expect("release the row");
    let requeued = tokio::time::timeout(Duration::from_secs(5), flushing)
        .await
        .expect("flush finished")
        .expect("flush task");
    assert!(!requeued);
    assert_eq!(buffer.queued_event_count(), 0, "nothing left in memory");
    let stored = click_counts(&db, &ids).await;
    for &link_id in &ids {
        assert_eq!(stored[&link_id], 2, "click_count of link {link_id}");
    }
    assert_eq!(stored_per_link(&db, &[held]).await.get(&held), Some(&2));

    delete_links(&db, &ids).await;
}

/// Soft-deleted and purged parents inside a batch are discarded; the other
/// links of the batch are written as if they were not there.
#[tokio::test]
async fn deleted_and_purged_links_inside_a_batch_are_discarded() {
    const LINKS: usize = 50;
    let db = setup_test_db().await;
    let ids = insert_links(&db, LINKS as i64).await;
    let soft_deleted: HashSet<i32> = [ids[3], ids[20], ids[49]].into();
    let purged: HashSet<i32> = [ids[0], ids[27]].into();
    sql(
        &db,
        format!(
            "UPDATE links SET deleted_at = now() WHERE id IN ({})",
            id_list(&soft_deleted.iter().copied().collect::<Vec<_>>())
        ),
    )
    .await;
    delete_links(&db, &purged.iter().copied().collect::<Vec<_>>()).await;

    let buffer = ClickBuffer::with_limits(10_000, 10_000, 60);
    for (i, &link_id) in ids.iter().enumerate() {
        for k in 0..(i % 3 + 1) {
            buffer.add_click(full_click(link_id, i, k));
        }
    }

    assert!(
        !buffer.flush(&db).await,
        "an orphan is discarded, not requeued"
    );
    assert_eq!(buffer.queued_event_count(), 0);
    for &link_id in &ids {
        assert_eq!(buffer.pending_count(link_id), 0);
    }

    let stored_rows = stored_per_link(&db, &ids).await;
    let stored = click_counts(&db, &ids).await;
    for (i, &link_id) in ids.iter().enumerate() {
        if purged.contains(&link_id) {
            assert!(!stored.contains_key(&link_id));
            assert!(!stored_rows.contains_key(&link_id));
        } else if soft_deleted.contains(&link_id) {
            assert_eq!(stored[&link_id], 0, "a deleted link's count is untouched");
            assert!(!stored_rows.contains_key(&link_id));
        } else {
            assert_eq!(stored[&link_id], (i % 3 + 1) as i64, "link {link_id}");
            assert_eq!(
                stored_rows.get(&link_id),
                Some(&(i % 3 + 1)),
                "link {link_id}"
            );
        }
    }

    delete_links(&db, &ids).await;
}

/// One link whose rows Postgres rejects (text cannot hold a NUL byte) fails
/// the statement it shares with the others. The flush must isolate it: every
/// other link of the chunk is written, the failing link's clicks are requeued,
/// and isolating it does not cost a transaction per link.
#[tokio::test]
async fn a_failing_link_does_not_cost_the_rest_of_its_chunk() {
    const LINKS: usize = 64;
    let db = setup_test_db().await;
    let ids = insert_links(&db, LINKS as i64).await;
    let bad = ids[41];

    let buffer = ClickBuffer::with_limits(10_000, 10_000, 60);
    for (i, &link_id) in ids.iter().enumerate() {
        for k in 0..(i % 3 + 1) {
            let mut click = full_click(link_id, i, k);
            if link_id == bad {
                click.user_agent = Some("broken\0agent".to_string());
            }
            buffer.add_click(click);
        }
    }
    let bad_clicks = 41 % 3 + 1;

    let (flush_db, counts) = counted_connection().await;
    let before = counts.snapshot();
    assert!(
        buffer.flush(&flush_db).await,
        "the failing link's clicks are requeued"
    );
    let transactions = counts.snapshot().1 - before.1;
    eprintln!("isolating one failing link among {LINKS}: {transactions} transactions");

    let stored_rows = stored_per_link(&db, &ids).await;
    let stored = click_counts(&db, &ids).await;
    for (i, &link_id) in ids.iter().enumerate() {
        if link_id == bad {
            assert!(
                !stored_rows.contains_key(&link_id),
                "nothing of it is written"
            );
            assert_eq!(stored[&link_id], 0);
        } else {
            assert_eq!(
                stored_rows.get(&link_id),
                Some(&(i % 3 + 1)),
                "link {link_id} lost its clicks to another link's failure"
            );
            assert_eq!(stored[&link_id], (i % 3 + 1) as i64, "link {link_id}");
        }
    }
    assert_eq!(
        buffer.queued_event_count(),
        bad_clicks,
        "requeued, not dropped"
    );
    assert_eq!(buffer.pending_count(bad), bad_clicks as i64);
    assert!(
        transactions < LINKS,
        "isolating one link took {transactions} transactions for {LINKS} links"
    );

    // Still failing next round, still kept: an error is retried, as before.
    assert!(buffer.flush(&db).await);
    assert_eq!(buffer.queued_event_count(), bad_clicks);

    delete_links(&db, &ids).await;
}

/// A click with every field unset (no GeoIP database, no user agent) is still
/// written in a batch, and the batch keeps `created_at` from the click.
#[tokio::test]
async fn bare_clicks_are_written_in_a_batch() {
    let db = setup_test_db().await;
    let ids = insert_links(&db, 3).await;
    let buffer = ClickBuffer::with_limits(100, 100, 60);
    for &link_id in &ids {
        let mut click = plain_click(link_id);
        click.created_at = Some(base_time());
        buffer.add_click(click);
    }
    assert!(!buffer.flush(&db).await);
    let rows = stored_clicks(&db, &ids).await;
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_row_matches(
            row,
            &ClickData {
                created_at: Some(base_time()),
                ..plain_click(row.link_id)
            },
        );
    }
    delete_links(&db, &ids).await;
}
