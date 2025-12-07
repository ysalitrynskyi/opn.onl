use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use parking_lot::Mutex;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Rate limiter configuration
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum requests allowed in the window
    pub max_requests: u32,
    /// Time window duration
    pub window_duration: Duration,
}

impl RateLimitConfig {
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            max_requests,
            window_duration: Duration::from_secs(window_seconds),
        }
    }
}

/// Rate limiter entry for tracking requests
#[derive(Debug)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// In-memory rate limiter state
#[derive(Debug)]
pub struct RateLimiter {
    entries: DashMap<String, Mutex<RateLimitEntry>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            entries: DashMap::new(),
            config,
        }
    }

    /// Check if a request is allowed and increment counter
    pub fn check(&self, key: &str) -> RateLimitResult {
        let now = Instant::now();
        
        // Get or create entry
        let entry = self.entries.entry(key.to_string()).or_insert_with(|| {
            Mutex::new(RateLimitEntry {
                count: 0,
                window_start: now,
            })
        });

        let mut entry = entry.lock();

        // Reset window if expired
        if now.duration_since(entry.window_start) >= self.config.window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        // Check limit
        if entry.count >= self.config.max_requests {
            let retry_after = self.config.window_duration
                .checked_sub(now.duration_since(entry.window_start))
                .unwrap_or(Duration::ZERO);
            
            return RateLimitResult::Limited {
                retry_after_secs: retry_after.as_secs(),
                limit: self.config.max_requests,
                remaining: 0,
            };
        }

        // Increment and allow
        entry.count += 1;
        let remaining = self.config.max_requests.saturating_sub(entry.count);

        RateLimitResult::Allowed {
            limit: self.config.max_requests,
            remaining,
        }
    }

    /// Clean up old entries periodically
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.entries.retain(|_, v| {
            let entry = v.lock();
            now.duration_since(entry.window_start) < self.config.window_duration * 2
        });
    }
}

/// Result of rate limit check
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed {
        limit: u32,
        remaining: u32,
    },
    Limited {
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
    },
}

/// Different rate limit tiers
#[derive(Clone)]
pub struct RateLimiters {
    /// Strict per-IP rate limiter (1 request per second)
    pub per_second: Arc<RateLimiter>,
    /// General API rate limiter (100 requests per minute)
    pub general: Arc<RateLimiter>,
    /// Link creation rate limiter (50 per hour)
    pub link_creation: Arc<RateLimiter>,
    /// Authentication rate limiter (10 per minute)
    pub auth: Arc<RateLimiter>,
    /// Link redirect rate limiter (100 per second per IP - more relaxed for redirects)
    pub redirect: Arc<RateLimiter>,
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self {
            // Increased from 1/sec to 10/sec for better UX
            per_second: Arc::new(RateLimiter::new(RateLimitConfig::new(10, 1))),
            general: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 60))),
            // Increased from 50/hour to 100/hour for link creation
            link_creation: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 3600))),
            auth: Arc::new(RateLimiter::new(RateLimitConfig::new(10, 60))),
            redirect: Arc::new(RateLimiter::new(RateLimitConfig::new(100, 1))),
        }
    }
}

impl RateLimiters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn background cleanup task
    pub fn spawn_cleanup_task(limiters: Arc<RateLimiters>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;
                limiters.per_second.cleanup();
                limiters.general.cleanup();
                limiters.link_creation.cleanup();
                limiters.auth.cleanup();
                limiters.redirect.cleanup();
                tracing::debug!("Rate limiter cleanup completed");
            }
        });
    }
}

/// Extract IP address from request
pub fn extract_ip(req: &Request<Body>) -> String {
    // Try X-Forwarded-For header first (for proxied requests)
    if let Some(xff) = req.headers().get("x-forwarded-for") {
        if let Ok(xff_str) = xff.to_str() {
            if let Some(first_ip) = xff_str.split(',').next() {
                let ip = first_ip.trim();
                if ip.parse::<IpAddr>().is_ok() {
                    return ip.to_string();
                }
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            if ip_str.parse::<IpAddr>().is_ok() {
                return ip_str.to_string();
            }
        }
    }

    // Fallback to connection info or default
    "unknown".to_string()
}

/// Rate limit middleware for general API endpoints
pub async fn rate_limit_middleware(
    State(limiters): State<Arc<RateLimiters>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = extract_ip(&req);
    let path = req.uri().path();

    // First check per-second rate limit (1 req/sec per IP) for non-redirect paths
    let is_redirect = path.len() <= 10 && !path.starts_with("/a") && !path.starts_with("/l") 
        && !path.starts_with("/o") && !path.starts_with("/f") && !path.starts_with("/t") 
        && !path.starts_with("/w") && !path.starts_with("/s") && !path.starts_with("/h");
    
    if !is_redirect {
        if let RateLimitResult::Limited { retry_after_secs, limit, remaining } = 
            limiters.per_second.check(&format!("sec:{}", ip)) 
        {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({
                    "error": "Too many requests",
                    "retry_after": retry_after_secs,
                    "message": format!("Rate limit: maximum {} requests per second", limit)
                }).to_string(),
            ).into_response();
            
            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert("X-RateLimit-Remaining", remaining.to_string().parse().unwrap());
            headers.insert("Retry-After", retry_after_secs.to_string().parse().unwrap());
            headers.insert("Content-Type", "application/json".parse().unwrap());
            
            return response;
        }
    }

    // Choose appropriate limiter based on path
    let result = if path.starts_with("/auth") {
        limiters.auth.check(&format!("auth:{}", ip))
    } else if path.starts_with("/links") && req.method() == axum::http::Method::POST {
        limiters.link_creation.check(&format!("create:{}", ip))
    } else if is_redirect {
        // Short code redirect - more relaxed
        limiters.redirect.check(&format!("redirect:{}", ip))
    } else {
        limiters.general.check(&format!("general:{}", ip))
    };

    match result {
        RateLimitResult::Allowed { limit, remaining } => {
            let mut response = next.run(req).await;
            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert("X-RateLimit-Remaining", remaining.to_string().parse().unwrap());
            response
        }
        RateLimitResult::Limited { retry_after_secs, limit, remaining } => {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({
                    "error": "Too many requests",
                    "retry_after": retry_after_secs,
                    "message": format!("Rate limit exceeded. Please try again in {} seconds.", retry_after_secs)
                }).to_string(),
            ).into_response();
            
            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            headers.insert("X-RateLimit-Remaining", remaining.to_string().parse().unwrap());
            headers.insert("Retry-After", retry_after_secs.to_string().parse().unwrap());
            headers.insert("Content-Type", "application/json".parse().unwrap());
            
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(3, 60));
        
        assert!(matches!(limiter.check("test"), RateLimitResult::Allowed { remaining: 2, .. }));
        assert!(matches!(limiter.check("test"), RateLimitResult::Allowed { remaining: 1, .. }));
        assert!(matches!(limiter.check("test"), RateLimitResult::Allowed { remaining: 0, .. }));
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(2, 60));
        
        limiter.check("test");
        limiter.check("test");
        
        assert!(matches!(limiter.check("test"), RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_rate_limiter_separate_keys() {
        let limiter = RateLimiter::new(RateLimitConfig::new(1, 60));
        
        assert!(matches!(limiter.check("user1"), RateLimitResult::Allowed { .. }));
        assert!(matches!(limiter.check("user2"), RateLimitResult::Allowed { .. }));
        assert!(matches!(limiter.check("user1"), RateLimitResult::Limited { .. }));
    }
}

