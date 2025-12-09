//! Rate limiter tests

use std::time::Duration;

// Import the rate limiter module
#[path = "../src/utils/rate_limiter.rs"]
mod rate_limiter;

use rate_limiter::{RateLimiter, RateLimitConfig, RateLimitResult};

#[test]
fn test_rate_limiter_allows_within_limit() {
    let limiter = RateLimiter::new(RateLimitConfig::new(5, 60));
    
    // Should allow 5 requests
    for i in 0..5 {
        let result = limiter.check("test_user");
        match result {
            RateLimitResult::Allowed { remaining, .. } => {
                assert_eq!(remaining, 4 - i);
            }
            RateLimitResult::Limited { .. } => {
                panic!("Request {} should have been allowed", i + 1);
            }
        }
    }
}

#[test]
fn test_rate_limiter_blocks_over_limit() {
    let limiter = RateLimiter::new(RateLimitConfig::new(3, 60));
    
    // Use up all requests
    for _ in 0..3 {
        limiter.check("test_user");
    }
    
    // 4th request should be blocked
    let result = limiter.check("test_user");
    assert!(matches!(result, RateLimitResult::Limited { .. }));
}

#[test]
fn test_rate_limiter_separate_keys() {
    let limiter = RateLimiter::new(RateLimitConfig::new(2, 60));
    
    // User 1 makes 2 requests
    limiter.check("user1");
    limiter.check("user1");
    
    // User 1 should be limited
    assert!(matches!(limiter.check("user1"), RateLimitResult::Limited { .. }));
    
    // User 2 should still have requests available
    assert!(matches!(limiter.check("user2"), RateLimitResult::Allowed { .. }));
}

#[test]
fn test_rate_limiter_returns_correct_limit_info() {
    let limiter = RateLimiter::new(RateLimitConfig::new(10, 60));
    
    let result = limiter.check("test");
    match result {
        RateLimitResult::Allowed { limit, remaining } => {
            assert_eq!(limit, 10);
            assert_eq!(remaining, 9);
        }
        _ => panic!("Expected Allowed result"),
    }
}

#[test]
fn test_rate_limiter_limited_returns_retry_after() {
    let limiter = RateLimiter::new(RateLimitConfig::new(1, 60));
    
    // Use up the limit
    limiter.check("test");
    
    // Check that we get retry_after
    let result = limiter.check("test");
    match result {
        RateLimitResult::Limited { retry_after_secs, .. } => {
            assert!(retry_after_secs > 0 && retry_after_secs <= 60);
        }
        _ => panic!("Expected Limited result"),
    }
}

#[test]
fn test_rate_limiter_cleanup() {
    let limiter = RateLimiter::new(RateLimitConfig::new(5, 60));
    
    // Add some entries
    limiter.check("user1");
    limiter.check("user2");
    limiter.check("user3");
    
    // Cleanup should run without errors
    limiter.cleanup();
}




