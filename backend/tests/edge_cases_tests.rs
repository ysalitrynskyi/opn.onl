//! Edge case tests for comprehensive coverage

use std::collections::HashMap;

// ============= URL Validation Edge Cases =============

#[cfg(test)]
mod url_validation {
    use super::*;

    fn is_valid_url(url: &str) -> bool {
        url::Url::parse(url).is_ok()
    }

    #[test]
    fn test_valid_http_url() {
        assert!(is_valid_url("http://example.com"));
    }

    #[test]
    fn test_valid_https_url() {
        assert!(is_valid_url("https://example.com"));
    }

    #[test]
    fn test_url_with_port() {
        assert!(is_valid_url("https://example.com:8080"));
    }

    #[test]
    fn test_url_with_path() {
        assert!(is_valid_url("https://example.com/path/to/resource"));
    }

    #[test]
    fn test_url_with_query() {
        assert!(is_valid_url("https://example.com?foo=bar&baz=qux"));
    }

    #[test]
    fn test_url_with_fragment() {
        assert!(is_valid_url("https://example.com#section"));
    }

    #[test]
    fn test_url_with_auth() {
        assert!(is_valid_url("https://user:pass@example.com"));
    }

    #[test]
    fn test_url_with_unicode() {
        assert!(is_valid_url("https://example.com/путь"));
    }

    #[test]
    fn test_url_with_encoded_chars() {
        assert!(is_valid_url("https://example.com/path%20with%20spaces"));
    }

    #[test]
    fn test_very_long_url() {
        let long_path = "a".repeat(2000);
        let url = format!("https://example.com/{}", long_path);
        assert!(is_valid_url(&url));
    }

    #[test]
    fn test_invalid_url_no_scheme() {
        assert!(!is_valid_url("example.com"));
    }

    #[test]
    fn test_invalid_url_empty() {
        assert!(!is_valid_url(""));
    }

    #[test]
    fn test_invalid_url_spaces() {
        assert!(!is_valid_url("https://example .com"));
    }

    #[test]
    fn test_localhost_url() {
        assert!(is_valid_url("http://localhost:3000"));
    }

    #[test]
    fn test_ip_address_url() {
        assert!(is_valid_url("http://192.168.1.1:8080"));
    }

    #[test]
    fn test_ipv6_url() {
        assert!(is_valid_url("http://[::1]:8080"));
    }
}

// ============= Short Code Generation Edge Cases =============

#[cfg(test)]
mod short_code_generation {
    use rand::Rng;

    const CODE_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const CODE_LENGTH: usize = 6;

    fn generate_code() -> String {
        let mut rng = rand::thread_rng();
        (0..CODE_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CODE_CHARS.len());
                CODE_CHARS[idx] as char
            })
            .collect()
    }

    #[test]
    fn test_code_length() {
        let code = generate_code();
        assert_eq!(code.len(), CODE_LENGTH);
    }

    #[test]
    fn test_code_characters() {
        let code = generate_code();
        for c in code.chars() {
            assert!(c.is_alphanumeric());
        }
    }

    #[test]
    fn test_code_uniqueness() {
        let codes: Vec<String> = (0..1000).map(|_| generate_code()).collect();
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        // All codes should be unique (with high probability)
        assert_eq!(codes.len(), unique.len());
    }

    #[test]
    fn test_code_no_special_chars() {
        let code = generate_code();
        assert!(!code.contains('/'));
        assert!(!code.contains('?'));
        assert!(!code.contains('#'));
        assert!(!code.contains('&'));
    }
}

// ============= Password Hashing Edge Cases =============

#[cfg(test)]
mod password_hashing {
    use bcrypt::{hash, verify, DEFAULT_COST};

    #[test]
    fn test_hash_short_password() {
        let password = "a";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(verify(password, &hashed).unwrap());
    }

    #[test]
    fn test_hash_long_password() {
        let password = "a".repeat(72); // bcrypt max length
        let hashed = hash(&password, DEFAULT_COST).unwrap();
        assert!(verify(&password, &hashed).unwrap());
    }

    #[test]
    fn test_hash_special_chars() {
        let password = "p@$$w0rd!@#$%^&*()";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(verify(password, &hashed).unwrap());
    }

    #[test]
    fn test_hash_unicode() {
        let password = "密码пароль";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(verify(password, &hashed).unwrap());
    }

    #[test]
    fn test_wrong_password() {
        let password = "correct";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(!verify("wrong", &hashed).unwrap());
    }

    #[test]
    fn test_case_sensitive() {
        let password = "Password";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(!verify("password", &hashed).unwrap());
    }

    #[test]
    fn test_whitespace_significant() {
        let password = "pass word";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(!verify("password", &hashed).unwrap());
    }
}

// ============= JWT Edge Cases =============

#[cfg(test)]
mod jwt_edge_cases {
    use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation, Algorithm};
    use serde::{Serialize, Deserialize};
    use chrono::{Utc, Duration};

    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        user_id: i32,
        exp: usize,
        iat: usize,
    }

    const SECRET: &[u8] = b"test-secret-key-for-testing";

    fn create_token(user_id: i32, expires_in: Duration) -> String {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            user_id,
            exp: (now + expires_in).timestamp() as usize,
            iat: now.timestamp() as usize,
        };
        encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET)).unwrap()
    }

    fn validate_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let validation = Validation::new(Algorithm::HS256);
        decode::<Claims>(token, &DecodingKey::from_secret(SECRET), &validation)
            .map(|data| data.claims)
    }

    #[test]
    fn test_valid_token() {
        let token = create_token(1, Duration::hours(1));
        assert!(validate_token(&token).is_ok());
    }

    #[test]
    fn test_expired_token() {
        let token = create_token(1, Duration::hours(-1)); // Expired 1 hour ago
        assert!(validate_token(&token).is_err());
    }

    #[test]
    fn test_token_contains_user_id() {
        let token = create_token(42, Duration::hours(1));
        let claims = validate_token(&token).unwrap();
        assert_eq!(claims.user_id, 42);
    }

    #[test]
    fn test_invalid_signature() {
        let token = create_token(1, Duration::hours(1));
        let other_secret = b"different-secret";
        let validation = Validation::new(Algorithm::HS256);
        let result = decode::<Claims>(&token, &DecodingKey::from_secret(other_secret), &validation);
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_token() {
        let result = validate_token("not.a.token");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_token() {
        let result = validate_token("");
        assert!(result.is_err());
    }

    #[test]
    fn test_token_with_extra_parts() {
        let token = create_token(1, Duration::hours(1));
        let invalid = format!("{}.extra", token);
        assert!(validate_token(&invalid).is_err());
    }
}

// ============= Rate Limiting Edge Cases =============

#[cfg(test)]
mod rate_limiting_edge_cases {
    use std::time::{Duration, Instant};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct SimpleRateLimiter {
        requests: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
        window: Duration,
        max_requests: usize,
    }

    impl SimpleRateLimiter {
        fn new(window: Duration, max_requests: usize) -> Self {
            Self {
                requests: Arc::new(Mutex::new(HashMap::new())),
                window,
                max_requests,
            }
        }

        fn is_allowed(&self, key: &str) -> bool {
            let now = Instant::now();
            let mut requests = self.requests.lock().unwrap();
            let entry = requests.entry(key.to_string()).or_insert_with(Vec::new);
            
            // Remove old requests
            entry.retain(|t| now.duration_since(*t) < self.window);
            
            if entry.len() < self.max_requests {
                entry.push(now);
                true
            } else {
                false
            }
        }

        fn remaining(&self, key: &str) -> usize {
            let now = Instant::now();
            let requests = self.requests.lock().unwrap();
            if let Some(entry) = requests.get(key) {
                let recent = entry.iter().filter(|t| now.duration_since(**t) < self.window).count();
                self.max_requests.saturating_sub(recent)
            } else {
                self.max_requests
            }
        }
    }

    #[test]
    fn test_allows_under_limit() {
        let limiter = SimpleRateLimiter::new(Duration::from_secs(60), 5);
        for _ in 0..5 {
            assert!(limiter.is_allowed("user1"));
        }
    }

    #[test]
    fn test_blocks_over_limit() {
        let limiter = SimpleRateLimiter::new(Duration::from_secs(60), 5);
        for _ in 0..5 {
            assert!(limiter.is_allowed("user1"));
        }
        assert!(!limiter.is_allowed("user1"));
    }

    #[test]
    fn test_different_keys_independent() {
        let limiter = SimpleRateLimiter::new(Duration::from_secs(60), 2);
        assert!(limiter.is_allowed("user1"));
        assert!(limiter.is_allowed("user1"));
        assert!(!limiter.is_allowed("user1"));
        
        // Different user should have fresh limit
        assert!(limiter.is_allowed("user2"));
        assert!(limiter.is_allowed("user2"));
    }

    #[test]
    fn test_remaining_count() {
        let limiter = SimpleRateLimiter::new(Duration::from_secs(60), 5);
        assert_eq!(limiter.remaining("user1"), 5);
        limiter.is_allowed("user1");
        assert_eq!(limiter.remaining("user1"), 4);
        limiter.is_allowed("user1");
        assert_eq!(limiter.remaining("user1"), 3);
    }

    #[test]
    fn test_zero_limit() {
        let limiter = SimpleRateLimiter::new(Duration::from_secs(60), 0);
        assert!(!limiter.is_allowed("user1"));
    }

    #[test]
    fn test_very_short_window() {
        let limiter = SimpleRateLimiter::new(Duration::from_millis(1), 1);
        assert!(limiter.is_allowed("user1"));
        std::thread::sleep(Duration::from_millis(10));
        // Should be allowed again after window expires
        assert!(limiter.is_allowed("user1"));
    }
}

// ============= Link Scheduling Edge Cases =============

#[cfg(test)]
mod link_scheduling_edge_cases {
    use chrono::{Utc, Duration, DateTime};

    struct ScheduledLink {
        starts_at: Option<DateTime<Utc>>,
        expires_at: Option<DateTime<Utc>>,
        max_clicks: Option<i32>,
        current_clicks: i32,
    }

    impl ScheduledLink {
        fn is_active(&self) -> bool {
            let now = Utc::now();
            
            // Check if not started yet
            if let Some(starts) = self.starts_at {
                if now < starts {
                    return false;
                }
            }
            
            // Check if expired
            if let Some(expires) = self.expires_at {
                if now > expires {
                    return false;
                }
            }
            
            // Check max clicks
            if let Some(max) = self.max_clicks {
                if self.current_clicks >= max {
                    return false;
                }
            }
            
            true
        }
    }

    #[test]
    fn test_active_no_constraints() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: None,
            max_clicks: None,
            current_clicks: 0,
        };
        assert!(link.is_active());
    }

    #[test]
    fn test_not_started_yet() {
        let link = ScheduledLink {
            starts_at: Some(Utc::now() + Duration::hours(1)),
            expires_at: None,
            max_clicks: None,
            current_clicks: 0,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_already_started() {
        let link = ScheduledLink {
            starts_at: Some(Utc::now() - Duration::hours(1)),
            expires_at: None,
            max_clicks: None,
            current_clicks: 0,
        };
        assert!(link.is_active());
    }

    #[test]
    fn test_not_expired() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            max_clicks: None,
            current_clicks: 0,
        };
        assert!(link.is_active());
    }

    #[test]
    fn test_already_expired() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: Some(Utc::now() - Duration::hours(1)),
            max_clicks: None,
            current_clicks: 0,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_under_max_clicks() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: None,
            max_clicks: Some(100),
            current_clicks: 50,
        };
        assert!(link.is_active());
    }

    #[test]
    fn test_at_max_clicks() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: None,
            max_clicks: Some(100),
            current_clicks: 100,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_over_max_clicks() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: None,
            max_clicks: Some(100),
            current_clicks: 150,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_within_window() {
        let link = ScheduledLink {
            starts_at: Some(Utc::now() - Duration::hours(1)),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            max_clicks: Some(100),
            current_clicks: 50,
        };
        assert!(link.is_active());
    }

    #[test]
    fn test_zero_max_clicks() {
        let link = ScheduledLink {
            starts_at: None,
            expires_at: None,
            max_clicks: Some(0),
            current_clicks: 0,
        };
        assert!(!link.is_active());
    }
}

// ============= Organization Permissions Edge Cases =============

#[cfg(test)]
mod organization_permissions {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Role {
        Owner,
        Admin,
        Member,
    }

    #[derive(Debug, Clone, Copy)]
    enum Permission {
        ViewLinks,
        CreateLinks,
        EditLinks,
        DeleteLinks,
        ManageMembers,
        ManageOrg,
        ViewAuditLog,
    }

    fn has_permission(role: Role, permission: Permission) -> bool {
        match permission {
            Permission::ViewLinks => true, // All roles can view
            Permission::CreateLinks => true, // All roles can create
            Permission::EditLinks => matches!(role, Role::Owner | Role::Admin | Role::Member),
            Permission::DeleteLinks => matches!(role, Role::Owner | Role::Admin),
            Permission::ManageMembers => matches!(role, Role::Owner | Role::Admin),
            Permission::ManageOrg => matches!(role, Role::Owner),
            Permission::ViewAuditLog => matches!(role, Role::Owner | Role::Admin),
        }
    }

    #[test]
    fn test_owner_has_all_permissions() {
        assert!(has_permission(Role::Owner, Permission::ViewLinks));
        assert!(has_permission(Role::Owner, Permission::CreateLinks));
        assert!(has_permission(Role::Owner, Permission::EditLinks));
        assert!(has_permission(Role::Owner, Permission::DeleteLinks));
        assert!(has_permission(Role::Owner, Permission::ManageMembers));
        assert!(has_permission(Role::Owner, Permission::ManageOrg));
        assert!(has_permission(Role::Owner, Permission::ViewAuditLog));
    }

    #[test]
    fn test_admin_permissions() {
        assert!(has_permission(Role::Admin, Permission::ViewLinks));
        assert!(has_permission(Role::Admin, Permission::CreateLinks));
        assert!(has_permission(Role::Admin, Permission::EditLinks));
        assert!(has_permission(Role::Admin, Permission::DeleteLinks));
        assert!(has_permission(Role::Admin, Permission::ManageMembers));
        assert!(!has_permission(Role::Admin, Permission::ManageOrg));
        assert!(has_permission(Role::Admin, Permission::ViewAuditLog));
    }

    #[test]
    fn test_member_permissions() {
        assert!(has_permission(Role::Member, Permission::ViewLinks));
        assert!(has_permission(Role::Member, Permission::CreateLinks));
        assert!(has_permission(Role::Member, Permission::EditLinks));
        assert!(!has_permission(Role::Member, Permission::DeleteLinks));
        assert!(!has_permission(Role::Member, Permission::ManageMembers));
        assert!(!has_permission(Role::Member, Permission::ManageOrg));
        assert!(!has_permission(Role::Member, Permission::ViewAuditLog));
    }
}

// ============= Tag Name Validation Edge Cases =============

#[cfg(test)]
mod tag_name_validation {
    fn is_valid_tag_name(name: &str) -> bool {
        let trimmed = name.trim();
        !trimmed.is_empty() 
            && trimmed.len() <= 50 
            && trimmed.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
    }

    #[test]
    fn test_valid_simple_tag() {
        assert!(is_valid_tag_name("marketing"));
    }

    #[test]
    fn test_valid_tag_with_spaces() {
        assert!(is_valid_tag_name("social media"));
    }

    #[test]
    fn test_valid_tag_with_dashes() {
        assert!(is_valid_tag_name("email-campaign"));
    }

    #[test]
    fn test_valid_tag_with_underscores() {
        assert!(is_valid_tag_name("q4_2024"));
    }

    #[test]
    fn test_valid_tag_with_numbers() {
        assert!(is_valid_tag_name("campaign123"));
    }

    #[test]
    fn test_invalid_empty_tag() {
        assert!(!is_valid_tag_name(""));
    }

    #[test]
    fn test_invalid_whitespace_only() {
        assert!(!is_valid_tag_name("   "));
    }

    #[test]
    fn test_invalid_too_long() {
        let long_name = "a".repeat(51);
        assert!(!is_valid_tag_name(&long_name));
    }

    #[test]
    fn test_invalid_special_chars() {
        assert!(!is_valid_tag_name("tag@name"));
        assert!(!is_valid_tag_name("tag#name"));
        assert!(!is_valid_tag_name("tag!"));
    }

    #[test]
    fn test_valid_max_length() {
        let max_name = "a".repeat(50);
        assert!(is_valid_tag_name(&max_name));
    }
}

// ============= Folder Hierarchy Edge Cases =============

#[cfg(test)]
mod folder_hierarchy {
    struct Folder {
        id: i32,
        name: String,
        parent_id: Option<i32>,
    }

    fn get_depth(folders: &[Folder], folder_id: i32) -> usize {
        let folder = folders.iter().find(|f| f.id == folder_id);
        match folder {
            Some(f) => match f.parent_id {
                Some(parent) => 1 + get_depth(folders, parent),
                None => 0,
            },
            None => 0,
        }
    }

    fn would_create_cycle(folders: &[Folder], folder_id: i32, new_parent_id: i32) -> bool {
        if folder_id == new_parent_id {
            return true;
        }
        
        let mut current_id = Some(new_parent_id);
        while let Some(id) = current_id {
            if id == folder_id {
                return true;
            }
            current_id = folders.iter()
                .find(|f| f.id == id)
                .and_then(|f| f.parent_id);
        }
        false
    }

    #[test]
    fn test_root_folder_depth() {
        let folders = vec![
            Folder { id: 1, name: "Root".to_string(), parent_id: None },
        ];
        assert_eq!(get_depth(&folders, 1), 0);
    }

    #[test]
    fn test_nested_folder_depth() {
        let folders = vec![
            Folder { id: 1, name: "Root".to_string(), parent_id: None },
            Folder { id: 2, name: "Level1".to_string(), parent_id: Some(1) },
            Folder { id: 3, name: "Level2".to_string(), parent_id: Some(2) },
        ];
        assert_eq!(get_depth(&folders, 1), 0);
        assert_eq!(get_depth(&folders, 2), 1);
        assert_eq!(get_depth(&folders, 3), 2);
    }

    #[test]
    fn test_self_reference_cycle() {
        let folders = vec![
            Folder { id: 1, name: "Test".to_string(), parent_id: None },
        ];
        assert!(would_create_cycle(&folders, 1, 1));
    }

    #[test]
    fn test_child_to_parent_cycle() {
        let folders = vec![
            Folder { id: 1, name: "Parent".to_string(), parent_id: None },
            Folder { id: 2, name: "Child".to_string(), parent_id: Some(1) },
        ];
        // Moving parent under child would create cycle
        assert!(would_create_cycle(&folders, 1, 2));
    }

    #[test]
    fn test_no_cycle() {
        let folders = vec![
            Folder { id: 1, name: "A".to_string(), parent_id: None },
            Folder { id: 2, name: "B".to_string(), parent_id: None },
            Folder { id: 3, name: "C".to_string(), parent_id: Some(1) },
        ];
        // Moving B under C is fine
        assert!(!would_create_cycle(&folders, 2, 3));
    }
}

// ============= Bulk Operation Edge Cases =============

#[cfg(test)]
mod bulk_operations {
    const MAX_BULK_SIZE: usize = 100;

    fn validate_bulk_ids(ids: &[i32]) -> Result<(), &'static str> {
        if ids.is_empty() {
            return Err("No IDs provided");
        }
        if ids.len() > MAX_BULK_SIZE {
            return Err("Too many IDs");
        }
        
        // Check for duplicates
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if *id <= 0 {
                return Err("Invalid ID");
            }
            if !seen.insert(id) {
                return Err("Duplicate ID");
            }
        }
        Ok(())
    }

    #[test]
    fn test_valid_bulk_ids() {
        assert!(validate_bulk_ids(&[1, 2, 3]).is_ok());
    }

    #[test]
    fn test_empty_bulk_ids() {
        assert!(validate_bulk_ids(&[]).is_err());
    }

    #[test]
    fn test_too_many_bulk_ids() {
        let ids: Vec<i32> = (1..=101).collect();
        assert!(validate_bulk_ids(&ids).is_err());
    }

    #[test]
    fn test_max_bulk_ids() {
        let ids: Vec<i32> = (1..=100).collect();
        assert!(validate_bulk_ids(&ids).is_ok());
    }

    #[test]
    fn test_duplicate_bulk_ids() {
        assert!(validate_bulk_ids(&[1, 2, 1]).is_err());
    }

    #[test]
    fn test_negative_bulk_id() {
        assert!(validate_bulk_ids(&[1, -1, 2]).is_err());
    }

    #[test]
    fn test_zero_bulk_id() {
        assert!(validate_bulk_ids(&[1, 0, 2]).is_err());
    }
}

// ============= Analytics Aggregation Edge Cases =============

#[cfg(test)]
mod analytics_aggregation {
    use std::collections::HashMap;

    fn calculate_percentage(count: u64, total: u64) -> f64 {
        if total == 0 {
            return 0.0;
        }
        (count as f64 / total as f64) * 100.0
    }

    fn aggregate_by_key<T: std::hash::Hash + Eq + Clone>(items: &[(T, u64)]) -> Vec<(T, u64, f64)> {
        let total: u64 = items.iter().map(|(_, c)| c).sum();
        
        let mut counts: HashMap<T, u64> = HashMap::new();
        for (key, count) in items {
            *counts.entry(key.clone()).or_insert(0) += count;
        }
        
        counts.into_iter()
            .map(|(key, count)| (key, count, calculate_percentage(count, total)))
            .collect()
    }

    #[test]
    fn test_percentage_calculation() {
        assert_eq!(calculate_percentage(50, 100), 50.0);
        assert_eq!(calculate_percentage(0, 100), 0.0);
        assert_eq!(calculate_percentage(100, 100), 100.0);
    }

    #[test]
    fn test_percentage_zero_total() {
        assert_eq!(calculate_percentage(0, 0), 0.0);
        assert_eq!(calculate_percentage(10, 0), 0.0);
    }

    #[test]
    fn test_aggregation_basic() {
        let items = vec![
            ("US", 50u64),
            ("UK", 30),
            ("DE", 20),
        ];
        let result = aggregate_by_key(&items);
        assert_eq!(result.len(), 3);
        
        let us = result.iter().find(|(k, _, _)| *k == "US").unwrap();
        assert_eq!(us.1, 50);
        assert_eq!(us.2, 50.0);
    }

    #[test]
    fn test_aggregation_with_duplicates() {
        let items = vec![
            ("US", 30u64),
            ("UK", 30),
            ("US", 20), // Duplicate country
        ];
        let result = aggregate_by_key(&items);
        
        let us = result.iter().find(|(k, _, _)| *k == "US").unwrap();
        assert_eq!(us.1, 50); // 30 + 20
    }

    #[test]
    fn test_aggregation_empty() {
        let items: Vec<(&str, u64)> = vec![];
        let result = aggregate_by_key(&items);
        assert!(result.is_empty());
    }
}



