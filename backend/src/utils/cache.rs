use redis::{AsyncCommands, Client};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Cached link data for fast redirects
#[derive(Clone, Debug)]
pub struct CachedLink {
    pub id: i32,
    pub original_url: String,
    pub has_password: bool,
    pub expires_at: Option<i64>,
    pub starts_at: Option<i64>,
    pub max_clicks: Option<i32>,
    pub click_count: i32,
    pub user_id: Option<i32>,
}

impl CachedLink {
    pub fn to_redis_value(&self) -> String {
        serde_json::json!({
            "id": self.id,
            "original_url": self.original_url,
            "has_password": self.has_password,
            "expires_at": self.expires_at,
            "starts_at": self.starts_at,
            "max_clicks": self.max_clicks,
            "click_count": self.click_count,
            "user_id": self.user_id,
        }).to_string()
    }

    pub fn from_redis_value(value: &str) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_str(value).ok()?;
        Some(CachedLink {
            id: json["id"].as_i64()? as i32,
            original_url: json["original_url"].as_str()?.to_string(),
            has_password: json["has_password"].as_bool()?,
            expires_at: json["expires_at"].as_i64(),
            starts_at: json["starts_at"].as_i64(),
            max_clicks: json["max_clicks"].as_i64().map(|n| n as i32),
            click_count: json["click_count"].as_i64()? as i32,
            user_id: json["user_id"].as_i64().map(|n| n as i32),
        })
    }
}

/// Redis cache manager for link lookups
pub struct RedisCache {
    client: Option<Client>,
    connection: Arc<RwLock<Option<redis::aio::ConnectionManager>>>,
    ttl_seconds: u64,
}

impl RedisCache {
    /// Create a new Redis cache instance
    /// Returns None if REDIS_URL is not set (cache disabled)
    pub async fn new() -> Option<Self> {
        let redis_url = std::env::var("REDIS_URL").ok()?;
        let ttl = std::env::var("REDIS_CACHE_TTL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300); // Default 5 minutes

        match Client::open(redis_url.as_str()) {
            Ok(client) => {
                match client.get_connection_manager().await {
                    Ok(conn) => {
                        info!("Redis cache connected successfully");
                        Some(Self {
                            client: Some(client),
                            connection: Arc::new(RwLock::new(Some(conn))),
                            ttl_seconds: ttl,
                        })
                    }
                    Err(e) => {
                        warn!("Failed to connect to Redis: {}. Cache disabled.", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("Failed to create Redis client: {}. Cache disabled.", e);
                None
            }
        }
    }

    /// Get a cached link by code
    pub async fn get_link(&self, code: &str) -> Option<CachedLink> {
        let conn_guard = self.connection.read().await;
        let conn = conn_guard.as_ref()?;
        
        let key = format!("link:{}", code);
        let result: Option<String> = conn.clone().get(&key).await.ok()?;
        
        result.and_then(|v| CachedLink::from_redis_value(&v))
    }

    /// Cache a link
    pub async fn set_link(&self, code: &str, link: &CachedLink) -> Result<(), redis::RedisError> {
        let conn_guard = self.connection.read().await;
        if let Some(conn) = conn_guard.as_ref() {
            let key = format!("link:{}", code);
            let value = link.to_redis_value();
            let _: () = conn.clone().set_ex(&key, value, self.ttl_seconds).await?;
        }
        Ok(())
    }

    /// Invalidate a cached link
    pub async fn invalidate_link(&self, code: &str) -> Result<(), redis::RedisError> {
        let conn_guard = self.connection.read().await;
        if let Some(conn) = conn_guard.as_ref() {
            let key = format!("link:{}", code);
            let _: () = conn.clone().del(&key).await?;
        }
        Ok(())
    }

    /// Update click count in cache
    pub async fn increment_clicks(&self, code: &str) -> Result<(), redis::RedisError> {
        // Invalidate the cache so the next request fetches fresh data
        self.invalidate_link(code).await
    }

    /// Check if Redis is connected
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }
}

impl Clone for RedisCache {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            connection: self.connection.clone(),
            ttl_seconds: self.ttl_seconds,
        }
    }
}

