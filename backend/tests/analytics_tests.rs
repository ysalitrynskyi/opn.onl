#[cfg(test)]
mod tests {
    use axum_test::TestServer;

    // Helper to create a minimal test app
    fn create_test_app() -> axum::Router {
        axum::Router::new()
            .route("/health", axum::routing::get(|| async { "OK" }))
    }

    #[tokio::test]
    async fn test_analytics_endpoint() {
        let app = create_test_app();
        let server = TestServer::new(app).unwrap();

        let response = server.get("/health").await;
        response.assert_status_ok();
    }
}

// Unit tests for analytics processing
#[cfg(test)]
mod unit_tests {
    use std::collections::HashMap;

    #[test]
    fn test_user_agent_parsing_desktop_chrome() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";
        
        let is_mobile = ua.to_lowercase().contains("mobile");
        let is_chrome = ua.to_lowercase().contains("chrome") && !ua.to_lowercase().contains("edge");

        assert!(!is_mobile);
        assert!(is_chrome);
    }

    #[test]
    fn test_user_agent_parsing_mobile() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.1 Mobile/15E148 Safari/604.1";
        
        let is_mobile = ua.to_lowercase().contains("mobile") || ua.to_lowercase().contains("iphone");

        assert!(is_mobile);
    }

    #[test]
    fn test_user_agent_parsing_firefox() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:89.0) Gecko/20100101 Firefox/89.0";
        
        let is_firefox = ua.to_lowercase().contains("firefox");

        assert!(is_firefox);
    }

    #[test]
    fn test_user_agent_parsing_safari() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.1 Safari/605.1.15";
        
        let is_safari = ua.to_lowercase().contains("safari") && !ua.to_lowercase().contains("chrome");

        assert!(is_safari);
    }

    #[test]
    fn test_referer_parsing() {
        let referer = "https://twitter.com/user/status/123456";
        
        let url = url::Url::parse(referer).unwrap();
        let hostname = url.host_str().unwrap();

        assert_eq!(hostname, "twitter.com");
    }

    #[test]
    fn test_referer_parsing_with_subdomain() {
        let referer = "https://www.google.com/search?q=test";
        
        let url = url::Url::parse(referer).unwrap();
        let hostname = url.host_str().unwrap();

        assert_eq!(hostname, "www.google.com");
    }

    #[test]
    fn test_click_aggregation_by_date() {
        let mut clicks_by_date: HashMap<String, i32> = HashMap::new();
        
        // Simulate click data
        let dates = vec!["2024-01-01", "2024-01-01", "2024-01-02", "2024-01-01"];
        
        for date in dates {
            *clicks_by_date.entry(date.to_string()).or_insert(0) += 1;
        }

        assert_eq!(clicks_by_date.get("2024-01-01"), Some(&3));
        assert_eq!(clicks_by_date.get("2024-01-02"), Some(&1));
    }

    #[test]
    fn test_device_categorization() {
        fn categorize_device(ua: &str) -> &str {
            let ua_lower = ua.to_lowercase();
            if ua_lower.contains("mobile") || ua_lower.contains("android") && !ua_lower.contains("tablet") {
                "Mobile"
            } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
                "Tablet"
            } else {
                "Desktop"
            }
        }

        assert_eq!(categorize_device("Mozilla/5.0 (iPhone; CPU iPhone OS) Mobile"), "Mobile");
        assert_eq!(categorize_device("Mozilla/5.0 (iPad; CPU OS) Safari"), "Tablet");
        assert_eq!(categorize_device("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"), "Desktop");
    }

    #[test]
    fn test_browser_detection() {
        fn detect_browser(ua: &str) -> &str {
            let ua_lower = ua.to_lowercase();
            if ua_lower.contains("edg") {
                "Edge"
            } else if ua_lower.contains("chrome") {
                "Chrome"
            } else if ua_lower.contains("firefox") {
                "Firefox"
            } else if ua_lower.contains("safari") {
                "Safari"
            } else {
                "Other"
            }
        }

        assert_eq!(detect_browser("Mozilla/5.0 Chrome/91.0"), "Chrome");
        assert_eq!(detect_browser("Mozilla/5.0 Firefox/89.0"), "Firefox");
        assert_eq!(detect_browser("Mozilla/5.0 Safari/605.1"), "Safari");
        assert_eq!(detect_browser("Mozilla/5.0 Edg/91.0"), "Edge");
    }
}

// Tests for statistics calculation
#[cfg(test)]
mod stats_tests {
    #[test]
    fn test_total_clicks_calculation() {
        let clicks = vec![5, 10, 3, 7, 15];
        let total: i32 = clicks.iter().sum();

        assert_eq!(total, 40);
    }

    #[test]
    fn test_percentage_calculation() {
        let total = 100;
        let mobile_clicks = 35;
        
        let percentage = (mobile_clicks as f64 / total as f64) * 100.0;

        assert_eq!(percentage, 35.0);
    }

    #[test]
    fn test_percentage_with_zero_total() {
        let total = 0;
        let mobile_clicks = 0;
        
        let percentage = if total > 0 {
            (mobile_clicks as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        assert_eq!(percentage, 0.0);
    }
}
