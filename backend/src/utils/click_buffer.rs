use parking_lot::RwLock;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::entity::{click_events, links};

/// Postgres caps a statement at 65535 bind parameters. Each click row binds a
/// dozen-plus columns, so 500 rows (~6.5k binds) leaves headroom if the row
/// shape grows. A single `insert_many` past that limit fails forever, even
/// after the database recovers.
const INSERT_CHUNK_SIZE: usize = 500;

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
}

/// Buffered click counter for aggregating click count updates
struct ClickCounter {
    count: i32,
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
        }
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
    fn push_event(&self, data: ClickData) -> bool {
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

    /// Check if buffer should be flushed
    pub fn should_flush(&self) -> bool {
        self.events.read().len() >= self.max_buffer_size
    }

    /// Number of clicks buffered (not yet flushed to the DB) for a link.
    /// Used so click limits account for in-flight clicks, not just the DB count.
    pub fn pending_count(&self, link_id: i32) -> i32 {
        self.counters
            .read()
            .get(&link_id)
            .map(|c| c.count)
            .unwrap_or(0)
    }

    /// Flush the buffer to the database
    pub async fn flush(&self, db: &DatabaseConnection) {
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
            return;
        }

        info!(
            "Flushing {} click events and {} counter updates",
            events.len(),
            counters.len()
        );

        // Isolate each link in its own transaction. A hard-deleted parent can
        // leave an orphan event in memory; one FK failure must not roll back and
        // lose every unrelated click in the batch.
        let mut events_by_link: HashMap<i32, Vec<ClickData>> = HashMap::new();
        for event in events {
            events_by_link.entry(event.link_id).or_default().push(event);
        }
        let mut counts: HashMap<i32, i32> = counters
            .into_iter()
            .map(|(link_id, counter)| (link_id, counter.count))
            .collect();
        let link_ids: HashSet<i32> = events_by_link
            .keys()
            .chain(counts.keys())
            .copied()
            .collect();

        let mut retry_events = Vec::new();
        let mut retry_counts: HashMap<i32, i32> = HashMap::new();

        for link_id in link_ids {
            let link_events = events_by_link.remove(&link_id).unwrap_or_default();
            let count = counts.remove(&link_id).unwrap_or(0);

            let txn = match db.begin().await {
                Ok(txn) => txn,
                Err(e) => {
                    error!(
                        "Click flush: failed to open transaction for link {}: {}",
                        link_id, e
                    );
                    retry_events.extend(link_events);
                    if count > 0 {
                        retry_counts.insert(link_id, count);
                    }
                    continue;
                }
            };

            // Lock the active parent while its events and counter are written.
            // Missing/deleted parents are isolated and discarded; they cannot
            // poison valid links in the same flush.
            let parent = links::Entity::find_by_id(link_id)
                .filter(links::Column::DeletedAt.is_null())
                .lock_shared()
                .one(&txn)
                .await;
            match parent {
                Ok(Some(_)) => {}
                Ok(None) => {
                    warn!(
                        "Click flush: discarded {} orphan events and {} counter increments for link {}",
                        link_events.len(),
                        count,
                        link_id
                    );
                    let _ = txn.rollback().await;
                    continue;
                }
                Err(e) => {
                    error!(
                        "Click flush: failed to validate parent link {}: {}",
                        link_id, e
                    );
                    let _ = txn.rollback().await;
                    retry_events.extend(link_events);
                    if count > 0 {
                        retry_counts.insert(link_id, count);
                    }
                    continue;
                }
            }

            let persist_result = async {
                if !link_events.is_empty() {
                    let models: Vec<click_events::ActiveModel> = link_events
                        .iter()
                        .cloned()
                        .map(|e| click_events::ActiveModel {
                            link_id: Set(e.link_id),
                            ip_address: Set(e.ip_address),
                            user_agent: Set(e.user_agent),
                            referer: Set(e.referer),
                            country: Set(e.country),
                            city: Set(e.city),
                            region: Set(e.region),
                            latitude: Set(e.latitude),
                            longitude: Set(e.longitude),
                            device: Set(e.device),
                            browser: Set(e.browser),
                            os: Set(e.os),
                            ..Default::default()
                        })
                        .collect();
                    for chunk in models.chunks(INSERT_CHUNK_SIZE) {
                        click_events::Entity::insert_many(chunk.to_vec())
                            .exec(&txn)
                            .await?;
                    }
                }

                if count > 0 {
                    use sea_orm::sea_query::Expr;
                    links::Entity::update_many()
                        .col_expr(
                            links::Column::ClickCount,
                            Expr::col(links::Column::ClickCount).add(count),
                        )
                        .filter(links::Column::Id.eq(link_id))
                        .exec(&txn)
                        .await?;
                }

                txn.commit().await
            }
            .await;

            if let Err(e) = persist_result {
                error!(
                    "Click flush: failed to persist link {} (will retry {} events / {} increments): {}",
                    link_id,
                    link_events.len(),
                    count,
                    e
                );
                retry_events.extend(link_events);
                if count > 0 {
                    retry_counts.insert(link_id, count);
                }
            }
        }

        // Transient DB failures are requeued ahead of newly arrived clicks.
        // Orphans are deliberately not requeued, avoiding an infinite poison
        // loop after their parent link has been hard-deleted.
        if !retry_events.is_empty() {
            let mut dropped_per_link: HashMap<i32, i32> = HashMap::new();
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

        if self.should_flush() {
            self.flush_notify.notify_one();
        }
    }

    /// Start the background flush task
    pub fn start_flush_task(self: Arc<Self>, db: DatabaseConnection) {
        let interval_secs = self.flush_interval_secs;

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));

            loop {
                // Flush on the timer, or early when the buffer signals it is full.
                tokio::select! {
                    _ = ticker.tick() => {}
                    _ = self.flush_notify.notified() => {}
                }
                self.flush(&db).await;
            }
        });
    }
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
        }
    }
}
