mod common;

// ============= JWT Tests =============

#[cfg(test)]
mod jwt_comprehensive_tests {
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, Algorithm};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
    struct Claims {
        sub: String,
        exp: usize,
        user_id: i32,
    }

    const TEST_SECRET: &str = "test-secret-key-minimum-32-characters-for-safety";

    fn create_test_token(user_id: i32, email: &str, hours: i64) -> String {
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(hours))
            .expect("valid timestamp")
            .timestamp();

        let claims = Claims {
            sub: email.to_owned(),
            exp: expiration as usize,
            user_id,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        ).expect("Failed to encode JWT")
    }

    #[test]
    fn test_create_valid_token() {
        let token = create_test_token(1, "test@example.com", 24);
        assert!(!token.is_empty());
        assert!(token.contains('.'));
        assert_eq!(token.matches('.').count(), 2); // JWT has 3 parts
    }

    #[test]
    fn test_decode_valid_token() {
        let token = create_test_token(42, "user@test.com", 24);
        
        let decoded = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        ).expect("Failed to decode");

        assert_eq!(decoded.claims.user_id, 42);
        assert_eq!(decoded.claims.sub, "user@test.com");
    }

    #[test]
    fn test_expired_token_rejected() {
        let token = create_test_token(1, "test@example.com", -1);
        
        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret_rejected() {
        let token = create_test_token(1, "test@example.com", 24);
        
        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret("wrong-secret".as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_token_rejected() {
        let mut token = create_test_token(1, "test@example.com", 24);
        // Tamper with the token
        token.push('x');
        
        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_token_header_algorithm() {
        let token = create_test_token(1, "test@example.com", 24);
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        
        // Decode header
        let header = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            parts[0]
        ).expect("Failed to decode header");
        
        let header_str = String::from_utf8(header).expect("Invalid UTF-8");
        assert!(header_str.contains("HS256"));
    }
}

// ============= Rate Limiter Tests =============

#[cfg(test)]
mod rate_limiter_tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    struct RateLimiter {
        entries: HashMap<String, (u32, Instant)>,
        max_requests: u32,
        window_duration: Duration,
    }

    impl RateLimiter {
        fn new(max_requests: u32, window_secs: u64) -> Self {
            Self {
                entries: HashMap::new(),
                max_requests,
                window_duration: Duration::from_secs(window_secs),
            }
        }

        fn check(&mut self, key: &str) -> bool {
            let now = Instant::now();
            
            if let Some((count, start)) = self.entries.get_mut(key) {
                if now.duration_since(*start) >= self.window_duration {
                    *count = 1;
                    *start = now;
                    return true;
                }
                
                if *count >= self.max_requests {
                    return false;
                }
                
                *count += 1;
                true
            } else {
                self.entries.insert(key.to_string(), (1, now));
                true
            }
        }
    }

    #[test]
    fn test_allows_under_limit() {
        let mut limiter = RateLimiter::new(5, 60);
        
        for _ in 0..5 {
            assert!(limiter.check("user1"));
        }
    }

    #[test]
    fn test_blocks_over_limit() {
        let mut limiter = RateLimiter::new(3, 60);
        
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(!limiter.check("user1")); // 4th request blocked
    }

    #[test]
    fn test_separate_keys_independent() {
        let mut limiter = RateLimiter::new(2, 60);
        
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(!limiter.check("user1")); // user1 blocked
        
        assert!(limiter.check("user2")); // user2 still allowed
        assert!(limiter.check("user2"));
    }

    #[test]
    fn test_window_reset() {
        let mut limiter = RateLimiter::new(2, 0); // 0 second window for testing
        
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        // Window should have expired
        assert!(limiter.check("user1"));
    }
}

// ============= Click Buffer Tests =============

#[cfg(test)]
mod click_buffer_tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct ClickBuffer {
        events: Arc<Mutex<Vec<ClickData>>>,
        counters: Arc<Mutex<HashMap<i32, i32>>>,
        max_size: usize,
    }

    #[derive(Clone)]
    struct ClickData {
        link_id: i32,
        ip_address: Option<String>,
        country: Option<String>,
    }

    impl ClickBuffer {
        fn new(max_size: usize) -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                counters: Arc::new(Mutex::new(HashMap::new())),
                max_size,
            }
        }

        fn add_click(&self, data: ClickData) {
            let link_id = data.link_id;
            
            self.events.lock().unwrap().push(data);
            
            *self.counters
                .lock()
                .unwrap()
                .entry(link_id)
                .or_insert(0) += 1;
        }

        fn should_flush(&self) -> bool {
            self.events.lock().unwrap().len() >= self.max_size
        }

        fn flush(&self) -> (Vec<ClickData>, HashMap<i32, i32>) {
            let events = std::mem::take(&mut *self.events.lock().unwrap());
            let counters = std::mem::take(&mut *self.counters.lock().unwrap());
            (events, counters)
        }

        fn len(&self) -> usize {
            self.events.lock().unwrap().len()
        }
    }

    #[test]
    fn test_add_single_click() {
        let buffer = ClickBuffer::new(100);
        
        buffer.add_click(ClickData {
            link_id: 1,
            ip_address: Some("127.0.0.1".to_string()),
            country: Some("US".to_string()),
        });

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_add_multiple_clicks() {
        let buffer = ClickBuffer::new(100);
        
        for i in 0..10 {
            buffer.add_click(ClickData {
                link_id: i,
                ip_address: None,
                country: None,
            });
        }

        assert_eq!(buffer.len(), 10);
    }

    #[test]
    fn test_counter_aggregation() {
        let buffer = ClickBuffer::new(100);
        
        // Add 5 clicks to link 1
        for _ in 0..5 {
            buffer.add_click(ClickData {
                link_id: 1,
                ip_address: None,
                country: None,
            });
        }
        
        // Add 3 clicks to link 2
        for _ in 0..3 {
            buffer.add_click(ClickData {
                link_id: 2,
                ip_address: None,
                country: None,
            });
        }

        let (_, counters) = buffer.flush();
        
        assert_eq!(counters.get(&1), Some(&5));
        assert_eq!(counters.get(&2), Some(&3));
    }

    #[test]
    fn test_should_flush_at_capacity() {
        let buffer = ClickBuffer::new(5);
        
        for i in 0..5 {
            buffer.add_click(ClickData {
                link_id: i,
                ip_address: None,
                country: None,
            });
        }

        assert!(buffer.should_flush());
    }

    #[test]
    fn test_should_not_flush_under_capacity() {
        let buffer = ClickBuffer::new(10);
        
        for i in 0..5 {
            buffer.add_click(ClickData {
                link_id: i,
                ip_address: None,
                country: None,
            });
        }

        assert!(!buffer.should_flush());
    }

    #[test]
    fn test_flush_clears_buffer() {
        let buffer = ClickBuffer::new(100);
        
        buffer.add_click(ClickData {
            link_id: 1,
            ip_address: None,
            country: None,
        });

        let (events, _) = buffer.flush();
        
        assert_eq!(events.len(), 1);
        assert_eq!(buffer.len(), 0);
    }
}

// ============= Cache Tests =============

#[cfg(test)]
mod cache_tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    struct CachedLink {
        id: i32,
        original_url: String,
        has_password: bool,
        expires_at: Option<i64>,
        cached_at: Instant,
    }

    struct Cache {
        entries: HashMap<String, CachedLink>,
        ttl: Duration,
    }

    impl Cache {
        fn new(ttl_secs: u64) -> Self {
            Self {
                entries: HashMap::new(),
                ttl: Duration::from_secs(ttl_secs),
            }
        }

        fn get(&self, key: &str) -> Option<&CachedLink> {
            if let Some(entry) = self.entries.get(key) {
                if entry.cached_at.elapsed() < self.ttl {
                    return Some(entry);
                }
            }
            None
        }

        fn set(&mut self, key: &str, link: CachedLink) {
            self.entries.insert(key.to_string(), link);
        }

        fn invalidate(&mut self, key: &str) {
            self.entries.remove(key);
        }
    }

    #[test]
    fn test_cache_set_and_get() {
        let mut cache = Cache::new(300);
        
        cache.set("abc123", CachedLink {
            id: 1,
            original_url: "https://example.com".to_string(),
            has_password: false,
            expires_at: None,
            cached_at: Instant::now(),
        });

        let entry = cache.get("abc123");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().original_url, "https://example.com");
    }

    #[test]
    fn test_cache_miss() {
        let cache = Cache::new(300);
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_invalidate() {
        let mut cache = Cache::new(300);
        
        cache.set("abc123", CachedLink {
            id: 1,
            original_url: "https://example.com".to_string(),
            has_password: false,
            expires_at: None,
            cached_at: Instant::now(),
        });

        cache.invalidate("abc123");
        assert!(cache.get("abc123").is_none());
    }

    #[test]
    fn test_cache_ttl_expired() {
        let mut cache = Cache::new(0); // 0 TTL for testing
        
        cache.set("abc123", CachedLink {
            id: 1,
            original_url: "https://example.com".to_string(),
            has_password: false,
            expires_at: None,
            cached_at: Instant::now() - Duration::from_secs(1), // Already expired
        });

        // Entry exists but is expired
        assert!(cache.get("abc123").is_none());
    }
}

// ============= Email Token Tests =============

#[cfg(test)]
mod email_token_tests {
    use rand::{thread_rng, Rng};
    use rand::distributions::Alphanumeric;

    fn generate_token(length: usize) -> String {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(length)
            .map(char::from)
            .collect()
    }

    #[test]
    fn test_verification_token_length() {
        let token = generate_token(32);
        assert_eq!(token.len(), 32);
    }

    #[test]
    fn test_password_reset_token_length() {
        let token = generate_token(48);
        assert_eq!(token.len(), 48);
    }

    #[test]
    fn test_token_is_url_safe() {
        let token = generate_token(32);
        // Alphanumeric tokens are URL-safe
        assert!(token.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_tokens_are_unique() {
        let token1 = generate_token(32);
        let token2 = generate_token(32);
        assert_ne!(token1, token2);
    }
}

// ============= GeoIP Tests =============

#[cfg(test)]
mod geoip_tests {
    #[derive(Default)]
    struct GeoInfo {
        country: Option<String>,
        city: Option<String>,
        region: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
    }

    #[test]
    fn test_default_geo_info() {
        let geo = GeoInfo::default();
        assert!(geo.country.is_none());
        assert!(geo.city.is_none());
    }

    #[test]
    fn test_geo_info_with_data() {
        let geo = GeoInfo {
            country: Some("US".to_string()),
            city: Some("New York".to_string()),
            region: Some("NY".to_string()),
            latitude: Some(40.7128),
            longitude: Some(-74.0060),
        };
        
        assert_eq!(geo.country.as_deref(), Some("US"));
        assert!(geo.latitude.unwrap() > 40.0);
    }
}

// ============= User Agent Parsing Tests =============

#[cfg(test)]
mod user_agent_tests {
    #[derive(Default)]
    struct UAInfo {
        browser: Option<String>,
        os: Option<String>,
        device: Option<String>,
    }

    fn parse_simple_ua(ua: &str) -> UAInfo {
        let ua_lower = ua.to_lowercase();
        
        let browser = if ua_lower.contains("chrome") {
            Some("Chrome".to_string())
        } else if ua_lower.contains("firefox") {
            Some("Firefox".to_string())
        } else if ua_lower.contains("safari") {
            Some("Safari".to_string())
        } else {
            None
        };

        let os = if ua_lower.contains("windows") {
            Some("Windows".to_string())
        } else if ua_lower.contains("mac os") || ua_lower.contains("macos") {
            Some("macOS".to_string())
        } else if ua_lower.contains("linux") {
            Some("Linux".to_string())
        } else if ua_lower.contains("android") {
            Some("Android".to_string())
        } else if ua_lower.contains("iphone") || ua_lower.contains("ipad") {
            Some("iOS".to_string())
        } else {
            None
        };

        let device = if ua_lower.contains("mobile") || ua_lower.contains("iphone") || ua_lower.contains("android") {
            Some("Mobile".to_string())
        } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
            Some("Tablet".to_string())
        } else {
            Some("Desktop".to_string())
        };

        UAInfo { browser, os, device }
    }

    #[test]
    fn test_chrome_windows() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/91.0";
        let info = parse_simple_ua(ua);
        
        assert_eq!(info.browser.as_deref(), Some("Chrome"));
        assert_eq!(info.os.as_deref(), Some("Windows"));
        assert_eq!(info.device.as_deref(), Some("Desktop"));
    }

    #[test]
    fn test_safari_macos() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X) Safari/14.0";
        let info = parse_simple_ua(ua);
        
        assert_eq!(info.browser.as_deref(), Some("Safari"));
        assert_eq!(info.os.as_deref(), Some("macOS"));
    }

    #[test]
    fn test_mobile_android() {
        let ua = "Mozilla/5.0 (Linux; Android 11; Mobile) Chrome/91.0";
        let info = parse_simple_ua(ua);
        
        assert_eq!(info.os.as_deref(), Some("Android"));
        assert_eq!(info.device.as_deref(), Some("Mobile"));
    }

    #[test]
    fn test_iphone_safari() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0) Safari/604.1";
        let info = parse_simple_ua(ua);
        
        assert_eq!(info.os.as_deref(), Some("iOS"));
        assert_eq!(info.device.as_deref(), Some("Mobile"));
    }
}
