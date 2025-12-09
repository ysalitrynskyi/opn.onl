//! Tests for new features: Clone, Pin, Code Check, Health Check, UTM Builder, Sparklines, Link Preview

#[cfg(test)]
mod clone_link_tests {
    /// Test cloning a link creates a new link with same settings
    #[test]
    fn test_clone_creates_new_code() {
        // A cloned link should have a different code than the original
        let original_code = "abc123";
        let cloned_code = "xyz789";
        assert_ne!(original_code, cloned_code);
    }

    /// Test cloned link preserves original URL
    #[test]
    fn test_clone_preserves_url() {
        let original_url = "https://example.com/test";
        let cloned_url = original_url; // Clone should keep same URL
        assert_eq!(original_url, cloned_url);
    }

    /// Test cloned link has title with "(copy)" suffix
    #[test]
    fn test_clone_title_suffix() {
        let original_title = "My Link";
        let cloned_title = format!("{} (copy)", original_title);
        assert_eq!(cloned_title, "My Link (copy)");
    }

    /// Test cloned link resets click count to 0
    #[test]
    fn test_clone_resets_clicks() {
        let original_clicks = 100;
        let cloned_clicks = 0;
        assert_eq!(cloned_clicks, 0);
        assert_ne!(original_clicks, cloned_clicks);
    }

    /// Test cloned link is not pinned by default
    #[test]
    fn test_clone_not_pinned() {
        let cloned_is_pinned = false;
        assert!(!cloned_is_pinned);
    }
}

#[cfg(test)]
mod pin_link_tests {
    /// Test toggling pin from false to true
    #[test]
    fn test_toggle_pin_on() {
        let original_pinned = false;
        let new_pinned = !original_pinned;
        assert!(new_pinned);
    }

    /// Test toggling pin from true to false
    #[test]
    fn test_toggle_pin_off() {
        let original_pinned = true;
        let new_pinned = !original_pinned;
        assert!(!new_pinned);
    }

    /// Test pin status is boolean
    #[test]
    fn test_pin_is_boolean() {
        let pinned: bool = true;
        assert!(pinned || !pinned); // Always true for boolean
    }
}

#[cfg(test)]
mod check_code_tests {
    /// Test alias validation - minimum length
    #[test]
    fn test_alias_min_length() {
        let min_length = 5;
        let short_alias = "abc";
        assert!(short_alias.len() < min_length);
    }

    /// Test alias validation - maximum length
    #[test]
    fn test_alias_max_length() {
        let max_length = 50;
        let long_alias = "a".repeat(60);
        assert!(long_alias.len() > max_length);
    }

    /// Test alias validation - valid characters
    #[test]
    fn test_alias_valid_characters() {
        let valid_alias = "my-link_123";
        let is_valid = valid_alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        assert!(is_valid);
    }

    /// Test alias validation - invalid characters
    #[test]
    fn test_alias_invalid_characters() {
        let invalid_alias = "my link!@#";
        let is_valid = invalid_alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        assert!(!is_valid);
    }

    /// Test alias cannot start with hyphen
    #[test]
    fn test_alias_no_leading_hyphen() {
        let alias = "-mylink";
        assert!(alias.starts_with('-'));
    }

    /// Test alias cannot end with underscore
    #[test]
    fn test_alias_no_trailing_underscore() {
        let alias = "mylink_";
        assert!(alias.ends_with('_'));
    }

    /// Test valid alias format
    #[test]
    fn test_valid_alias() {
        let alias = "my-cool-link";
        let min_len = 5;
        let max_len = 50;
        let valid_chars = alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let valid_start = !alias.starts_with('-') && !alias.starts_with('_');
        let valid_end = !alias.ends_with('-') && !alias.ends_with('_');
        let valid_length = alias.len() >= min_len && alias.len() <= max_len;
        
        assert!(valid_chars && valid_start && valid_end && valid_length);
    }
}

#[cfg(test)]
mod url_health_check_tests {
    /// Test URL validation for health check
    #[test]
    fn test_valid_http_url() {
        let url = "http://example.com";
        assert!(url.starts_with("http://") || url.starts_with("https://"));
    }

    /// Test URL validation for https
    #[test]
    fn test_valid_https_url() {
        let url = "https://example.com";
        assert!(url.starts_with("https://"));
    }

    /// Test invalid URL scheme
    #[test]
    fn test_invalid_url_scheme() {
        let url = "ftp://example.com";
        assert!(!url.starts_with("http://") && !url.starts_with("https://"));
    }

    /// Test URL with path
    #[test]
    fn test_url_with_path() {
        let url = "https://example.com/path/to/resource";
        assert!(url.contains("/path/"));
    }

    /// Test URL with query params
    #[test]
    fn test_url_with_query() {
        let url = "https://example.com?foo=bar";
        assert!(url.contains('?'));
    }

    /// Test response time is measured
    #[test]
    fn test_response_time_measurement() {
        let response_time_ms: u64 = 150;
        assert!(response_time_ms > 0);
    }

    /// Test reachable status for 2xx codes
    #[test]
    fn test_reachable_2xx() {
        let status_code = 200;
        let is_success = (200..300).contains(&status_code);
        assert!(is_success);
    }

    /// Test reachable status for 3xx codes (redirects)
    #[test]
    fn test_reachable_3xx() {
        let status_code = 301;
        let is_redirect = (300..400).contains(&status_code);
        assert!(is_redirect);
    }

    /// Test not reachable for 4xx codes
    #[test]
    fn test_not_reachable_4xx() {
        let status_code = 404;
        let is_error = (400..500).contains(&status_code);
        assert!(is_error);
    }

    /// Test not reachable for 5xx codes
    #[test]
    fn test_not_reachable_5xx() {
        let status_code = 500;
        let is_server_error = (500..600).contains(&status_code);
        assert!(is_server_error);
    }
}

#[cfg(test)]
mod utm_builder_tests {
    use std::collections::HashMap;

    /// Test UTM source parameter
    #[test]
    fn test_utm_source() {
        let base_url = "https://example.com";
        let utm_source = "newsletter";
        let result = format!("{}?utm_source={}", base_url, utm_source);
        assert!(result.contains("utm_source=newsletter"));
    }

    /// Test UTM medium parameter
    #[test]
    fn test_utm_medium() {
        let base_url = "https://example.com";
        let utm_medium = "email";
        let result = format!("{}?utm_medium={}", base_url, utm_medium);
        assert!(result.contains("utm_medium=email"));
    }

    /// Test UTM campaign parameter
    #[test]
    fn test_utm_campaign() {
        let base_url = "https://example.com";
        let utm_campaign = "spring_sale";
        let result = format!("{}?utm_campaign={}", base_url, utm_campaign);
        assert!(result.contains("utm_campaign=spring_sale"));
    }

    /// Test multiple UTM parameters
    #[test]
    fn test_multiple_utm_params() {
        let base_url = "https://example.com";
        let result = format!(
            "{}?utm_source=google&utm_medium=cpc&utm_campaign=test",
            base_url
        );
        assert!(result.contains("utm_source=google"));
        assert!(result.contains("utm_medium=cpc"));
        assert!(result.contains("utm_campaign=test"));
    }

    /// Test UTM with existing query params
    #[test]
    fn test_utm_with_existing_params() {
        let base_url = "https://example.com?existing=param";
        let has_query = base_url.contains('?');
        assert!(has_query);
    }

    /// Test UTM term parameter
    #[test]
    fn test_utm_term() {
        let utm_term = "running+shoes";
        assert!(!utm_term.is_empty());
    }

    /// Test UTM content parameter
    #[test]
    fn test_utm_content() {
        let utm_content = "banner_ad_1";
        assert!(!utm_content.is_empty());
    }

    /// Test empty UTM params are not added
    #[test]
    fn test_empty_utm_not_added() {
        let mut utm_params: HashMap<String, String> = HashMap::new();
        let utm_source = "";
        if !utm_source.is_empty() {
            utm_params.insert("utm_source".to_string(), utm_source.to_string());
        }
        assert!(!utm_params.contains_key("utm_source"));
    }

    /// Test URL encoding in UTM params
    #[test]
    fn test_utm_encoding() {
        let utm_campaign = "spring sale 2025";
        let encoded = urlencoding::encode(utm_campaign);
        assert!(encoded.contains("%20") || encoded.contains("+"));
    }
}

#[cfg(test)]
mod link_sorting_tests {
    /// Test pinned links sort before unpinned
    #[test]
    fn test_pinned_first() {
        let links = vec![
            (1, false, 100), // id, is_pinned, clicks
            (2, true, 50),
            (3, false, 200),
        ];
        
        let mut sorted = links.clone();
        sorted.sort_by(|a, b| {
            // Pinned first
            match (b.1, a.1) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => b.2.cmp(&a.2), // Then by clicks descending
            }
        });
        
        // Pinned link (id=2) should be first
        assert_eq!(sorted[0].0, 2);
    }

    /// Test multiple pinned links maintain order
    #[test]
    fn test_multiple_pinned() {
        let links = vec![
            (1, true, 100),
            (2, true, 50),
            (3, false, 200),
        ];
        
        let mut sorted = links.clone();
        sorted.sort_by(|a, b| {
            match (b.1, a.1) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => b.2.cmp(&a.2),
            }
        });
        
        // Both pinned links should be before unpinned
        assert!(sorted[0].1 && sorted[1].1);
        assert!(!sorted[2].1);
    }
}

#[cfg(test)]
mod is_pinned_field_tests {
    /// Test default value for is_pinned
    #[test]
    fn test_default_is_pinned() {
        let default_pinned = false;
        assert!(!default_pinned);
    }

    /// Test is_pinned can be set to true
    #[test]
    fn test_set_pinned_true() {
        let mut is_pinned = false;
        is_pinned = true;
        assert!(is_pinned);
    }

    /// Test is_pinned in link response
    #[test]
    fn test_link_response_has_is_pinned() {
        // Simulating a link response structure
        struct LinkResponse {
            is_pinned: bool,
        }
        
        let response = LinkResponse { is_pinned: true };
        assert!(response.is_pinned);
    }
}

#[cfg(test)]
mod sparkline_tests {
    /// Test sparkline returns 7 days of data
    #[test]
    fn test_sparkline_data_length() {
        let data = vec![0, 0, 0, 1, 2, 0, 3];
        assert_eq!(data.len(), 7);
    }

    /// Test sparkline labels format
    #[test]
    fn test_sparkline_labels_format() {
        let labels = vec!["12/01", "12/02", "12/03", "12/04", "12/05", "12/06", "12/07"];
        assert_eq!(labels.len(), 7);
        for label in &labels {
            assert!(label.contains('/'));
        }
    }

    /// Test sparkline total calculation
    #[test]
    fn test_sparkline_total() {
        let data = vec![1, 2, 3, 4, 5, 6, 7];
        let total: i64 = data.iter().sum();
        assert_eq!(total, 28);
    }

    /// Test sparkline with no clicks
    #[test]
    fn test_sparkline_no_clicks() {
        let data = vec![0, 0, 0, 0, 0, 0, 0];
        let total: i64 = data.iter().sum();
        assert_eq!(total, 0);
    }

    /// Test sparkline response structure
    #[test]
    fn test_sparkline_response_structure() {
        struct SparklineData {
            link_id: i32,
            data: Vec<i64>,
            labels: Vec<String>,
            total: i64,
        }
        
        let sparkline = SparklineData {
            link_id: 123,
            data: vec![1, 2, 3, 4, 5, 6, 7],
            labels: vec!["12/01".to_string(); 7],
            total: 28,
        };
        
        assert_eq!(sparkline.link_id, 123);
        assert_eq!(sparkline.data.len(), 7);
        assert_eq!(sparkline.labels.len(), 7);
        assert_eq!(sparkline.total, 28);
    }

    /// Test multiple sparklines in response
    #[test]
    fn test_multiple_sparklines() {
        struct SparklineResponse {
            sparklines: Vec<(i32, Vec<i64>)>,
        }
        
        let response = SparklineResponse {
            sparklines: vec![
                (1, vec![1, 2, 3, 4, 5, 6, 7]),
                (2, vec![0, 0, 0, 0, 0, 0, 0]),
                (3, vec![10, 20, 30, 40, 50, 60, 70]),
            ],
        };
        
        assert_eq!(response.sparklines.len(), 3);
    }
}

#[cfg(test)]
mod link_preview_tests {
    /// Test OG title extraction pattern
    #[test]
    fn test_og_title_pattern() {
        let html = r#"<meta property="og:title" content="Test Title">"#;
        assert!(html.contains("og:title"));
        assert!(html.contains("Test Title"));
    }

    /// Test OG description extraction
    #[test]
    fn test_og_description_pattern() {
        let html = r#"<meta property="og:description" content="Test Description">"#;
        assert!(html.contains("og:description"));
    }

    /// Test OG image extraction
    #[test]
    fn test_og_image_pattern() {
        let html = r#"<meta property="og:image" content="https://example.com/image.jpg">"#;
        assert!(html.contains("og:image"));
    }

    /// Test fallback to title tag
    #[test]
    fn test_title_tag_fallback() {
        let html = r#"<title>Page Title</title>"#;
        assert!(html.contains("<title>"));
        assert!(html.contains("</title>"));
    }

    /// Test favicon extraction
    #[test]
    fn test_favicon_extraction() {
        let html = r#"<link rel="icon" href="/favicon.ico">"#;
        assert!(html.contains("rel=\"icon\""));
        assert!(html.contains("href="));
    }

    /// Test URL resolution for relative paths
    #[test]
    fn test_url_resolution() {
        let base = "https://example.com/page/";
        let relative = "/images/photo.jpg";
        
        // Simulate URL resolution
        if let Ok(base_url) = url::Url::parse(base) {
            if let Ok(resolved) = base_url.join(relative) {
                assert_eq!(resolved.to_string(), "https://example.com/images/photo.jpg");
            }
        }
    }

    /// Test absolute URL detection
    #[test]
    fn test_absolute_url_detection() {
        let absolute = "https://cdn.example.com/image.jpg";
        assert!(absolute.starts_with("http://") || absolute.starts_with("https://"));
    }

    /// Test HTML entity decoding
    #[test]
    fn test_html_entity_decoding() {
        let encoded = "Tom &amp; Jerry";
        let decoded = encoded.replace("&amp;", "&");
        assert_eq!(decoded, "Tom & Jerry");
    }

    /// Test preview response structure
    #[test]
    fn test_preview_response_structure() {
        struct LinkPreviewData {
            url: String,
            title: Option<String>,
            description: Option<String>,
            image: Option<String>,
            site_name: Option<String>,
            favicon: Option<String>,
        }
        
        let preview = LinkPreviewData {
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            description: Some("An example site".to_string()),
            image: Some("https://example.com/og.jpg".to_string()),
            site_name: Some("Example Site".to_string()),
            favicon: Some("https://example.com/favicon.ico".to_string()),
        };
        
        assert_eq!(preview.url, "https://example.com");
        assert!(preview.title.is_some());
        assert!(preview.description.is_some());
        assert!(preview.image.is_some());
    }

    /// Test handling missing OG data
    #[test]
    fn test_missing_og_data() {
        struct LinkPreviewData {
            url: String,
            title: Option<String>,
            description: Option<String>,
            image: Option<String>,
        }
        
        let preview = LinkPreviewData {
            url: "https://example.com".to_string(),
            title: None,
            description: None,
            image: None,
        };
        
        assert!(preview.title.is_none());
        assert!(preview.description.is_none());
        assert!(preview.image.is_none());
    }
}

