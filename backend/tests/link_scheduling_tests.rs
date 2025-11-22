//! Link scheduling and automation tests

use chrono::{Duration, NaiveDateTime, Utc};

/// Test helper to create a mock link model
fn create_test_link(
    starts_at: Option<NaiveDateTime>,
    expires_at: Option<NaiveDateTime>,
    max_clicks: Option<i32>,
    click_count: i32,
) -> MockLink {
    MockLink {
        starts_at,
        expires_at,
        max_clicks,
        click_count,
    }
}

struct MockLink {
    starts_at: Option<NaiveDateTime>,
    expires_at: Option<NaiveDateTime>,
    max_clicks: Option<i32>,
    click_count: i32,
}

impl MockLink {
    fn is_active(&self) -> bool {
        let now = Utc::now().naive_utc();
        
        // Check if link hasn't started yet
        if let Some(starts_at) = self.starts_at {
            if now < starts_at {
                return false;
            }
        }
        
        // Check if link has expired
        if let Some(expires_at) = self.expires_at {
            if now > expires_at {
                return false;
            }
        }
        
        // Check if max clicks reached
        if let Some(max_clicks) = self.max_clicks {
            if self.click_count >= max_clicks {
                return false;
            }
        }
        
        true
    }

    fn inactive_reason(&self) -> Option<&'static str> {
        let now = Utc::now().naive_utc();
        
        if let Some(starts_at) = self.starts_at {
            if now < starts_at {
                return Some("Link is scheduled to activate later");
            }
        }
        
        if let Some(expires_at) = self.expires_at {
            if now > expires_at {
                return Some("Link has expired");
            }
        }
        
        if let Some(max_clicks) = self.max_clicks {
            if self.click_count >= max_clicks {
                return Some("Link has reached maximum clicks");
            }
        }
        
        None
    }
}

#[test]
fn test_active_link_without_constraints() {
    let link = create_test_link(None, None, None, 0);
    assert!(link.is_active());
    assert!(link.inactive_reason().is_none());
}

#[test]
fn test_link_not_started_yet() {
    let future = Utc::now().naive_utc() + Duration::hours(1);
    let link = create_test_link(Some(future), None, None, 0);
    
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link is scheduled to activate later"));
}

#[test]
fn test_link_started_in_past() {
    let past = Utc::now().naive_utc() - Duration::hours(1);
    let link = create_test_link(Some(past), None, None, 0);
    
    assert!(link.is_active());
    assert!(link.inactive_reason().is_none());
}

#[test]
fn test_link_expired() {
    let past = Utc::now().naive_utc() - Duration::hours(1);
    let link = create_test_link(None, Some(past), None, 0);
    
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link has expired"));
}

#[test]
fn test_link_not_expired_yet() {
    let future = Utc::now().naive_utc() + Duration::hours(1);
    let link = create_test_link(None, Some(future), None, 0);
    
    assert!(link.is_active());
    assert!(link.inactive_reason().is_none());
}

#[test]
fn test_max_clicks_reached() {
    let link = create_test_link(None, None, Some(10), 10);
    
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link has reached maximum clicks"));
}

#[test]
fn test_max_clicks_not_reached() {
    let link = create_test_link(None, None, Some(10), 5);
    
    assert!(link.is_active());
    assert!(link.inactive_reason().is_none());
}

#[test]
fn test_max_clicks_exceeded() {
    let link = create_test_link(None, None, Some(10), 15);
    
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link has reached maximum clicks"));
}

#[test]
fn test_combined_constraints_active() {
    let past = Utc::now().naive_utc() - Duration::hours(1);
    let future = Utc::now().naive_utc() + Duration::hours(1);
    let link = create_test_link(Some(past), Some(future), Some(100), 50);
    
    assert!(link.is_active());
    assert!(link.inactive_reason().is_none());
}

#[test]
fn test_combined_constraints_not_started() {
    let future1 = Utc::now().naive_utc() + Duration::hours(1);
    let future2 = Utc::now().naive_utc() + Duration::hours(2);
    let link = create_test_link(Some(future1), Some(future2), Some(100), 50);
    
    assert!(!link.is_active());
    // Should report the first failing condition
    assert_eq!(link.inactive_reason(), Some("Link is scheduled to activate later"));
}

#[test]
fn test_combined_constraints_expired() {
    let past1 = Utc::now().naive_utc() - Duration::hours(2);
    let past2 = Utc::now().naive_utc() - Duration::hours(1);
    let link = create_test_link(Some(past1), Some(past2), Some(100), 50);
    
    assert!(!link.is_active());
    assert_eq!(link.inactive_reason(), Some("Link has expired"));
}

