//! Comprehensive link features tests

use chrono::{Duration, NaiveDateTime, Utc};

// ============= Link URL Validation Tests =============

mod url_validation_tests {
    fn is_valid_url(url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        
        // Must start with http:// or https://
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return false;
        }
        
        // Must have a host
        let without_protocol = url.trim_start_matches("http://").trim_start_matches("https://");
        if without_protocol.is_empty() || without_protocol.starts_with('/') {
            return false;
        }
        
        // Basic URL parsing check
        url::Url::parse(url).is_ok()
    }

    #[test]
    fn test_valid_urls() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://example.com"));
        assert!(is_valid_url("https://www.example.com/path/to/page"));
        assert!(is_valid_url("https://example.com:8080/path"));
        assert!(is_valid_url("https://example.com/path?query=value"));
        assert!(is_valid_url("https://example.com/path#anchor"));
        assert!(is_valid_url("https://sub.domain.example.com"));
        assert!(is_valid_url("https://example.com/path/with%20space"));
    }

    #[test]
    fn test_invalid_urls() {
        assert!(!is_valid_url(""));
        assert!(!is_valid_url("example.com"));
        assert!(!is_valid_url("ftp://example.com"));
        assert!(!is_valid_url("//example.com"));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("http://"));
        assert!(!is_valid_url("not a url at all"));
    }
}

// ============= Short Code Generation Tests =============

mod short_code_tests {
    use rand::{thread_rng, Rng};
    use rand::distributions::Alphanumeric;

    fn generate_short_code(length: usize) -> String {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(length)
            .map(char::from)
            .collect()
    }

    fn is_valid_code(code: &str) -> bool {
        !code.is_empty() 
            && code.len() >= 3 
            && code.len() <= 20
            && code.chars().all(|c| c.is_ascii_alphanumeric())
    }

    #[test]
    fn test_generate_code_length() {
        let code = generate_short_code(6);
        assert_eq!(code.len(), 6);
        
        let code = generate_short_code(8);
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn test_generate_code_alphanumeric() {
        for _ in 0..100 {
            let code = generate_short_code(6);
            assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
        }
    }

    #[test]
    fn test_generate_unique_codes() {
        let mut codes = std::collections::HashSet::new();
        for _ in 0..1000 {
            let code = generate_short_code(6);
            codes.insert(code);
        }
        // Should generate mostly unique codes
        assert!(codes.len() > 990);
    }

    #[test]
    fn test_valid_custom_codes() {
        assert!(is_valid_code("abc"));
        assert!(is_valid_code("MyLink123"));
        assert!(is_valid_code("ABC123xyz"));
    }

    #[test]
    fn test_invalid_custom_codes() {
        assert!(!is_valid_code(""));
        assert!(!is_valid_code("ab")); // Too short
        assert!(!is_valid_code(&"x".repeat(21))); // Too long
        assert!(!is_valid_code("my-link")); // Contains hyphen
        assert!(!is_valid_code("my link")); // Contains space
        assert!(!is_valid_code("link@123")); // Contains @
    }
}

// ============= Password Protection Tests =============

mod password_tests {
    fn hash_password(password: &str) -> String {
        // Simplified for testing - in real code uses bcrypt
        format!("hashed:{}", password)
    }

    fn verify_password(password: &str, hash: &str) -> bool {
        // Simplified for testing
        hash == format!("hashed:{}", password)
    }

    fn is_strong_password(password: &str) -> (bool, Vec<&'static str>) {
        let mut errors = Vec::new();
        
        if password.len() < 8 {
            errors.push("Password must be at least 8 characters");
        }
        if !password.chars().any(|c| c.is_uppercase()) {
            errors.push("Password must contain an uppercase letter");
        }
        if !password.chars().any(|c| c.is_lowercase()) {
            errors.push("Password must contain a lowercase letter");
        }
        if !password.chars().any(|c| c.is_numeric()) {
            errors.push("Password must contain a number");
        }
        
        (errors.is_empty(), errors)
    }

    #[test]
    fn test_hash_and_verify() {
        let password = "MySecurePassword123";
        let hash = hash_password(password);
        
        assert!(verify_password(password, &hash));
        assert!(!verify_password("WrongPassword", &hash));
    }

    #[test]
    fn test_strong_password() {
        let (is_strong, errors) = is_strong_password("MyPass123");
        assert!(is_strong);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_weak_password_too_short() {
        let (is_strong, errors) = is_strong_password("Aa1");
        assert!(!is_strong);
        assert!(errors.contains(&"Password must be at least 8 characters"));
    }

    #[test]
    fn test_weak_password_no_uppercase() {
        let (is_strong, errors) = is_strong_password("mypassword123");
        assert!(!is_strong);
        assert!(errors.contains(&"Password must contain an uppercase letter"));
    }

    #[test]
    fn test_weak_password_no_number() {
        let (is_strong, errors) = is_strong_password("MyPassword");
        assert!(!is_strong);
        assert!(errors.contains(&"Password must contain a number"));
    }
}

// ============= Expiration Tests =============

mod expiration_tests {
    use super::*;

    fn is_expired(expires_at: Option<NaiveDateTime>) -> bool {
        match expires_at {
            None => false,
            Some(exp) => Utc::now().naive_utc() > exp,
        }
    }

    fn time_until_expiry(expires_at: Option<NaiveDateTime>) -> Option<Duration> {
        expires_at.map(|exp| {
            let now = Utc::now().naive_utc();
            if exp > now {
                exp - now
            } else {
                Duration::zero()
            }
        })
    }

    #[test]
    fn test_not_expired_no_expiry() {
        assert!(!is_expired(None));
    }

    #[test]
    fn test_not_expired_future() {
        let future = Utc::now().naive_utc() + Duration::hours(1);
        assert!(!is_expired(Some(future)));
    }

    #[test]
    fn test_expired_past() {
        let past = Utc::now().naive_utc() - Duration::hours(1);
        assert!(is_expired(Some(past)));
    }

    #[test]
    fn test_time_until_expiry() {
        let future = Utc::now().naive_utc() + Duration::hours(2);
        let time_left = time_until_expiry(Some(future)).unwrap();
        
        // Should be approximately 2 hours (with some tolerance)
        assert!(time_left.num_minutes() >= 119);
        assert!(time_left.num_minutes() <= 120);
    }

    #[test]
    fn test_time_until_expiry_already_expired() {
        let past = Utc::now().naive_utc() - Duration::hours(1);
        let time_left = time_until_expiry(Some(past)).unwrap();
        
        assert_eq!(time_left.num_seconds(), 0);
    }
}

// ============= Click Limit Tests =============

mod click_limit_tests {
    fn is_click_limited(click_count: i32, max_clicks: Option<i32>) -> bool {
        match max_clicks {
            None => false,
            Some(max) => click_count >= max,
        }
    }

    fn remaining_clicks(click_count: i32, max_clicks: Option<i32>) -> Option<i32> {
        max_clicks.map(|max| (max - click_count).max(0))
    }

    #[test]
    fn test_no_limit() {
        assert!(!is_click_limited(1000, None));
        assert_eq!(remaining_clicks(1000, None), None);
    }

    #[test]
    fn test_under_limit() {
        assert!(!is_click_limited(5, Some(10)));
        assert_eq!(remaining_clicks(5, Some(10)), Some(5));
    }

    #[test]
    fn test_at_limit() {
        assert!(is_click_limited(10, Some(10)));
        assert_eq!(remaining_clicks(10, Some(10)), Some(0));
    }

    #[test]
    fn test_over_limit() {
        assert!(is_click_limited(15, Some(10)));
        assert_eq!(remaining_clicks(15, Some(10)), Some(0));
    }

    #[test]
    fn test_single_click_limit() {
        assert!(!is_click_limited(0, Some(1)));
        assert!(is_click_limited(1, Some(1)));
    }
}

// ============= Scheduled Activation Tests =============

mod scheduled_tests {
    use super::*;

    fn is_scheduled_active(starts_at: Option<NaiveDateTime>) -> bool {
        match starts_at {
            None => true, // No schedule = always active
            Some(start) => Utc::now().naive_utc() >= start,
        }
    }

    fn time_until_active(starts_at: Option<NaiveDateTime>) -> Option<Duration> {
        starts_at.map(|start| {
            let now = Utc::now().naive_utc();
            if start > now {
                start - now
            } else {
                Duration::zero()
            }
        })
    }

    #[test]
    fn test_no_schedule() {
        assert!(is_scheduled_active(None));
    }

    #[test]
    fn test_scheduled_past() {
        let past = Utc::now().naive_utc() - Duration::hours(1);
        assert!(is_scheduled_active(Some(past)));
    }

    #[test]
    fn test_scheduled_future() {
        let future = Utc::now().naive_utc() + Duration::hours(1);
        assert!(!is_scheduled_active(Some(future)));
    }

    #[test]
    fn test_time_until_active() {
        let future = Utc::now().naive_utc() + Duration::hours(2);
        let time_left = time_until_active(Some(future)).unwrap();
        
        assert!(time_left.num_minutes() >= 119);
        assert!(time_left.num_minutes() <= 120);
    }

    #[test]
    fn test_time_until_active_already_active() {
        let past = Utc::now().naive_utc() - Duration::hours(1);
        let time_left = time_until_active(Some(past)).unwrap();
        
        assert_eq!(time_left.num_seconds(), 0);
    }
}

// ============= Link Status Tests =============

mod link_status_tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum LinkStatus {
        Active,
        Scheduled,
        Expired,
        ClickLimitReached,
        Inactive,
    }

    fn get_link_status(
        starts_at: Option<NaiveDateTime>,
        expires_at: Option<NaiveDateTime>,
        max_clicks: Option<i32>,
        click_count: i32,
    ) -> LinkStatus {
        let now = Utc::now().naive_utc();
        
        // Check scheduled activation
        if let Some(start) = starts_at {
            if now < start {
                return LinkStatus::Scheduled;
            }
        }
        
        // Check expiration
        if let Some(exp) = expires_at {
            if now > exp {
                return LinkStatus::Expired;
            }
        }
        
        // Check click limit
        if let Some(max) = max_clicks {
            if click_count >= max {
                return LinkStatus::ClickLimitReached;
            }
        }
        
        LinkStatus::Active
    }

    #[test]
    fn test_active_status() {
        let status = get_link_status(None, None, None, 0);
        assert_eq!(status, LinkStatus::Active);
    }

    #[test]
    fn test_scheduled_status() {
        let future = Utc::now().naive_utc() + Duration::hours(1);
        let status = get_link_status(Some(future), None, None, 0);
        assert_eq!(status, LinkStatus::Scheduled);
    }

    #[test]
    fn test_expired_status() {
        let past = Utc::now().naive_utc() - Duration::hours(1);
        let status = get_link_status(None, Some(past), None, 0);
        assert_eq!(status, LinkStatus::Expired);
    }

    #[test]
    fn test_click_limit_status() {
        let status = get_link_status(None, None, Some(10), 10);
        assert_eq!(status, LinkStatus::ClickLimitReached);
    }

    #[test]
    fn test_scheduled_takes_priority() {
        let future = Utc::now().naive_utc() + Duration::hours(1);
        let past = Utc::now().naive_utc() - Duration::hours(1);
        
        // Even with past expiration, scheduled should take priority
        let status = get_link_status(Some(future), Some(past), Some(10), 10);
        assert_eq!(status, LinkStatus::Scheduled);
    }
}

// ============= Bulk Operations Tests =============

mod bulk_operations_tests {
    fn validate_bulk_urls(urls: &[&str]) -> (Vec<String>, Vec<String>) {
        let mut valid = Vec::new();
        let mut invalid = Vec::new();
        
        for url in urls {
            if url.starts_with("http://") || url.starts_with("https://") {
                valid.push(url.to_string());
            } else {
                invalid.push(url.to_string());
            }
        }
        
        (valid, invalid)
    }

    #[test]
    fn test_all_valid() {
        let urls = vec![
            "https://example.com",
            "http://test.com",
            "https://another.org",
        ];
        
        let (valid, invalid) = validate_bulk_urls(&urls);
        
        assert_eq!(valid.len(), 3);
        assert!(invalid.is_empty());
    }

    #[test]
    fn test_all_invalid() {
        let urls = vec![
            "example.com",
            "ftp://test.com",
            "not a url",
        ];
        
        let (valid, invalid) = validate_bulk_urls(&urls);
        
        assert!(valid.is_empty());
        assert_eq!(invalid.len(), 3);
    }

    #[test]
    fn test_mixed() {
        let urls = vec![
            "https://valid.com",
            "invalid",
            "http://also-valid.com",
            "nope",
        ];
        
        let (valid, invalid) = validate_bulk_urls(&urls);
        
        assert_eq!(valid.len(), 2);
        assert_eq!(invalid.len(), 2);
    }

    #[test]
    fn test_empty() {
        let urls: Vec<&str> = vec![];
        let (valid, invalid) = validate_bulk_urls(&urls);
        
        assert!(valid.is_empty());
        assert!(invalid.is_empty());
    }
}

// ============= CSV Export Tests =============

mod csv_export_tests {
    fn escape_csv_field(field: &str) -> String {
        if field.contains(',') || field.contains('"') || field.contains('\n') {
            format!("\"{}\"", field.replace('"', "\"\""))
        } else {
            field.to_string()
        }
    }

    fn generate_csv_row(fields: &[&str]) -> String {
        fields
            .iter()
            .map(|f| escape_csv_field(f))
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn test_simple_field() {
        assert_eq!(escape_csv_field("hello"), "hello");
    }

    #[test]
    fn test_field_with_comma() {
        assert_eq!(escape_csv_field("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn test_field_with_quote() {
        assert_eq!(escape_csv_field("hello\"world"), "\"hello\"\"world\"");
    }

    #[test]
    fn test_field_with_newline() {
        assert_eq!(escape_csv_field("hello\nworld"), "\"hello\nworld\"");
    }

    #[test]
    fn test_generate_row() {
        let row = generate_csv_row(&["1", "abc123", "https://example.com"]);
        assert_eq!(row, "1,abc123,https://example.com");
    }

    #[test]
    fn test_generate_row_with_special_chars() {
        let row = generate_csv_row(&["1", "code", "https://example.com/path?a=1,2"]);
        assert!(row.contains("\"https://example.com/path?a=1,2\""));
    }
}




