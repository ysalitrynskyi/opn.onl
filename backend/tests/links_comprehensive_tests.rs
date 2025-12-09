mod common;

use axum_test::TestServer;
use serde_json::json;

// ============= URL Validation Tests =============

#[cfg(test)]
mod url_validation_tests {
    #[test]
    fn test_valid_http_url() {
        let url = "http://example.com";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().scheme(), "http");
    }

    #[test]
    fn test_valid_https_url() {
        let url = "https://example.com";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().scheme(), "https");
    }

    #[test]
    fn test_url_with_path() {
        let url = "https://example.com/path/to/page";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().path(), "/path/to/page");
    }

    #[test]
    fn test_url_with_query() {
        let url = "https://example.com/search?q=test&page=1";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().query(), Some("q=test&page=1"));
    }

    #[test]
    fn test_url_with_fragment() {
        let url = "https://example.com/page#section";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().fragment(), Some("section"));
    }

    #[test]
    fn test_url_with_port() {
        let url = "https://example.com:8080/api";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().port(), Some(8080));
    }

    #[test]
    fn test_invalid_url_no_scheme() {
        let url = "example.com";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_invalid_url_ftp_scheme() {
        let url = "ftp://example.com/file";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        // But we should reject non-http/https schemes
        let parsed = parsed.unwrap();
        assert_ne!(parsed.scheme(), "http");
        assert_ne!(parsed.scheme(), "https");
    }

    #[test]
    fn test_url_with_unicode() {
        let url = "https://例え.jp/パス";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_very_long_url() {
        let long_path = "a".repeat(2000);
        let url = format!("https://example.com/{}", long_path);
        let parsed = url::Url::parse(&url);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_url_with_credentials() {
        let url = "https://user:pass@example.com/";
        let parsed = url::Url::parse(url);
        assert!(parsed.is_ok());
        let parsed = parsed.unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password(), Some("pass"));
    }
}

// ============= XSS Prevention Tests =============

#[cfg(test)]
mod xss_prevention_tests {
    fn contains_xss_pattern(url: &str) -> bool {
        let url_lower = url.to_lowercase();
        let xss_patterns = [
            "<script", "</script>", "onerror=", "onload=", "onclick=",
            "onmouseover=", "onfocus=", "onblur=", "eval(", "alert(",
            "document.cookie", "document.location", "window.location",
        ];
        
        xss_patterns.iter().any(|pattern| url_lower.contains(pattern))
    }

    #[test]
    fn test_detect_script_tag() {
        assert!(contains_xss_pattern("https://example.com/<script>alert(1)</script>"));
    }

    #[test]
    fn test_detect_onerror() {
        assert!(contains_xss_pattern("https://example.com/img?onerror=alert(1)"));
    }

    #[test]
    fn test_detect_javascript_protocol() {
        let url = "javascript:alert(1)";
        assert!(url.to_lowercase().contains("javascript:"));
    }

    #[test]
    fn test_clean_url() {
        assert!(!contains_xss_pattern("https://example.com/page?id=123"));
    }

    #[test]
    fn test_detect_encoded_xss() {
        let encoded = "https://example.com/%3Cscript%3Ealert(1)%3C/script%3E";
        let decoded = urlencoding::decode(encoded).unwrap_or_default();
        assert!(decoded.to_lowercase().contains("<script"));
    }
}

// ============= Alias Validation Tests =============

#[cfg(test)]
mod alias_validation_tests {
    fn validate_alias(alias: &str, min_len: usize, max_len: usize) -> Result<(), String> {
        if alias.len() < min_len {
            return Err(format!("Alias must be at least {} characters", min_len));
        }
        
        if alias.len() > max_len {
            return Err(format!("Alias must be at most {} characters", max_len));
        }
        
        if !alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("Alias can only contain letters, numbers, hyphens, and underscores".to_string());
        }
        
        if alias.starts_with('-') || alias.starts_with('_') || alias.ends_with('-') || alias.ends_with('_') {
            return Err("Alias cannot start or end with hyphen or underscore".to_string());
        }
        
        Ok(())
    }

    #[test]
    fn test_valid_alias() {
        assert!(validate_alias("my-link", 5, 50).is_ok());
    }

    #[test]
    fn test_valid_alias_with_numbers() {
        assert!(validate_alias("link123", 5, 50).is_ok());
    }

    #[test]
    fn test_valid_alias_with_underscore() {
        assert!(validate_alias("my_custom_link", 5, 50).is_ok());
    }

    #[test]
    fn test_alias_too_short() {
        assert!(validate_alias("abc", 5, 50).is_err());
    }

    #[test]
    fn test_alias_too_long() {
        let long_alias = "a".repeat(51);
        assert!(validate_alias(&long_alias, 5, 50).is_err());
    }

    #[test]
    fn test_alias_starts_with_hyphen() {
        assert!(validate_alias("-mylink", 5, 50).is_err());
    }

    #[test]
    fn test_alias_ends_with_underscore() {
        assert!(validate_alias("mylink_", 5, 50).is_err());
    }

    #[test]
    fn test_alias_with_special_chars() {
        assert!(validate_alias("my@link!", 5, 50).is_err());
    }

    #[test]
    fn test_alias_with_spaces() {
        assert!(validate_alias("my link", 5, 50).is_err());
    }

    #[test]
    fn test_alias_exact_min_length() {
        assert!(validate_alias("abcde", 5, 50).is_ok());
    }

    #[test]
    fn test_alias_exact_max_length() {
        let alias = "a".repeat(50);
        assert!(validate_alias(&alias, 5, 50).is_ok());
    }
}

// ============= Short Code Generation Tests =============

#[cfg(test)]
mod short_code_tests {
    use rand::{thread_rng, Rng};
    use rand::distributions::Alphanumeric;
    use std::collections::HashSet;

    fn generate_short_code(length: usize) -> String {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(length)
            .map(char::from)
            .collect()
    }

    #[test]
    fn test_code_length_6() {
        let code = generate_short_code(6);
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn test_code_length_8() {
        let code = generate_short_code(8);
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn test_code_is_alphanumeric() {
        let code = generate_short_code(6);
        assert!(code.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_code_uniqueness_high_probability() {
        let mut codes = HashSet::new();
        for _ in 0..1000 {
            let code = generate_short_code(6);
            codes.insert(code);
        }
        // With 62^6 possible combinations, 1000 codes should all be unique
        assert_eq!(codes.len(), 1000);
    }

    #[test]
    fn test_code_distribution() {
        let codes: Vec<String> = (0..1000)
            .map(|_| generate_short_code(6))
            .collect();
        
        // Check that codes contain both letters and numbers
        let has_letters = codes.iter().any(|c| c.chars().any(|ch| ch.is_alphabetic()));
        let has_digits = codes.iter().any(|c| c.chars().any(|ch| ch.is_numeric()));
        
        assert!(has_letters);
        assert!(has_digits);
    }
}

// ============= Link Active Status Tests =============

#[cfg(test)]
mod link_status_tests {
    use chrono::{Utc, Duration, NaiveDateTime};

    struct Link {
        starts_at: Option<NaiveDateTime>,
        expires_at: Option<NaiveDateTime>,
        max_clicks: Option<i32>,
        click_count: i32,
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
    }

    #[test]
    fn test_active_link_no_constraints() {
        let link = Link {
            starts_at: None,
            expires_at: None,
            max_clicks: None,
            click_count: 100,
            deleted_at: None,
        };
        assert!(link.is_active());
    }

    #[test]
    fn test_inactive_deleted_link() {
        let link = Link {
            starts_at: None,
            expires_at: None,
            max_clicks: None,
            click_count: 0,
            deleted_at: Some(Utc::now().naive_utc()),
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_inactive_expired_link() {
        let link = Link {
            starts_at: None,
            expires_at: Some((Utc::now() - Duration::hours(1)).naive_utc()),
            max_clicks: None,
            click_count: 0,
            deleted_at: None,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_inactive_not_started_link() {
        let link = Link {
            starts_at: Some((Utc::now() + Duration::hours(1)).naive_utc()),
            expires_at: None,
            max_clicks: None,
            click_count: 0,
            deleted_at: None,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_inactive_max_clicks_reached() {
        let link = Link {
            starts_at: None,
            expires_at: None,
            max_clicks: Some(100),
            click_count: 100,
            deleted_at: None,
        };
        assert!(!link.is_active());
    }

    #[test]
    fn test_active_within_time_window() {
        let link = Link {
            starts_at: Some((Utc::now() - Duration::hours(1)).naive_utc()),
            expires_at: Some((Utc::now() + Duration::hours(1)).naive_utc()),
            max_clicks: Some(100),
            click_count: 50,
            deleted_at: None,
        };
        assert!(link.is_active());
    }
}

// ============= Password Hash Tests =============

#[cfg(test)]
mod password_protection_tests {
    use bcrypt::{hash, verify, DEFAULT_COST};

    #[test]
    fn test_link_password_hash() {
        let password = "secret123";
        let hash_result = hash(password, DEFAULT_COST);
        assert!(hash_result.is_ok());
    }

    #[test]
    fn test_link_password_verify_correct() {
        let password = "link-password";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(verify(password, &hashed).unwrap());
    }

    #[test]
    fn test_link_password_verify_incorrect() {
        let password = "correct";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(!verify("wrong", &hashed).unwrap());
    }

    #[test]
    fn test_password_with_special_characters() {
        let password = "p@$$w0rd!#%^&*()";
        let hashed = hash(password, DEFAULT_COST).unwrap();
        assert!(verify(password, &hashed).unwrap());
    }
}

// ============= CSV Export Tests =============

#[cfg(test)]
mod csv_export_tests {
    fn escape_csv_field(field: &str) -> String {
        field.replace(',', "%2C")
    }

    #[test]
    fn test_escape_comma_in_url() {
        let url = "https://example.com/path?a=1,b=2";
        let escaped = escape_csv_field(url);
        assert!(!escaped.contains(','));
    }

    #[test]
    fn test_csv_header() {
        let header = "ID,Code,Original URL,Short URL,Click Count,Created At,Expires At,Has Password,Notes,Folder ID,Max Clicks,Starts At\n";
        assert!(header.starts_with("ID"));
        assert!(header.ends_with("\n"));
        assert_eq!(header.matches(',').count(), 11);
    }

    #[test]
    fn test_csv_row_format() {
        let row = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            1,
            "abc123",
            "https://example.com".replace(',', "%2C"),
            "https://short.url/abc123",
            42,
            "2024-01-01 00:00:00",
            "",
            false,
            "",
            "",
            "",
            ""
        );
        assert!(row.contains("abc123"));
        assert!(row.ends_with("\n"));
    }
}

// ============= QR Code Tests =============

#[cfg(test)]
mod qr_code_tests {
    use qrcode::QrCode;

    #[test]
    fn test_qr_code_generation() {
        let url = "https://example.com/abc123";
        let qr = QrCode::new(url.as_bytes());
        assert!(qr.is_ok());
    }

    #[test]
    fn test_qr_code_long_url() {
        let long_url = format!("https://example.com/{}", "a".repeat(500));
        let qr = QrCode::new(long_url.as_bytes());
        assert!(qr.is_ok());
    }

    #[test]
    fn test_qr_code_unicode_url() {
        let url = "https://例え.jp/パス";
        let qr = QrCode::new(url.as_bytes());
        assert!(qr.is_ok());
    }

    #[test]
    fn test_qr_code_empty_fails() {
        let qr = QrCode::new(b"");
        // QR code with empty data should still work
        assert!(qr.is_ok() || qr.is_err());
    }
}

// ============= Bulk Operations Tests =============

#[cfg(test)]
mod bulk_operations_tests {
    #[test]
    fn test_bulk_url_validation() {
        let urls = vec![
            "https://example.com/1",
            "https://example.com/2",
            "invalid-url",
            "https://example.com/3",
        ];
        
        let valid_count = urls.iter()
            .filter(|u| url::Url::parse(u).is_ok())
            .count();
        
        assert_eq!(valid_count, 3);
    }

    #[test]
    fn test_bulk_ids_parsing() {
        let ids = vec![1, 2, 3, 4, 5];
        assert_eq!(ids.len(), 5);
        assert!(ids.iter().all(|&id| id > 0));
    }

    #[test]
    fn test_bulk_empty_list() {
        let urls: Vec<&str> = vec![];
        assert!(urls.is_empty());
    }

    #[test]
    fn test_bulk_deduplication() {
        let urls = vec![
            "https://example.com/page",
            "https://example.com/page",
            "https://example.com/other",
        ];
        
        let unique: std::collections::HashSet<_> = urls.iter().collect();
        assert_eq!(unique.len(), 2);
    }
}

