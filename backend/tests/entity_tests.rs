mod common;

// ============= Link Entity Tests =============

#[cfg(test)]
mod link_entity_tests {
    use chrono::{Duration, NaiveDateTime, Utc};

    #[derive(Clone)]
    struct Link {
        id: i32,
        code: String,
        original_url: String,
        user_id: Option<i32>,
        click_count: i32,
        expires_at: Option<NaiveDateTime>,
        starts_at: Option<NaiveDateTime>,
        max_clicks: Option<i32>,
        password_hash: Option<String>,
        title: Option<String>,
        notes: Option<String>,
        folder_id: Option<i32>,
        org_id: Option<i32>,
        deleted_at: Option<NaiveDateTime>,
    }

    impl Link {
        fn is_active(&self) -> bool {
            if self.deleted_at.is_some() {
                return false;
            }

            let now = Utc::now().naive_utc();
            
            if let Some(starts_at) = self.starts_at {
                if now < starts_at {
                    return false;
                }
            }
            
            if let Some(expires_at) = self.expires_at {
                if now > expires_at {
                    return false;
                }
            }
            
            if let Some(max_clicks) = self.max_clicks {
                if self.click_count >= max_clicks {
                    return false;
                }
            }
            
            true
        }

        fn is_password_protected(&self) -> bool {
            self.password_hash.is_some()
        }

        fn is_deleted(&self) -> bool {
            self.deleted_at.is_some()
        }
    }

    fn create_test_link() -> Link {
        Link {
            id: 1,
            code: "abc123".to_string(),
            original_url: "https://example.com".to_string(),
            user_id: Some(1),
            click_count: 0,
            expires_at: None,
            starts_at: None,
            max_clicks: None,
            password_hash: None,
            title: None,
            notes: None,
            folder_id: None,
            org_id: None,
            deleted_at: None,
        }
    }

    #[test]
    fn test_new_link_is_active() {
        let link = create_test_link();
        assert!(link.is_active());
    }

    #[test]
    fn test_deleted_link_is_inactive() {
        let mut link = create_test_link();
        link.deleted_at = Some(Utc::now().naive_utc());
        assert!(!link.is_active());
    }

    #[test]
    fn test_expired_link_is_inactive() {
        let mut link = create_test_link();
        link.expires_at = Some((Utc::now() - Duration::hours(1)).naive_utc());
        assert!(!link.is_active());
    }

    #[test]
    fn test_scheduled_link_is_inactive() {
        let mut link = create_test_link();
        link.starts_at = Some((Utc::now() + Duration::hours(1)).naive_utc());
        assert!(!link.is_active());
    }

    #[test]
    fn test_max_clicks_reached_is_inactive() {
        let mut link = create_test_link();
        link.max_clicks = Some(100);
        link.click_count = 100;
        assert!(!link.is_active());
    }

    #[test]
    fn test_max_clicks_not_reached_is_active() {
        let mut link = create_test_link();
        link.max_clicks = Some(100);
        link.click_count = 50;
        assert!(link.is_active());
    }

    #[test]
    fn test_link_within_schedule_is_active() {
        let mut link = create_test_link();
        link.starts_at = Some((Utc::now() - Duration::hours(1)).naive_utc());
        link.expires_at = Some((Utc::now() + Duration::hours(1)).naive_utc());
        assert!(link.is_active());
    }

    #[test]
    fn test_password_protected_link() {
        let mut link = create_test_link();
        link.password_hash = Some("$2b$12$...".to_string());
        assert!(link.is_password_protected());
    }

    #[test]
    fn test_non_password_protected_link() {
        let link = create_test_link();
        assert!(!link.is_password_protected());
    }

    #[test]
    fn test_is_deleted() {
        let mut link = create_test_link();
        assert!(!link.is_deleted());
        
        link.deleted_at = Some(Utc::now().naive_utc());
        assert!(link.is_deleted());
    }

    #[test]
    fn test_link_with_folder() {
        let mut link = create_test_link();
        link.folder_id = Some(5);
        assert_eq!(link.folder_id, Some(5));
    }

    #[test]
    fn test_link_with_organization() {
        let mut link = create_test_link();
        link.org_id = Some(3);
        assert_eq!(link.org_id, Some(3));
    }

    #[test]
    fn test_anonymous_link() {
        let mut link = create_test_link();
        link.user_id = None;
        assert!(link.user_id.is_none());
    }
}

// ============= User Entity Tests =============

#[cfg(test)]
mod user_entity_tests {
    use chrono::{NaiveDateTime, Utc};

    struct User {
        id: i32,
        email: String,
        password_hash: String,
        is_admin: bool,
        email_verified: bool,
        verification_token: Option<String>,
        verification_token_expires: Option<NaiveDateTime>,
        password_reset_token: Option<String>,
        password_reset_expires: Option<NaiveDateTime>,
        deleted_at: Option<NaiveDateTime>,
        display_name: Option<String>,
        bio: Option<String>,
        website: Option<String>,
        location: Option<String>,
    }

    impl User {
        fn is_deleted(&self) -> bool {
            self.deleted_at.is_some()
        }

        fn has_pending_verification(&self) -> bool {
            self.verification_token.is_some() && !self.email_verified
        }

        fn has_pending_password_reset(&self) -> bool {
            if let (Some(_token), Some(expires)) = (&self.password_reset_token, self.password_reset_expires) {
                return Utc::now().naive_utc() < expires;
            }
            false
        }
    }

    fn create_test_user() -> User {
        User {
            id: 1,
            email: "test@example.com".to_string(),
            password_hash: "$2b$12$...".to_string(),
            is_admin: false,
            email_verified: true,
            verification_token: None,
            verification_token_expires: None,
            password_reset_token: None,
            password_reset_expires: None,
            deleted_at: None,
            display_name: None,
            bio: None,
            website: None,
            location: None,
        }
    }

    #[test]
    fn test_verified_user() {
        let user = create_test_user();
        assert!(user.email_verified);
        assert!(!user.has_pending_verification());
    }

    #[test]
    fn test_unverified_user() {
        let mut user = create_test_user();
        user.email_verified = false;
        user.verification_token = Some("token123".to_string());
        
        assert!(!user.email_verified);
        assert!(user.has_pending_verification());
    }

    #[test]
    fn test_admin_user() {
        let mut user = create_test_user();
        user.is_admin = true;
        assert!(user.is_admin);
    }

    #[test]
    fn test_deleted_user() {
        let mut user = create_test_user();
        user.deleted_at = Some(Utc::now().naive_utc());
        assert!(user.is_deleted());
    }

    #[test]
    fn test_user_with_profile() {
        let mut user = create_test_user();
        user.display_name = Some("John Doe".to_string());
        user.bio = Some("Software developer".to_string());
        user.website = Some("https://johndoe.com".to_string());
        user.location = Some("San Francisco, CA".to_string());
        
        assert_eq!(user.display_name.as_deref(), Some("John Doe"));
        assert!(user.bio.is_some());
    }

    #[test]
    fn test_pending_password_reset() {
        let mut user = create_test_user();
        user.password_reset_token = Some("reset-token".to_string());
        user.password_reset_expires = Some((Utc::now() + chrono::Duration::hours(1)).naive_utc());
        
        assert!(user.has_pending_password_reset());
    }

    #[test]
    fn test_expired_password_reset() {
        let mut user = create_test_user();
        user.password_reset_token = Some("reset-token".to_string());
        user.password_reset_expires = Some((Utc::now() - chrono::Duration::hours(1)).naive_utc());
        
        assert!(!user.has_pending_password_reset());
    }
}

// ============= Click Event Entity Tests =============

#[cfg(test)]
mod click_event_entity_tests {
    use chrono::{NaiveDateTime, Utc};

    struct ClickEvent {
        id: i32,
        link_id: i32,
        ip_address: Option<String>,
        user_agent: Option<String>,
        referer: Option<String>,
        country: Option<String>,
        city: Option<String>,
        device: Option<String>,
        browser: Option<String>,
        os: Option<String>,
        created_at: NaiveDateTime,
    }

    #[test]
    fn test_click_event_with_all_data() {
        let event = ClickEvent {
            id: 1,
            link_id: 42,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            referer: Some("https://google.com".to_string()),
            country: Some("US".to_string()),
            city: Some("New York".to_string()),
            device: Some("Desktop".to_string()),
            browser: Some("Chrome".to_string()),
            os: Some("Windows".to_string()),
            created_at: Utc::now().naive_utc(),
        };

        assert_eq!(event.link_id, 42);
        assert!(event.ip_address.is_some());
        assert_eq!(event.country.as_deref(), Some("US"));
    }

    #[test]
    fn test_click_event_minimal_data() {
        let event = ClickEvent {
            id: 1,
            link_id: 42,
            ip_address: None,
            user_agent: None,
            referer: None,
            country: None,
            city: None,
            device: None,
            browser: None,
            os: None,
            created_at: Utc::now().naive_utc(),
        };

        assert_eq!(event.link_id, 42);
        assert!(event.ip_address.is_none());
    }

    #[test]
    fn test_click_event_direct_traffic() {
        let event = ClickEvent {
            id: 1,
            link_id: 42,
            ip_address: Some("10.0.0.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            referer: None, // Direct traffic
            country: None,
            city: None,
            device: None,
            browser: None,
            os: None,
            created_at: Utc::now().naive_utc(),
        };

        assert!(event.referer.is_none());
    }
}

// ============= Organization Entity Tests =============

#[cfg(test)]
mod organization_entity_tests {
    use chrono::{NaiveDateTime, Utc};

    struct Organization {
        id: i32,
        name: String,
        slug: String,
        owner_id: i32,
        created_at: NaiveDateTime,
    }

    struct OrgMember {
        id: i32,
        org_id: i32,
        user_id: i32,
        role: String,
    }

    impl OrgMember {
        fn can_create_links(&self) -> bool {
            matches!(self.role.as_str(), "owner" | "admin" | "member")
        }

        fn can_manage_members(&self) -> bool {
            matches!(self.role.as_str(), "owner" | "admin")
        }

        fn can_delete_org(&self) -> bool {
            self.role == "owner"
        }
    }

    #[test]
    fn test_organization_creation() {
        let org = Organization {
            id: 1,
            name: "Test Org".to_string(),
            slug: "test-org".to_string(),
            owner_id: 1,
            created_at: Utc::now().naive_utc(),
        };

        assert_eq!(org.name, "Test Org");
        assert_eq!(org.slug, "test-org");
    }

    #[test]
    fn test_owner_permissions() {
        let member = OrgMember {
            id: 1,
            org_id: 1,
            user_id: 1,
            role: "owner".to_string(),
        };

        assert!(member.can_create_links());
        assert!(member.can_manage_members());
        assert!(member.can_delete_org());
    }

    #[test]
    fn test_admin_permissions() {
        let member = OrgMember {
            id: 2,
            org_id: 1,
            user_id: 2,
            role: "admin".to_string(),
        };

        assert!(member.can_create_links());
        assert!(member.can_manage_members());
        assert!(!member.can_delete_org());
    }

    #[test]
    fn test_member_permissions() {
        let member = OrgMember {
            id: 3,
            org_id: 1,
            user_id: 3,
            role: "member".to_string(),
        };

        assert!(member.can_create_links());
        assert!(!member.can_manage_members());
        assert!(!member.can_delete_org());
    }

    #[test]
    fn test_viewer_permissions() {
        let member = OrgMember {
            id: 4,
            org_id: 1,
            user_id: 4,
            role: "viewer".to_string(),
        };

        assert!(!member.can_create_links());
        assert!(!member.can_manage_members());
        assert!(!member.can_delete_org());
    }
}

// ============= Folder Entity Tests =============

#[cfg(test)]
mod folder_entity_tests {
    struct Folder {
        id: i32,
        name: String,
        color: Option<String>,
        user_id: Option<i32>,
        org_id: Option<i32>,
    }

    #[test]
    fn test_personal_folder() {
        let folder = Folder {
            id: 1,
            name: "My Links".to_string(),
            color: Some("#3b82f6".to_string()),
            user_id: Some(1),
            org_id: None,
        };

        assert!(folder.user_id.is_some());
        assert!(folder.org_id.is_none());
    }

    #[test]
    fn test_org_folder() {
        let folder = Folder {
            id: 2,
            name: "Team Links".to_string(),
            color: Some("#10b981".to_string()),
            user_id: None,
            org_id: Some(1),
        };

        assert!(folder.user_id.is_none());
        assert!(folder.org_id.is_some());
    }

    #[test]
    fn test_folder_with_color() {
        let folder = Folder {
            id: 1,
            name: "Important".to_string(),
            color: Some("#ef4444".to_string()),
            user_id: Some(1),
            org_id: None,
        };

        assert!(folder.color.is_some());
        assert!(folder.color.as_ref().unwrap().starts_with('#'));
    }

    #[test]
    fn test_folder_without_color() {
        let folder = Folder {
            id: 1,
            name: "Default".to_string(),
            color: None,
            user_id: Some(1),
            org_id: None,
        };

        assert!(folder.color.is_none());
    }
}

// ============= Tag Entity Tests =============

#[cfg(test)]
mod tag_entity_tests {
    struct Tag {
        id: i32,
        name: String,
        color: Option<String>,
        user_id: Option<i32>,
        org_id: Option<i32>,
    }

    struct LinkTag {
        id: i32,
        link_id: i32,
        tag_id: i32,
    }

    #[test]
    fn test_tag_creation() {
        let tag = Tag {
            id: 1,
            name: "marketing".to_string(),
            color: Some("#8b5cf6".to_string()),
            user_id: Some(1),
            org_id: None,
        };

        assert_eq!(tag.name, "marketing");
    }

    #[test]
    fn test_link_tag_association() {
        let link_tag = LinkTag {
            id: 1,
            link_id: 42,
            tag_id: 5,
        };

        assert_eq!(link_tag.link_id, 42);
        assert_eq!(link_tag.tag_id, 5);
    }

    #[test]
    fn test_multiple_tags_on_link() {
        let link_tags = vec![
            LinkTag { id: 1, link_id: 42, tag_id: 1 },
            LinkTag { id: 2, link_id: 42, tag_id: 2 },
            LinkTag { id: 3, link_id: 42, tag_id: 3 },
        ];

        let tag_count = link_tags.iter().filter(|lt| lt.link_id == 42).count();
        assert_eq!(tag_count, 3);
    }
}

// ============= Passkey Entity Tests =============

#[cfg(test)]
mod passkey_entity_tests {
    use chrono::{NaiveDateTime, Utc};

    struct Passkey {
        id: i32,
        user_id: i32,
        name: String,
        credential_id: String,
        public_key: String,
        created_at: NaiveDateTime,
        last_used: Option<NaiveDateTime>,
    }

    #[test]
    fn test_passkey_creation() {
        let passkey = Passkey {
            id: 1,
            user_id: 1,
            name: "MacBook Pro".to_string(),
            credential_id: "base64-credential-id".to_string(),
            public_key: "base64-public-key".to_string(),
            created_at: Utc::now().naive_utc(),
            last_used: None,
        };

        assert_eq!(passkey.name, "MacBook Pro");
        assert!(passkey.last_used.is_none());
    }

    #[test]
    fn test_passkey_with_usage() {
        let passkey = Passkey {
            id: 1,
            user_id: 1,
            name: "iPhone".to_string(),
            credential_id: "cred-id".to_string(),
            public_key: "pub-key".to_string(),
            created_at: Utc::now().naive_utc(),
            last_used: Some(Utc::now().naive_utc()),
        };

        assert!(passkey.last_used.is_some());
    }
}

// ============= Audit Log Entity Tests =============

#[cfg(test)]
mod audit_log_entity_tests {
    use chrono::{NaiveDateTime, Utc};

    struct AuditLog {
        id: i32,
        org_id: i32,
        user_id: Option<i32>,
        action: String,
        resource_type: String,
        resource_id: Option<i32>,
        details: Option<String>,
        ip_address: Option<String>,
        created_at: NaiveDateTime,
    }

    #[test]
    fn test_audit_log_link_created() {
        let log = AuditLog {
            id: 1,
            org_id: 1,
            user_id: Some(1),
            action: "create".to_string(),
            resource_type: "link".to_string(),
            resource_id: Some(42),
            details: Some(r#"{"url":"https://example.com"}"#.to_string()),
            ip_address: Some("192.168.1.1".to_string()),
            created_at: Utc::now().naive_utc(),
        };

        assert_eq!(log.action, "create");
        assert_eq!(log.resource_type, "link");
    }

    #[test]
    fn test_audit_log_member_added() {
        let log = AuditLog {
            id: 2,
            org_id: 1,
            user_id: Some(1),
            action: "add_member".to_string(),
            resource_type: "organization".to_string(),
            resource_id: Some(1),
            details: Some(r#"{"member_id":5,"role":"member"}"#.to_string()),
            ip_address: None,
            created_at: Utc::now().naive_utc(),
        };

        assert_eq!(log.action, "add_member");
    }

    #[test]
    fn test_audit_log_system_action() {
        let log = AuditLog {
            id: 3,
            org_id: 1,
            user_id: None, // System action
            action: "scheduled_cleanup".to_string(),
            resource_type: "system".to_string(),
            resource_id: None,
            details: None,
            ip_address: None,
            created_at: Utc::now().naive_utc(),
        };

        assert!(log.user_id.is_none());
    }
}

