use parking_lot::RwLock;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info};

use crate::entity::click_events;

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
    /// Flush interval in seconds
    flush_interval_secs: u64,
    /// Signals the flush task to flush early once the buffer reaches max_buffer_size.
    flush_notify: Arc<tokio::sync::Notify>,
}

impl ClickBuffer {
    pub fn new() -> Self {
        let max_buffer_size = std::env::var("CLICK_BUFFER_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);
        
        let flush_interval_secs = std::env::var("CLICK_FLUSH_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        Self {
            events: Arc::new(RwLock::new(Vec::with_capacity(max_buffer_size))),
            counters: Arc::new(RwLock::new(HashMap::new())),
            max_buffer_size,
            flush_interval_secs,
            flush_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Add a click event to the buffer
    pub fn add_click(&self, data: ClickData) {
        let link_id = data.link_id;

        // Add to events buffer
        let should_flush = {
            let mut events = self.events.write();
            events.push(data);
            events.len() >= self.max_buffer_size
        };

        // Increment counter
        {
            let mut counters = self.counters.write();
            counters
                .entry(link_id)
                .and_modify(|c| c.count += 1)
                .or_insert(ClickCounter { count: 1 });
        }

        // Trigger an early flush when the buffer is full so it can't grow
        // unbounded between timer ticks under load.
        if should_flush {
            self.flush_notify.notify_one();
        }
    }

    /// Check if buffer should be flushed
    pub fn should_flush(&self) -> bool {
        self.events.read().len() >= self.max_buffer_size
    }

    /// Number of clicks buffered (not yet flushed to the DB) for a link.
    /// Used so click limits account for in-flight clicks, not just the DB count.
    pub fn pending_count(&self, link_id: i32) -> i32 {
        self.counters.read().get(&link_id).map(|c| c.count).unwrap_or(0)
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

        info!("Flushing {} click events and {} counter updates", events.len(), counters.len());

        // Batch insert click events
        if !events.is_empty() {
            let models: Vec<click_events::ActiveModel> = events
                .into_iter()
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

            if let Err(e) = click_events::Entity::insert_many(models).exec(db).await {
                error!("Failed to batch insert click events: {}", e);
            }
        }

        // Update click counts
        for (link_id, counter) in counters {
            use sea_orm::sea_query::Expr;
            use crate::entity::links;

            if let Err(e) = links::Entity::update_many()
                .col_expr(
                    links::Column::ClickCount,
                    Expr::col(links::Column::ClickCount).add(counter.count),
                )
                .filter(links::Column::Id.eq(link_id))
                .exec(db)
                .await
            {
                error!("Failed to update click count for link {}: {}", link_id, e);
            }
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
            flush_interval_secs: self.flush_interval_secs,
            flush_notify: self.flush_notify.clone(),
        }
    }
}

