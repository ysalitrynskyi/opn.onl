use parking_lot::RwLock;
use sea_orm::sea_query::{LockBehavior, LockType};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Select, Set, Statement, TransactionTrait,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::entity::{click_events, links};

/// Postgres caps a statement at 65535 bind parameters. Each click row binds a
/// dozen-plus columns, so 500 rows (~7k binds) leaves headroom if the row
/// shape grows. A single `insert_many` past that limit fails forever, even
/// after the database recovers.
const INSERT_CHUNK_SIZE: usize = 500;

/// Links whose clicks one flush transaction writes. A transaction per link
/// (a parent lock, an insert and a counter update each) made a round with
/// thousands of distinct links outlast the flush interval while the queue
/// filled towards its cap. A chunk costs one parent lock, one counter update
/// and an insert per `INSERT_CHUNK_SIZE` events, however many links it holds.
/// 500 keeps the parent lock's `IN` list (one bind per link) and the counter
/// update (two per link) far below the bind parameter limit.
const LINK_CHUNK_SIZE: usize = 500;

/// Click event data to be batched
#[derive(Clone, Debug)]
pub struct ClickData {
    pub link_id: i32,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    /// Where a routed click was sent (`click_events.destination`); `None`
    /// for a link without routing rules.
    pub destination: Option<String>,
    /// When the click happened. Stamped in `push_event` if unset, so a delayed
    /// flush does not collapse analytics onto the recover instant.
    pub created_at: Option<chrono::NaiveDateTime>,
}

/// How a flush treats a link row that another transaction holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParentLock {
    /// Wait for it (shutdown: nothing may stay in memory).
    Wait,
    /// Skip the link and requeue its clicks (the background flush).
    SkipLocked,
}

/// Buffered click counter for aggregating click count updates
struct ClickCounter {
    count: i64,
}

/// One link's share of a flush round.
struct LinkClicks {
    link_id: i32,
    events: Vec<ClickData>,
    /// What to add to `links.click_count`. A capped link's clicks add
    /// nothing: the redirect already counted them (`add_event_only`).
    count: i64,
}

/// The links of a committed chunk that it did not write.
#[derive(Default)]
struct Unwritten {
    /// Live rows another transaction holds (only under `SkipLocked`).
    held: HashSet<i32>,
    /// Deleted or purged parents. Their clicks are discarded: requeueing them
    /// would retry a write that can never succeed, forever.
    gone: HashSet<i32>,
}

/// Why a chunk wrote nothing.
enum ChunkError {
    /// No transaction could be opened. The database is unreachable, and the
    /// rest of the round would fail the same way.
    Begin(DbErr),
    /// A statement or the commit failed, and the transaction rolled back.
    Write(DbErr),
}

/// Click buffer for batching database writes
pub struct ClickBuffer {
    /// Buffer for click events
    events: Arc<RwLock<Vec<ClickData>>>,
    /// Buffer for click count increments per link
    counters: Arc<RwLock<HashMap<i32, ClickCounter>>>,
    /// Maximum buffer size before forced flush
    max_buffer_size: usize,
    /// Hard cap on queued events. `max_buffer_size` is the early-flush
    /// threshold; this bound exists so a failed flush cannot grow until OOM.
    max_queued: usize,
    /// Flush interval in seconds
    flush_interval_secs: u64,
    /// Signals the flush task to flush early once the buffer reaches max_buffer_size.
    flush_notify: Arc<tokio::sync::Notify>,
    /// Wakes the flush task so it can exit on shutdown instead of being dropped
    /// mid-statement with a taken-but-unwritten batch.
    stop: Arc<tokio::sync::Notify>,
    stopped: Arc<AtomicBool>,
    /// Serializes `flush` against shutdown and against `take_pending_count` /
    /// `add_pending_count`, so a cap-write cannot fold from an empty map while
    /// an in-flight flush still holds those counters.
    flush_lock: Arc<tokio::sync::Mutex<()>>,
    flush_attempts: Arc<AtomicU64>,
}

impl Default for ClickBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickBuffer {
    pub fn new() -> Self {
        let max_buffer_size: usize = std::env::var("CLICK_BUFFER_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);

        let flush_interval_secs: u64 = std::env::var("CLICK_FLUSH_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        // Early-flush threshold is small (default 100). The hard cap is a
        // multiple of that so brief DB slowness can still queue, but a stuck
        // flush cannot grow without bound.
        let max_queued = max_buffer_size.saturating_mul(100).max(max_buffer_size);
        Self::with_limits(max_buffer_size, max_queued, flush_interval_secs)
    }

    /// Construct a buffer with explicit flush threshold, hard cap, and interval.
    pub fn with_limits(
        max_buffer_size: usize,
        max_queued: usize,
        flush_interval_secs: u64,
    ) -> Self {
        let max_buffer_size = max_buffer_size.max(1);
        let max_queued = max_queued.max(max_buffer_size);
        Self {
            events: Arc::new(RwLock::new(Vec::with_capacity(max_buffer_size))),
            counters: Arc::new(RwLock::new(HashMap::new())),
            max_buffer_size,
            max_queued,
            flush_interval_secs,
            flush_notify: Arc::new(tokio::sync::Notify::new()),
            stop: Arc::new(tokio::sync::Notify::new()),
            stopped: Arc::new(AtomicBool::new(false)),
            flush_lock: Arc::new(tokio::sync::Mutex::new(())),
            flush_attempts: Arc::new(AtomicU64::new(0)),
        }
    }

    /// How many times `flush` has run. Used to assert the background task
    /// backs off on database errors instead of notify-spinning.
    pub fn flush_attempt_count(&self) -> u64 {
        self.flush_attempts.load(Ordering::Relaxed)
    }

    /// Number of click events waiting to be flushed.
    pub fn queued_event_count(&self) -> usize {
        self.events.read().len()
    }

    /// Add a click event to the buffer and count it towards the link's
    /// aggregate click_count (applied to links.click_count at flush).
    pub fn add_click(&self, data: ClickData) {
        let link_id = data.link_id;
        if !self.push_event(data) {
            return;
        }

        let mut counters = self.counters.write();
        counters
            .entry(link_id)
            .and_modify(|c| c.count += 1)
            .or_insert(ClickCounter { count: 1 });
    }

    /// Buffer only the analytics event row, without touching the aggregate
    /// counter. Used for capped (max_clicks) links whose click_count was
    /// already incremented atomically at the DB — counting it here too would
    /// double-add at flush time.
    pub fn add_event_only(&self, data: ClickData) {
        self.push_event(data);
    }

    /// Returns false when the event was shed because the hard cap is full.
    fn push_event(&self, mut data: ClickData) -> bool {
        if data.created_at.is_none() {
            data.created_at = Some(chrono::Utc::now().naive_utc());
        }
        let (queued, should_flush) = {
            let mut events = self.events.write();
            if events.len() >= self.max_queued {
                warn!(
                    cap = self.max_queued,
                    "click buffer at hard cap; shedding incoming click"
                );
                return false;
            }
            events.push(data);
            let len = events.len();
            (true, len >= self.max_buffer_size)
        };

        // Trigger an early flush when the buffer hits the flush threshold so
        // it drains before the hard cap starts shedding.
        if should_flush {
            self.flush_notify.notify_one();
        }
        queued
    }

    /// Number of clicks buffered (not yet flushed to the DB) for a link.
    /// Used so click limits account for in-flight clicks, not just the DB count.
    pub fn pending_count(&self, link_id: i32) -> i64 {
        self.counters
            .read()
            .get(&link_id)
            .map(|c| c.count)
            .unwrap_or(0)
    }

    /// Remove and return the unflushed aggregate count for one link so a
    /// later max_clicks write can fold those clicks into `links.click_count`
    /// instead of letting a cap-blind flush overshoot the new cap.
    ///
    /// Takes `flush_lock` so this cannot run in the gap after flush has
    /// `mem::take`n the counters but before it commits `click_count`.
    pub async fn take_pending_count(&self, link_id: i32) -> i64 {
        let _guard = self.flush_lock.lock().await;
        self.counters
            .write()
            .remove(&link_id)
            .map(|c| c.count)
            .unwrap_or(0)
    }

    /// Put clicks back into the buffer when folding them into click_count failed.
    pub async fn add_pending_count(&self, link_id: i32, n: i64) {
        if n <= 0 {
            return;
        }
        let _guard = self.flush_lock.lock().await;
        self.counters
            .write()
            .entry(link_id)
            .and_modify(|c| c.count += n)
            .or_insert(ClickCounter { count: n });
    }

    /// Ask the background flush task to exit after its current (or next) flush.
    ///
    /// `Notify::notify_waiters` is lost if the task is inside `flush` and not
    /// polling, so the AtomicBool is the source of truth and `flush_notify`
    /// stores a permit to wake the select.
    pub fn request_stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.stop.notify_waiters();
        self.flush_notify.notify_one();
    }

    /// Flush the buffer to the database. Returns true when any events or
    /// counters were requeued because persistence failed.
    ///
    /// Waits for a link row another transaction holds, so nothing is left in
    /// memory afterwards: this is the flush that runs at shutdown.
    pub async fn flush(&self, db: &DatabaseConnection) -> bool {
        self.flush_with(db, ParentLock::Wait).await
    }

    /// The background flush. A link whose row another transaction holds (a
    /// domain block, an account deletion) is skipped and its clicks requeued
    /// for the next round, instead of holding `flush_lock` while that writer
    /// finishes: every other link's clicks would wait too, and once the queue
    /// reached `max_queued` new clicks were dropped.
    async fn flush_skipping_locked(&self, db: &DatabaseConnection) -> bool {
        self.flush_with(db, ParentLock::SkipLocked).await
    }

    async fn flush_with(&self, db: &DatabaseConnection, parent_lock: ParentLock) -> bool {
        let _guard = self.flush_lock.lock().await;
        self.flush_attempts.fetch_add(1, Ordering::Relaxed);

        // Take events from buffer
        let events: Vec<ClickData> = {
            let mut buffer = self.events.write();
            std::mem::take(&mut *buffer)
        };

        // Take counters from buffer
        let counters: HashMap<i32, ClickCounter> = {
            let mut buffer = self.counters.write();
            std::mem::take(&mut *buffer)
        };

        if events.is_empty() && counters.is_empty() {
            return false;
        }

        info!(
            "Flushing {} click events and {} counter updates",
            events.len(),
            counters.len()
        );

        // Group the round per link, in id order so the chunks are stable.
        let mut by_link: BTreeMap<i32, LinkClicks> = BTreeMap::new();
        for event in events {
            by_link
                .entry(event.link_id)
                .or_insert_with(|| LinkClicks::new(event.link_id))
                .events
                .push(event);
        }
        for (link_id, counter) in counters {
            by_link
                .entry(link_id)
                .or_insert_with(|| LinkClicks::new(link_id))
                .count = counter.count;
        }
        // Chunks still to write, the next one last.
        let mut pending: Vec<Vec<LinkClicks>> = Vec::new();
        for link in by_link.into_values() {
            match pending.last_mut() {
                Some(chunk) if chunk.len() < LINK_CHUNK_SIZE => chunk.push(link),
                _ => pending.push(vec![link]),
            }
        }
        pending.reverse();

        let mut retry: Vec<LinkClicks> = Vec::new();
        let mut held: Vec<LinkClicks> = Vec::new();
        let mut unreachable_db = false;

        // Every chunk is written with SKIP LOCKED, whichever flush this is:
        // waiting for one held row while holding the share locks of a whole
        // chunk could deadlock with the transaction that holds it (an account
        // deletion updates many of one owner's links), and would hold up every
        // other link of the chunk.
        while let Some(mut chunk) = pending.pop() {
            match write_chunk(db, &chunk, ParentLock::SkipLocked).await {
                Ok(unwritten) => {
                    for link in chunk {
                        if unwritten.held.contains(&link.link_id) {
                            held.push(link);
                        } else if unwritten.gone.contains(&link.link_id) {
                            discard_orphan(&link);
                        }
                    }
                }
                Err(ChunkError::Begin(e)) => {
                    let links = chunk.len() + pending.iter().map(Vec::len).sum::<usize>();
                    error!(
                        "Click flush: failed to open a transaction (will retry {} links): {}",
                        links, e
                    );
                    retry.extend(chunk);
                    retry.extend(pending.drain(..).flatten());
                    unreachable_db = true;
                }
                // One bad link must not cost the others their clicks. Halving
                // the chunk until the failure is down to single links isolates
                // it in a few transactions, not one per link, which matters
                // because a link whose rows Postgres rejects is retried (and
                // fails again) every round.
                Err(ChunkError::Write(e)) if chunk.len() > 1 => {
                    warn!(
                        "Click flush: writing {} links in one transaction failed, splitting them: {}",
                        chunk.len(),
                        e
                    );
                    let second_half = chunk.split_off(chunk.len() / 2);
                    pending.push(second_half);
                    pending.push(chunk);
                }
                Err(ChunkError::Write(e)) => {
                    for link in &chunk {
                        log_write_failure(link, &e);
                    }
                    retry.extend(chunk);
                }
            }
        }

        match parent_lock {
            // Retried next round, once whoever holds the row is done with it.
            ParentLock::SkipLocked => retry.extend(held),
            // Nothing may stay in memory, so wait for each held row, in a
            // transaction of its own as before.
            ParentLock::Wait => {
                for link in held {
                    if unreachable_db {
                        retry.push(link);
                        continue;
                    }
                    match write_chunk(db, std::slice::from_ref(&link), ParentLock::Wait).await {
                        Ok(unwritten) => {
                            if unwritten.gone.contains(&link.link_id) {
                                discard_orphan(&link);
                            }
                        }
                        Err(ChunkError::Begin(e)) => {
                            error!(
                                "Click flush: failed to open a transaction for link {}: {}",
                                link.link_id, e
                            );
                            unreachable_db = true;
                            retry.push(link);
                        }
                        Err(ChunkError::Write(e)) => {
                            log_write_failure(&link, &e);
                            retry.push(link);
                        }
                    }
                }
            }
        }

        let mut retry_events = Vec::new();
        let mut retry_counts: HashMap<i32, i64> = HashMap::new();
        for link in retry {
            retry_events.extend(link.events);
            if link.count > 0 {
                retry_counts.insert(link.link_id, link.count);
            }
        }

        // Transient DB failures are requeued ahead of newly arrived clicks.
        // Orphans are deliberately not requeued, avoiding an infinite poison
        // loop after their parent link has been hard-deleted.
        let had_retry = !retry_events.is_empty() || !retry_counts.is_empty();
        if !retry_events.is_empty() {
            let mut dropped_per_link: HashMap<i32, i64> = HashMap::new();
            {
                let mut buffer = self.events.write();
                retry_events.append(&mut *buffer);
                if retry_events.len() > self.max_queued {
                    let dropped = retry_events.len() - self.max_queued;
                    for event in retry_events.drain(self.max_queued..) {
                        *dropped_per_link.entry(event.link_id).or_insert(0) += 1;
                    }
                    warn!(
                        dropped,
                        cap = self.max_queued,
                        "click buffer cap: dropped requeued events"
                    );
                }
                *buffer = retry_events;
            }
            for (link_id, mut n) in dropped_per_link {
                if let Some(count) = retry_counts.get_mut(&link_id) {
                    let take = (*count).min(n);
                    *count -= take;
                    n -= take;
                }
                if n > 0 {
                    let mut counters = self.counters.write();
                    if let Some(counter) = counters.get_mut(&link_id) {
                        counter.count = (counter.count - n).max(0);
                        if counter.count == 0 {
                            counters.remove(&link_id);
                        }
                    }
                }
            }
        }
        if !retry_counts.is_empty() {
            let mut buffer = self.counters.write();
            for (link_id, count) in retry_counts {
                if count <= 0 {
                    continue;
                }
                buffer
                    .entry(link_id)
                    .and_modify(|counter| counter.count += count)
                    .or_insert(ClickCounter { count });
            }
        }

        // A failed flush that requeues past the threshold used to notify
        // immediately, spinning the background task until the database
        // recovered (or the process ran out of memory). Pace retries with
        // the timer and the sleep in `start_flush_task`; new clicks still
        // notify via `push_event`.
        had_retry
    }

    /// Start the background flush task
    pub fn start_flush_task(
        self: Arc<Self>,
        db: DatabaseConnection,
    ) -> tokio::task::JoinHandle<()> {
        let interval_secs = self.flush_interval_secs.max(1);

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));
            let mut backoff = Duration::from_millis(200);

            loop {
                if self.stopped.load(Ordering::SeqCst) {
                    let _ = self.flush(&db).await;
                    break;
                }
                // Flush on the timer, or early when the buffer signals it is full.
                tokio::select! {
                    _ = ticker.tick() => {}
                    _ = self.flush_notify.notified() => {}
                    _ = self.stop.notified() => {}
                }
                if self.stopped.load(Ordering::SeqCst) {
                    let _ = self.flush(&db).await;
                    break;
                }
                let had_retry = self.flush_skipping_locked(&db).await;
                if self.stopped.load(Ordering::SeqCst) {
                    let _ = self.flush(&db).await;
                    break;
                }
                if had_retry {
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = self.stop.notified() => {}
                    }
                    if self.stopped.load(Ordering::SeqCst) {
                        let _ = self.flush(&db).await;
                        break;
                    }
                    backoff = backoff
                        .saturating_mul(2)
                        .min(Duration::from_secs(interval_secs));
                } else {
                    backoff = Duration::from_millis(200);
                }
            }
        })
    }
}

impl LinkClicks {
    fn new(link_id: i32) -> Self {
        Self {
            link_id,
            events: Vec::new(),
            count: 0,
        }
    }
}

fn discard_orphan(link: &LinkClicks) {
    warn!(
        "Click flush: discarded {} orphan events and {} counter increments for link {}",
        link.events.len(),
        link.count,
        link.link_id
    );
}

fn log_write_failure(link: &LinkClicks, e: &DbErr) {
    error!(
        "Click flush: failed to persist link {} (will retry {} events / {} increments): {}",
        link.link_id,
        link.events.len(),
        link.count,
        e
    );
}

/// The live (not soft-deleted) rows among `ids`, selecting only their id.
fn live_links(ids: Vec<i32>) -> Select<links::Entity> {
    links::Entity::find()
        .select_only()
        .column(links::Column::Id)
        .filter(links::Column::Id.is_in(ids))
        .filter(links::Column::DeletedAt.is_null())
}

/// Write a chunk of links in one transaction: lock their live parent rows with
/// one statement, insert the locked links' events, then add the locked links'
/// increments to `click_count` with one statement. A chunk either commits
/// whole or writes nothing.
async fn write_chunk(
    db: &DatabaseConnection,
    chunk: &[LinkClicks],
    parent_lock: ParentLock,
) -> Result<Unwritten, ChunkError> {
    let txn = db.begin().await.map_err(ChunkError::Begin)?;
    match write_chunk_in(&txn, chunk, parent_lock).await {
        Ok(unwritten) => {
            txn.commit().await.map_err(ChunkError::Write)?;
            Ok(unwritten)
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(ChunkError::Write(e))
        }
    }
}

async fn write_chunk_in(
    txn: &DatabaseTransaction,
    chunk: &[LinkClicks],
    parent_lock: ParentLock,
) -> Result<Unwritten, DbErr> {
    let ids: Vec<i32> = chunk.iter().map(|link| link.link_id).collect();

    // Share-lock the live parents while their events and counters are
    // written, so a soft delete or a purge cannot land in between; the events
    // of a locked parent cannot fail their foreign key. A missing or deleted
    // parent is simply not returned.
    let parents = live_links(ids.clone()).order_by_asc(links::Column::Id);
    let parents = match parent_lock {
        ParentLock::Wait => parents.lock_shared(),
        ParentLock::SkipLocked => {
            parents.lock_with_behavior(LockType::Share, LockBehavior::SkipLocked)
        }
    };
    let locked: HashSet<i32> = parents
        .into_tuple::<i32>()
        .all(txn)
        .await?
        .into_iter()
        .collect();

    let mut unwritten = Unwritten::default();
    let missing: Vec<i32> = ids.into_iter().filter(|id| !locked.contains(id)).collect();
    if !missing.is_empty() {
        // SKIP LOCKED answers a held row as if it did not exist. Tell the two
        // apart without a lock: a live row is busy, not an orphan.
        let live: HashSet<i32> = match parent_lock {
            ParentLock::SkipLocked => live_links(missing.clone())
                .into_tuple::<i32>()
                .all(txn)
                .await?
                .into_iter()
                .collect(),
            ParentLock::Wait => HashSet::new(),
        };
        for id in missing {
            if live.contains(&id) {
                unwritten.held.insert(id);
            } else {
                unwritten.gone.insert(id);
            }
        }
    }

    let mut rows: Vec<click_events::ActiveModel> = Vec::new();
    for event in chunk
        .iter()
        .filter(|link| locked.contains(&link.link_id))
        .flat_map(|link| &link.events)
    {
        rows.push(event_row(event));
        if rows.len() == INSERT_CHUNK_SIZE {
            click_events::Entity::insert_many(std::mem::take(&mut rows))
                .exec_without_returning(txn)
                .await?;
        }
    }
    if !rows.is_empty() {
        click_events::Entity::insert_many(rows)
            .exec_without_returning(txn)
            .await?;
    }

    let increments: Vec<(i32, i64)> = chunk
        .iter()
        .filter(|link| link.count > 0 && locked.contains(&link.link_id))
        .map(|link| (link.link_id, link.count))
        .collect();
    add_click_counts(txn, &increments).await?;

    Ok(unwritten)
}

fn event_row(event: &ClickData) -> click_events::ActiveModel {
    let event = event.clone();
    click_events::ActiveModel {
        link_id: Set(event.link_id),
        ip_address: Set(event.ip_address),
        user_agent: Set(event.user_agent),
        referer: Set(event.referer),
        country: Set(event.country),
        city: Set(event.city),
        region: Set(event.region),
        latitude: Set(event.latitude),
        longitude: Set(event.longitude),
        device: Set(event.device),
        browser: Set(event.browser),
        os: Set(event.os),
        destination: Set(event.destination),
        created_at: Set(event
            .created_at
            .unwrap_or_else(|| chrono::Utc::now().naive_utc())),
        ..Default::default()
    }
}

/// Add each link's buffered clicks to its `click_count`, all in one
/// statement. Each link appears once (the round is grouped per link), which
/// matters: `UPDATE ... FROM` applies only one of several matching rows.
async fn add_click_counts(
    txn: &DatabaseTransaction,
    increments: &[(i32, i64)],
) -> Result<(), DbErr> {
    if increments.is_empty() {
        return Ok(());
    }
    let mut rows = Vec::with_capacity(increments.len());
    let mut values = Vec::with_capacity(increments.len() * 2);
    for (i, &(link_id, count)) in increments.iter().enumerate() {
        rows.push(format!("(${}::integer, ${}::bigint)", 2 * i + 1, 2 * i + 2));
        values.push(link_id.into());
        values.push(count.into());
    }
    let sql = format!(
        "UPDATE links SET click_count = links.click_count + v.n \
         FROM (VALUES {}) AS v(id, n) WHERE links.id = v.id",
        rows.join(", ")
    );
    txn.execute(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        values,
    ))
    .await?;
    Ok(())
}

impl Clone for ClickBuffer {
    fn clone(&self) -> Self {
        Self {
            events: self.events.clone(),
            counters: self.counters.clone(),
            max_buffer_size: self.max_buffer_size,
            max_queued: self.max_queued,
            flush_interval_secs: self.flush_interval_secs,
            flush_notify: self.flush_notify.clone(),
            stop: self.stop.clone(),
            stopped: self.stopped.clone(),
            flush_lock: self.flush_lock.clone(),
            flush_attempts: self.flush_attempts.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(link_id: i32) -> ClickData {
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

    /// `add_click` must account every event once in the queue and once against
    /// its own link. Asserted through the queue and per-link counters rather
    /// than through `should_flush`, which had no production caller left after
    /// the flush-backoff change and was removed.
    #[test]
    fn add_click_accounts_events_in_queue_and_per_link() {
        let buf = ClickBuffer::with_limits(3, 10, 60);
        assert_eq!(buf.queued_event_count(), 0);
        buf.add_click(click(1));
        buf.add_click(click(1));
        assert_eq!(buf.queued_event_count(), 2);
        assert_eq!(buf.pending_count(1), 2);

        buf.add_click(click(2));
        assert_eq!(buf.queued_event_count(), 3);
        assert_eq!(buf.pending_count(2), 1);
        assert_eq!(buf.pending_count(1), 2);
    }
}
