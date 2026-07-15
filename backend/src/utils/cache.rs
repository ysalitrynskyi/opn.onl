use redis::{Client, Script};
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
    /// When true, redirect must go through the frontend interstitial first.
    pub safe_link_interstitial: bool,
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
            "safe_link_interstitial": self.safe_link_interstitial,
        })
        .to_string()
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
            safe_link_interstitial: json["safe_link_interstitial"].as_bool().unwrap_or(false),
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
            Ok(client) => match client.get_connection_manager().await {
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
            },
            Err(e) => {
                warn!("Failed to create Redis client: {}. Cache disabled.", e);
                None
            }
        }
    }

    fn link_key(code: &str) -> String {
        format!("link:{}", code)
    }

    fn generation_key(code: &str) -> String {
        format!("link_generation:{}", code)
    }

    /// Read a cached link and its invalidation generation in one Redis command.
    ///
    /// Writers capture this generation before loading from Postgres and may only
    /// populate the cache if it is unchanged. An invalidation that races the DB
    /// lookup therefore prevents the stale row from being written back.
    pub async fn get_link_versioned(
        &self,
        code: &str,
    ) -> Result<(Option<CachedLink>, u64), redis::RedisError> {
        let conn_guard = self.connection.read().await;
        let Some(conn) = conn_guard.as_ref() else {
            return Ok((None, 0));
        };

        let mut conn = conn.clone();
        let (value, generation): (Option<String>, Option<u64>) = redis::cmd("MGET")
            .arg(Self::link_key(code))
            .arg(Self::generation_key(code))
            .query_async(&mut conn)
            .await?;

        Ok((
            value.as_deref().and_then(CachedLink::from_redis_value),
            generation.unwrap_or(0),
        ))
    }

    /// Cache a link only if no invalidation occurred since the caller's read.
    pub async fn set_link_if_generation(
        &self,
        code: &str,
        expected_generation: u64,
        link: &CachedLink,
    ) -> Result<bool, redis::RedisError> {
        let conn_guard = self.connection.read().await;
        let Some(conn) = conn_guard.as_ref() else {
            return Ok(false);
        };

        let mut conn = conn.clone();
        let wrote: i32 = Script::new(
            r#"
            local current = tonumber(redis.call('GET', KEYS[1]) or '0')
            if current ~= tonumber(ARGV[1]) then
                return 0
            end
            redis.call('SET', KEYS[2], ARGV[2], 'EX', ARGV[3])
            return 1
            "#,
        )
        .key(Self::generation_key(code))
        .key(Self::link_key(code))
        .arg(expected_generation)
        .arg(link.to_redis_value())
        .arg(self.ttl_seconds)
        .invoke_async(&mut conn)
        .await?;

        Ok(wrote == 1)
    }

    /// Atomically advance the invalidation generation and delete the cached row.
    ///
    /// The generation key intentionally outlives cached values. Expiring it could
    /// let a very slow stale writer observe generation zero again.
    pub async fn invalidate_link(&self, code: &str) -> Result<(), redis::RedisError> {
        let conn_guard = self.connection.read().await;
        if let Some(conn) = conn_guard.as_ref() {
            let mut conn = conn.clone();
            let _: i32 = Script::new(
                r#"
                redis.call('INCR', KEYS[1])
                return redis.call('DEL', KEYS[2])
                "#,
            )
            .key(Self::generation_key(code))
            .key(Self::link_key(code))
            .invoke_async(&mut conn)
            .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cached(url: &str) -> CachedLink {
        CachedLink {
            id: 1,
            original_url: url.to_string(),
            has_password: false,
            expires_at: None,
            starts_at: None,
            max_clicks: None,
            click_count: 0,
            user_id: Some(1),
            safe_link_interstitial: false,
        }
    }

    #[test]
    fn cached_link_roundtrips() {
        let original = cached("https://example.com/path");
        let decoded = CachedLink::from_redis_value(&original.to_redis_value()).unwrap();
        assert_eq!(decoded.id, original.id);
        assert_eq!(decoded.original_url, original.original_url);
        assert_eq!(decoded.user_id, original.user_id);
    }

    #[tokio::test]
    async fn invalidation_generation_rejects_stale_writer() {
        let Some(cache) = RedisCache::new().await else {
            eprintln!("skipping Redis race test: REDIS_URL is not set or unavailable");
            return;
        };
        let code = format!("cache-race-{}", uuid::Uuid::new_v4());

        let (_, generation) = cache.get_link_versioned(&code).await.unwrap();
        cache.invalidate_link(&code).await.unwrap();

        assert!(
            !cache
                .set_link_if_generation(&code, generation, &cached("https://stale.example"))
                .await
                .unwrap(),
            "a writer that started before invalidation must not repopulate stale data"
        );

        let (value, new_generation) = cache.get_link_versioned(&code).await.unwrap();
        assert!(value.is_none());
        assert!(new_generation > generation);

        cache.invalidate_link(&code).await.unwrap();
    }
}
