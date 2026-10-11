//! `GET /links/sparklines` must count a week of clicks in the database, not
//! load every click row into the process.
//!
//! This is its own test binary because it installs a counting
//! `#[global_allocator]`, which would otherwise count every other test too.
//! Tests here take `serial()` so a second test could not pollute the numbers.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseBackend, Statement};
use serde_json::{Value, json};

// ------------------------------------------------------------ counting allocator

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grow(bytes: usize) {
    let now = CURRENT.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(now, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            if new_size >= layout.size() {
                grow(new_size - layout.size());
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Relaxed);
            }
        }
        new_ptr
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Start a measurement: the peak so far becomes the live size right now.
fn reset_peak() -> usize {
    let current = CURRENT.load(Relaxed);
    PEAK.store(current, Relaxed);
    current
}

async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

// ------------------------------------------------------------------- test

const SEED_ROWS: i32 = 100_000;
/// Loading the rows cost about 700 bytes each (70 MiB for the seed below); the
/// aggregate needs a few KiB. The limit leaves room for the request itself.
const PEAK_LIMIT: usize = 16 * 1024 * 1024;

#[tokio::test]
async fn sparklines_are_counted_in_sql_not_loaded_into_memory() {
    let _guard = serial().await;
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

    let link_id = links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set("https://example.com".to_string()),
        user_id: Set(Some(user_id)),
        click_count: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert link")
    .id;

    // Rows with a realistic payload, spread over the last five days: inside the
    // seven-day window even if the UTC day rolls over while the test runs.
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO click_events \
            (link_id, created_at, ip_address, user_agent, referer, country, city, region, \
             latitude, longitude, device, browser, os) \
         SELECT $1::int, (now() at time zone 'utc') - random() * interval '5 days', \
                '203.0.113.0', \
                'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36', \
                'example.com', 'United States', 'Austin', 'Texas', 30.27, -97.74, \
                'Desktop', 'Chrome', 'Windows 10' \
         FROM generate_series(1, $2::int)",
        [link_id.into(), SEED_ROWS.into()],
    ))
    .await
    .expect("seed click_events");

    let baseline = reset_peak();
    let res = server
        .get("/links/sparklines")
        .add_query_param("ids", link_id.to_string())
        .authorization_bearer(&token)
        .await;
    let peak_growth = PEAK.load(Relaxed).saturating_sub(baseline);

    let status = res.status_code();
    let body: Value = res.json();
    let total = body["sparklines"][0]["total"].as_i64();

    // Remove the seed before asserting, so a failure does not leave it behind.
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "DELETE FROM click_events WHERE link_id = $1",
        [link_id.into()],
    ))
    .await
    .expect("delete seeded click_events");
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "DELETE FROM links WHERE id = $1",
        [link_id.into()],
    ))
    .await
    .expect("delete seeded link");

    assert_eq!(status, 200);
    assert_eq!(
        total,
        Some(SEED_ROWS as i64),
        "every seeded click is counted"
    );
    assert!(
        peak_growth < PEAK_LIMIT,
        "peak heap grew by {:.1} MiB for {SEED_ROWS} click rows (limit {} MiB): \
         the rows are being loaded instead of counted in SQL",
        peak_growth as f64 / (1024.0 * 1024.0),
        PEAK_LIMIT / (1024 * 1024)
    );
}
