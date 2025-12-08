//! Comprehensive rate limiter integration tests

use std::time::Duration;
use std::thread;

#[path = "../src/utils/rate_limiter.rs"]
mod rate_limiter;

use rate_limiter::{RateLimiter, RateLimitConfig, RateLimitResult, RateLimiters};

// ============= Basic Rate Limiter Tests =============

mod basic_tests {
    use super::*;

    #[test]
    fn test_allows_first_request() {
        let limiter = RateLimiter::new(RateLimitConfig::new(10, 60));
        let result = limiter.check("test_key");
        
        match result {
            RateLimitResult::Allowed { limit, remaining } => {
                assert_eq!(limit, 10);
                assert_eq!(remaining, 9);
            }
            _ => panic!("First request should be allowed"),
        }
    }

    #[test]
    fn test_allows_up_to_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(5, 60));
        
        for i in 0..5 {
            let result = limiter.check("test_key");
            match result {
                RateLimitResult::Allowed { remaining, .. } => {
                    assert_eq!(remaining, 4 - i);
                }
                _ => panic!("Request {} should be allowed", i + 1),
            }
        }
    }

    #[test]
    fn test_blocks_after_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(3, 60));
        
        // Use up the limit
        for _ in 0..3 {
            limiter.check("test_key");
        }
        
        // Should be blocked
        let result = limiter.check("test_key");
        match result {
            RateLimitResult::Limited { limit, remaining, .. } => {
                assert_eq!(limit, 3);
                assert_eq!(remaining, 0);
            }
            _ => panic!("Should be rate limited"),
        }
    }

    #[test]
    fn test_different_keys_independent() {
        let limiter = RateLimiter::new(RateLimitConfig::new(2, 60));
        
        // User 1 makes 2 requests
        limiter.check("user1");
        limiter.check("user1");
        
        // User 1 should be limited
        assert!(matches!(limiter.check("user1"), RateLimitResult::Limited { .. }));
        
        // User 2 should still be allowed
        assert!(matches!(limiter.check("user2"), RateLimitResult::Allowed { .. }));
        assert!(matches!(limiter.check("user2"), RateLimitResult::Allowed { .. }));
        
        // User 2 should now be limited
        assert!(matches!(limiter.check("user2"), RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_returns_correct_retry_after() {
        let limiter = RateLimiter::new(RateLimitConfig::new(1, 60));
        
        limiter.check("test_key");
        
        let result = limiter.check("test_key");
        match result {
            RateLimitResult::Limited { retry_after_secs, .. } => {
                assert!(retry_after_secs > 0);
                assert!(retry_after_secs <= 60);
            }
            _ => panic!("Should be rate limited"),
        }
    }
}

// ============= Edge Cases =============

mod edge_cases {
    use super::*;

    #[test]
    fn test_zero_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(0, 60));
        
        // Should immediately be limited
        let result = limiter.check("test_key");
        assert!(matches!(result, RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_single_request_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(1, 60));
        
        // First should be allowed
        assert!(matches!(limiter.check("test_key"), RateLimitResult::Allowed { remaining: 0, .. }));
        
        // Second should be blocked
        assert!(matches!(limiter.check("test_key"), RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_large_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(1000000, 60));
        
        // Should handle large limits
        for _ in 0..100 {
            let result = limiter.check("test_key");
            assert!(matches!(result, RateLimitResult::Allowed { .. }));
        }
    }

    #[test]
    fn test_empty_key() {
        let limiter = RateLimiter::new(RateLimitConfig::new(5, 60));
        
        let result = limiter.check("");
        assert!(matches!(result, RateLimitResult::Allowed { .. }));
    }

    #[test]
    fn test_special_characters_in_key() {
        let limiter = RateLimiter::new(RateLimitConfig::new(5, 60));
        
        let special_keys = vec![
            "user@example.com",
            "192.168.1.1",
            "key:with:colons",
            "key/with/slashes",
            "key with spaces",
            "키한글",
            "🚀emoji",
        ];
        
        for key in special_keys {
            let result = limiter.check(key);
            assert!(matches!(result, RateLimitResult::Allowed { .. }), "Key '{}' should be allowed", key);
        }
    }

    #[test]
    fn test_very_short_window() {
        let limiter = RateLimiter::new(RateLimitConfig::new(5, 1)); // 1 second window
        
        // Use up limit
        for _ in 0..5 {
            limiter.check("test_key");
        }
        
        assert!(matches!(limiter.check("test_key"), RateLimitResult::Limited { .. }));
        
        // Wait for window to expire
        thread::sleep(Duration::from_millis(1100));
        
        // Should be allowed again
        assert!(matches!(limiter.check("test_key"), RateLimitResult::Allowed { .. }));
    }
}

// ============= Concurrent Access Tests =============

mod concurrency_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_access_same_key() {
        let limiter = Arc::new(RateLimiter::new(RateLimitConfig::new(100, 60)));
        let mut handles = vec![];
        
        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(thread::spawn(move || {
                for _ in 0..10 {
                    limiter_clone.check("shared_key");
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // After 100 requests, should be at limit
        let result = limiter.check("shared_key");
        assert!(matches!(result, RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_concurrent_access_different_keys() {
        let limiter = Arc::new(RateLimiter::new(RateLimitConfig::new(5, 60)));
        let mut handles = vec![];
        
        for i in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(thread::spawn(move || {
                let key = format!("user_{}", i);
                for _ in 0..5 {
                    limiter_clone.check(&key);
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Each user should be at their limit independently
        for i in 0..10 {
            let key = format!("user_{}", i);
            let result = limiter.check(&key);
            assert!(matches!(result, RateLimitResult::Limited { .. }), "User {} should be limited", i);
        }
    }
}

// ============= Cleanup Tests =============

mod cleanup_tests {
    use super::*;

    #[test]
    fn test_cleanup_removes_old_entries() {
        let limiter = RateLimiter::new(RateLimitConfig::new(5, 1)); // 1 second window
        
        // Add some entries
        limiter.check("key1");
        limiter.check("key2");
        limiter.check("key3");
        
        // Wait for entries to expire
        thread::sleep(Duration::from_millis(2100));
        
        // Run cleanup
        limiter.cleanup();
        
        // Entries should be gone, so new requests should have full limit
        let result = limiter.check("key1");
        match result {
            RateLimitResult::Allowed { remaining, .. } => {
                assert_eq!(remaining, 4);
            }
            _ => panic!("Should be allowed with full limit"),
        }
    }

    #[test]
    fn test_cleanup_preserves_active_entries() {
        let limiter = RateLimiter::new(RateLimitConfig::new(5, 60)); // 60 second window
        
        // Add an entry
        limiter.check("active_key");
        
        // Run cleanup
        limiter.cleanup();
        
        // Entry should still be there
        let result = limiter.check("active_key");
        match result {
            RateLimitResult::Allowed { remaining, .. } => {
                assert_eq!(remaining, 3); // 5 - 2 = 3
            }
            _ => panic!("Should be allowed"),
        }
    }
}

// ============= RateLimiters Tests =============

mod limiters_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_default_limiters() {
        let limiters = RateLimiters::default();
        
        // General: 100 per minute
        for _ in 0..100 {
            assert!(matches!(limiters.general.check("test"), RateLimitResult::Allowed { .. }));
        }
        assert!(matches!(limiters.general.check("test"), RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_auth_limiter() {
        let limiters = RateLimiters::default();
        
        // Auth: 10 per minute
        for _ in 0..10 {
            assert!(matches!(limiters.auth.check("test"), RateLimitResult::Allowed { .. }));
        }
        assert!(matches!(limiters.auth.check("test"), RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_link_creation_limiter() {
        let limiters = RateLimiters::default();
        
        // Link creation: 50 per hour
        for _ in 0..50 {
            assert!(matches!(limiters.link_creation.check("test"), RateLimitResult::Allowed { .. }));
        }
        assert!(matches!(limiters.link_creation.check("test"), RateLimitResult::Limited { .. }));
    }

    #[test]
    fn test_redirect_limiter() {
        let limiters = RateLimiters::default();
        
        // Redirect: 1000 per minute
        for _ in 0..1000 {
            assert!(matches!(limiters.redirect.check("test"), RateLimitResult::Allowed { .. }));
        }
        assert!(matches!(limiters.redirect.check("test"), RateLimitResult::Limited { .. }));
    }
}

// ============= IP Extraction Tests =============

mod ip_extraction_tests {
    // These would test the extract_ip function with mock requests
    // Since we can't easily create axum Request objects in tests,
    // we'll test the logic separately
    
    #[test]
    fn test_ip_parsing() {
        // Test that various IP formats are valid
        let ips = vec![
            "192.168.1.1",
            "10.0.0.1",
            "172.16.0.1",
            "8.8.8.8",
            "2001:db8::1",
            "::1",
        ];
        
        for ip in ips {
            assert!(ip.parse::<std::net::IpAddr>().is_ok(), "Failed to parse: {}", ip);
        }
    }

    #[test]
    fn test_xff_parsing() {
        // Test X-Forwarded-For header parsing
        let xff = "203.0.113.195, 70.41.3.18, 150.172.238.178";
        let first_ip = xff.split(',').next().unwrap().trim();
        assert_eq!(first_ip, "203.0.113.195");
    }
}



