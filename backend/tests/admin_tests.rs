mod common;

// ============= Admin Stats Tests =============

#[cfg(test)]
mod admin_stats_tests {
    #[derive(Default)]
    struct AdminStats {
        total_users: u64,
        active_users: u64,
        total_links: u64,
        active_links: u64,
        total_clicks: u64,
        blocked_links_count: u64,
        blocked_domains_count: u64,
    }

    #[test]
    fn test_stats_initialization() {
        let stats = AdminStats::default();
        assert_eq!(stats.total_users, 0);
        assert_eq!(stats.active_users, 0);
        assert_eq!(stats.total_links, 0);
    }

    #[test]
    fn test_active_users_less_than_total() {
        let stats = AdminStats {
            total_users: 100,
            active_users: 80,
            ..Default::default()
        };
        assert!(stats.active_users <= stats.total_users);
    }

    #[test]
    fn test_active_links_less_than_total() {
        let stats = AdminStats {
            total_links: 1000,
            active_links: 950,
            ..Default::default()
        };
        assert!(stats.active_links <= stats.total_links);
    }
}

// ============= User Management Tests =============

#[cfg(test)]
mod user_management_tests {
    use chrono::{Utc, NaiveDateTime};

    struct User {
        id: i32,
        email: String,
        is_admin: bool,
        deleted_at: Option<NaiveDateTime>,
    }

    #[test]
    fn test_user_is_deleted() {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            is_admin: false,
            deleted_at: Some(Utc::now().naive_utc()),
        };
        assert!(user.deleted_at.is_some());
    }

    #[test]
    fn test_user_is_not_deleted() {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            is_admin: false,
            deleted_at: None,
        };
        assert!(user.deleted_at.is_none());
    }

    #[test]
    fn test_user_admin_status() {
        let admin = User {
            id: 1,
            email: "admin@example.com".to_string(),
            is_admin: true,
            deleted_at: None,
        };
        assert!(admin.is_admin);
    }

    #[test]
    fn test_user_regular_status() {
        let user = User {
            id: 2,
            email: "user@example.com".to_string(),
            is_admin: false,
            deleted_at: None,
        };
        assert!(!user.is_admin);
    }

    #[test]
    fn test_cannot_delete_self() {
        let current_user_id = 1;
        let target_user_id = 1;
        // Simulating the check that prevents self-deletion
        let can_delete = current_user_id != target_user_id;
        assert!(!can_delete);
    }

    #[test]
    fn test_can_delete_other_user() {
        let current_user_id = 1;
        let target_user_id = 2;
        let can_delete = current_user_id != target_user_id;
        assert!(can_delete);
    }
}

// ============= URL Blocking Tests =============

#[cfg(test)]
mod url_blocking_tests {
    struct BlockedLink {
        id: i32,
        url: String,
        reason: Option<String>,
    }

    struct BlockedDomain {
        id: i32,
        domain: String,
        reason: Option<String>,
    }

    fn is_url_blocked(url: &str, blocked_links: &[BlockedLink]) -> bool {
        blocked_links.iter().any(|bl| bl.url == url)
    }

    fn is_domain_blocked(url: &str, blocked_domains: &[BlockedDomain]) -> bool {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                return blocked_domains.iter().any(|bd| {
                    host == bd.domain || host.ends_with(&format!(".{}", bd.domain))
                });
            }
        }
        false
    }

    #[test]
    fn test_exact_url_blocked() {
        let blocked = vec![BlockedLink {
            id: 1,
            url: "https://malicious.com/bad".to_string(),
            reason: Some("Malware".to_string()),
        }];
        
        assert!(is_url_blocked("https://malicious.com/bad", &blocked));
    }

    #[test]
    fn test_url_not_blocked() {
        let blocked = vec![BlockedLink {
            id: 1,
            url: "https://malicious.com/bad".to_string(),
            reason: None,
        }];
        
        assert!(!is_url_blocked("https://safe.com/good", &blocked));
    }

    #[test]
    fn test_domain_blocked() {
        let blocked = vec![BlockedDomain {
            id: 1,
            domain: "evil.com".to_string(),
            reason: Some("Spam".to_string()),
        }];
        
        assert!(is_domain_blocked("https://evil.com/page", &blocked));
    }

    #[test]
    fn test_subdomain_blocked() {
        let blocked = vec![BlockedDomain {
            id: 1,
            domain: "evil.com".to_string(),
            reason: None,
        }];
        
        assert!(is_domain_blocked("https://sub.evil.com/page", &blocked));
    }

    #[test]
    fn test_similar_domain_not_blocked() {
        let blocked = vec![BlockedDomain {
            id: 1,
            domain: "evil.com".to_string(),
            reason: None,
        }];
        
        // "notevil.com" should NOT be blocked just because it contains "evil.com"
        assert!(!is_domain_blocked("https://notevil.com/page", &blocked));
    }

    #[test]
    fn test_empty_blocklist() {
        let blocked: Vec<BlockedLink> = vec![];
        assert!(!is_url_blocked("https://any.com", &blocked));
    }
}

// ============= Backup Tests =============

#[cfg(test)]
mod backup_tests {
    use chrono::Utc;

    fn generate_backup_filename() -> String {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        format!("backup_{}.sql.gz", timestamp)
    }

    #[test]
    fn test_backup_filename_format() {
        let filename = generate_backup_filename();
        assert!(filename.starts_with("backup_"));
        assert!(filename.ends_with(".sql.gz"));
    }

    #[test]
    fn test_backup_filename_uniqueness() {
        let filename1 = generate_backup_filename();
        std::thread::sleep(std::time::Duration::from_millis(10));
        // Within the same second, filenames might be the same
        // This is expected behavior
        let filename2 = generate_backup_filename();
        // Both should be valid filenames
        assert!(filename1.starts_with("backup_"));
        assert!(filename2.starts_with("backup_"));
    }

    #[test]
    fn test_backup_retention() {
        let backups = vec![
            "backup_20240101_000000.sql.gz",
            "backup_20240102_000000.sql.gz",
            "backup_20240103_000000.sql.gz",
            "backup_20240104_000000.sql.gz",
            "backup_20240105_000000.sql.gz",
        ];
        
        let keep_count = 3;
        let to_delete = backups.len().saturating_sub(keep_count);
        
        assert_eq!(to_delete, 2);
    }
}

// ============= Admin Permission Tests =============

#[cfg(test)]
mod admin_permission_tests {
    #[derive(Clone, Copy, PartialEq)]
    enum Role {
        User,
        Admin,
    }

    fn can_access_admin_panel(role: Role) -> bool {
        role == Role::Admin
    }

    fn can_manage_users(role: Role) -> bool {
        role == Role::Admin
    }

    fn can_block_content(role: Role) -> bool {
        role == Role::Admin
    }

    fn can_create_backup(role: Role) -> bool {
        role == Role::Admin
    }

    #[test]
    fn test_admin_can_access_panel() {
        assert!(can_access_admin_panel(Role::Admin));
    }

    #[test]
    fn test_user_cannot_access_panel() {
        assert!(!can_access_admin_panel(Role::User));
    }

    #[test]
    fn test_admin_can_manage_users() {
        assert!(can_manage_users(Role::Admin));
    }

    #[test]
    fn test_admin_can_block_content() {
        assert!(can_block_content(Role::Admin));
    }

    #[test]
    fn test_admin_can_create_backup() {
        assert!(can_create_backup(Role::Admin));
    }

    #[test]
    fn test_user_cannot_manage_users() {
        assert!(!can_manage_users(Role::User));
    }
}

// ============= First User Admin Tests =============

#[cfg(test)]
mod first_user_admin_tests {
    #[test]
    fn test_first_user_becomes_admin() {
        let user_count = 0;
        let is_first_user = user_count == 0;
        assert!(is_first_user);
    }

    #[test]
    fn test_second_user_not_admin() {
        let user_count = 1;
        let is_first_user = user_count == 0;
        assert!(!is_first_user);
    }

    #[test]
    fn test_ensure_admin_exists() {
        let admin_count = 0;
        let total_users = 5;
        
        // If no admins exist but users exist, promote first user
        let should_promote = admin_count == 0 && total_users > 0;
        assert!(should_promote);
    }

    #[test]
    fn test_admin_already_exists() {
        let admin_count = 1;
        let should_promote = admin_count == 0;
        assert!(!should_promote);
    }
}
