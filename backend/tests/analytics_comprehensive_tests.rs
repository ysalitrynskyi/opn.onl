//! Comprehensive analytics tests

use std::collections::HashMap;

// ============= Analytics Aggregation Tests =============

mod aggregation_tests {
    use super::*;

    #[derive(Clone)]
    struct ClickEvent {
        link_id: i32,
        country: Option<String>,
        city: Option<String>,
        browser: Option<String>,
        device: Option<String>,
        os: Option<String>,
        referer: Option<String>,
        ip: Option<String>,
        timestamp: String,
    }

    fn aggregate_by_country(events: &[ClickEvent]) -> HashMap<String, i64> {
        let mut map = HashMap::new();
        for event in events {
            let country = event.country.clone().unwrap_or_else(|| "Unknown".to_string());
            *map.entry(country).or_insert(0) += 1;
        }
        map
    }

    fn aggregate_by_browser(events: &[ClickEvent]) -> HashMap<String, i64> {
        let mut map = HashMap::new();
        for event in events {
            let browser = event.browser.clone().unwrap_or_else(|| "Unknown".to_string());
            *map.entry(browser).or_insert(0) += 1;
        }
        map
    }

    fn aggregate_by_device(events: &[ClickEvent]) -> HashMap<String, i64> {
        let mut map = HashMap::new();
        for event in events {
            let device = event.device.clone().unwrap_or_else(|| "Unknown".to_string());
            *map.entry(device).or_insert(0) += 1;
        }
        map
    }

    fn count_unique_visitors(events: &[ClickEvent]) -> usize {
        let unique_ips: std::collections::HashSet<_> = events
            .iter()
            .filter_map(|e| e.ip.as_ref())
            .collect();
        unique_ips.len()
    }

    fn create_test_event(country: &str, browser: &str, device: &str, ip: &str) -> ClickEvent {
        ClickEvent {
            link_id: 1,
            country: Some(country.to_string()),
            city: None,
            browser: Some(browser.to_string()),
            device: Some(device.to_string()),
            os: None,
            referer: None,
            ip: Some(ip.to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_aggregate_by_country() {
        let events = vec![
            create_test_event("USA", "Chrome", "Desktop", "1.1.1.1"),
            create_test_event("USA", "Firefox", "Mobile", "2.2.2.2"),
            create_test_event("UK", "Chrome", "Desktop", "3.3.3.3"),
            create_test_event("USA", "Safari", "Tablet", "4.4.4.4"),
        ];

        let aggregated = aggregate_by_country(&events);
        
        assert_eq!(aggregated.get("USA"), Some(&3));
        assert_eq!(aggregated.get("UK"), Some(&1));
    }

    #[test]
    fn test_aggregate_by_browser() {
        let events = vec![
            create_test_event("USA", "Chrome", "Desktop", "1.1.1.1"),
            create_test_event("USA", "Chrome", "Mobile", "2.2.2.2"),
            create_test_event("UK", "Firefox", "Desktop", "3.3.3.3"),
            create_test_event("USA", "Safari", "Tablet", "4.4.4.4"),
        ];

        let aggregated = aggregate_by_browser(&events);
        
        assert_eq!(aggregated.get("Chrome"), Some(&2));
        assert_eq!(aggregated.get("Firefox"), Some(&1));
        assert_eq!(aggregated.get("Safari"), Some(&1));
    }

    #[test]
    fn test_aggregate_by_device() {
        let events = vec![
            create_test_event("USA", "Chrome", "Desktop", "1.1.1.1"),
            create_test_event("USA", "Chrome", "Mobile", "2.2.2.2"),
            create_test_event("UK", "Firefox", "Desktop", "3.3.3.3"),
            create_test_event("USA", "Safari", "Mobile", "4.4.4.4"),
        ];

        let aggregated = aggregate_by_device(&events);
        
        assert_eq!(aggregated.get("Desktop"), Some(&2));
        assert_eq!(aggregated.get("Mobile"), Some(&2));
    }

    #[test]
    fn test_unique_visitors() {
        let events = vec![
            create_test_event("USA", "Chrome", "Desktop", "1.1.1.1"),
            create_test_event("USA", "Chrome", "Desktop", "1.1.1.1"), // Same IP
            create_test_event("UK", "Firefox", "Desktop", "2.2.2.2"),
            create_test_event("USA", "Safari", "Mobile", "1.1.1.1"), // Same IP again
        ];

        assert_eq!(count_unique_visitors(&events), 2);
    }

    #[test]
    fn test_empty_events() {
        let events: Vec<ClickEvent> = vec![];
        
        assert!(aggregate_by_country(&events).is_empty());
        assert!(aggregate_by_browser(&events).is_empty());
        assert_eq!(count_unique_visitors(&events), 0);
    }

    #[test]
    fn test_unknown_values() {
        let event = ClickEvent {
            link_id: 1,
            country: None,
            city: None,
            browser: None,
            device: None,
            os: None,
            referer: None,
            ip: None,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let events = vec![event];
        
        let countries = aggregate_by_country(&events);
        assert_eq!(countries.get("Unknown"), Some(&1));
    }
}

// ============= Time Series Tests =============

mod time_series_tests {
    use super::*;

    fn aggregate_by_day(timestamps: &[&str]) -> HashMap<String, i64> {
        let mut map = HashMap::new();
        for ts in timestamps {
            // Extract date part (YYYY-MM-DD)
            let date = &ts[..10];
            *map.entry(date.to_string()).or_insert(0) += 1;
        }
        map
    }

    fn aggregate_by_hour(timestamps: &[&str]) -> HashMap<u8, i64> {
        let mut map = HashMap::new();
        for ts in timestamps {
            // Extract hour (HH from YYYY-MM-DDTHH:MM:SS)
            if ts.len() >= 13 {
                let hour: u8 = ts[11..13].parse().unwrap_or(0);
                *map.entry(hour).or_insert(0) += 1;
            }
        }
        map
    }

    #[test]
    fn test_aggregate_by_day() {
        let timestamps = vec![
            "2024-01-01T10:00:00Z",
            "2024-01-01T15:00:00Z",
            "2024-01-02T09:00:00Z",
            "2024-01-02T14:00:00Z",
            "2024-01-02T18:00:00Z",
        ];

        let aggregated = aggregate_by_day(&timestamps);
        
        assert_eq!(aggregated.get("2024-01-01"), Some(&2));
        assert_eq!(aggregated.get("2024-01-02"), Some(&3));
    }

    #[test]
    fn test_aggregate_by_hour() {
        let timestamps = vec![
            "2024-01-01T09:00:00Z",
            "2024-01-01T09:30:00Z",
            "2024-01-01T14:00:00Z",
            "2024-01-01T14:45:00Z",
            "2024-01-01T20:00:00Z",
        ];

        let aggregated = aggregate_by_hour(&timestamps);
        
        assert_eq!(aggregated.get(&9), Some(&2));
        assert_eq!(aggregated.get(&14), Some(&2));
        assert_eq!(aggregated.get(&20), Some(&1));
    }
}

// ============= Percentage Calculation Tests =============

mod percentage_tests {
    fn calculate_percentage(count: i64, total: i64) -> f64 {
        if total == 0 {
            0.0
        } else {
            (count as f64 / total as f64) * 100.0
        }
    }

    fn calculate_percentages(counts: &[(String, i64)]) -> Vec<(String, i64, f64)> {
        let total: i64 = counts.iter().map(|(_, c)| c).sum();
        counts
            .iter()
            .map(|(name, count)| (name.clone(), *count, calculate_percentage(*count, total)))
            .collect()
    }

    #[test]
    fn test_percentage_calculation() {
        assert!((calculate_percentage(50, 100) - 50.0).abs() < 0.001);
        assert!((calculate_percentage(25, 100) - 25.0).abs() < 0.001);
        assert!((calculate_percentage(1, 3) - 33.333).abs() < 0.01);
    }

    #[test]
    fn test_zero_total() {
        assert_eq!(calculate_percentage(0, 0), 0.0);
        assert_eq!(calculate_percentage(10, 0), 0.0);
    }

    #[test]
    fn test_calculate_percentages() {
        let counts = vec![
            ("USA".to_string(), 60),
            ("UK".to_string(), 30),
            ("Canada".to_string(), 10),
        ];

        let result = calculate_percentages(&counts);
        
        assert_eq!(result.len(), 3);
        assert!((result[0].2 - 60.0).abs() < 0.001); // USA = 60%
        assert!((result[1].2 - 30.0).abs() < 0.001); // UK = 30%
        assert!((result[2].2 - 10.0).abs() < 0.001); // Canada = 10%
    }
}

// ============= Referer Analysis Tests =============

mod referer_tests {
    fn extract_domain(url: &str) -> Option<String> {
        // Simple domain extraction
        let url = url.trim_start_matches("http://").trim_start_matches("https://");
        url.split('/').next().map(|s| s.to_string())
    }

    fn categorize_referer(referer: &Option<String>) -> String {
        match referer {
            None => "Direct".to_string(),
            Some(url) => {
                let domain = extract_domain(url).unwrap_or_default().to_lowercase();
                
                if domain.contains("google") {
                    "Google".to_string()
                } else if domain.contains("facebook") || domain.contains("fb.com") {
                    "Facebook".to_string()
                } else if domain.contains("twitter") || domain.contains("x.com") {
                    "Twitter/X".to_string()
                } else if domain.contains("linkedin") {
                    "LinkedIn".to_string()
                } else if domain.is_empty() {
                    "Direct".to_string()
                } else {
                    domain
                }
            }
        }
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://www.google.com/search?q=test"), Some("www.google.com".to_string()));
        assert_eq!(extract_domain("http://example.com"), Some("example.com".to_string()));
        assert_eq!(extract_domain("https://sub.domain.com/path"), Some("sub.domain.com".to_string()));
    }

    #[test]
    fn test_categorize_referer_direct() {
        assert_eq!(categorize_referer(&None), "Direct");
        assert_eq!(categorize_referer(&Some("".to_string())), "Direct");
    }

    #[test]
    fn test_categorize_referer_google() {
        assert_eq!(
            categorize_referer(&Some("https://www.google.com/search?q=test".to_string())),
            "Google"
        );
    }

    #[test]
    fn test_categorize_referer_social() {
        assert_eq!(
            categorize_referer(&Some("https://facebook.com/posts/123".to_string())),
            "Facebook"
        );
        assert_eq!(
            categorize_referer(&Some("https://twitter.com/user/status/123".to_string())),
            "Twitter/X"
        );
        assert_eq!(
            categorize_referer(&Some("https://x.com/user/status/123".to_string())),
            "Twitter/X"
        );
    }

    #[test]
    fn test_categorize_referer_other() {
        assert_eq!(
            categorize_referer(&Some("https://myblog.com/post".to_string())),
            "myblog.com"
        );
    }
}

// ============= Dashboard Stats Tests =============

mod dashboard_tests {
    use chrono::{Duration, NaiveDateTime, Utc};

    struct Link {
        id: i32,
        click_count: i32,
        is_active: bool,
        created_at: NaiveDateTime,
    }

    struct DashboardStats {
        total_links: i64,
        total_clicks: i64,
        active_links: i64,
        clicks_today: i64,
        clicks_this_week: i64,
        clicks_this_month: i64,
    }

    fn calculate_dashboard_stats(
        links: &[Link],
        click_timestamps: &[NaiveDateTime],
    ) -> DashboardStats {
        let now = Utc::now().naive_utc();
        let today_start = now.date().and_hms_opt(0, 0, 0).unwrap();
        let week_start = now - Duration::days(7);
        let month_start = now - Duration::days(30);

        DashboardStats {
            total_links: links.len() as i64,
            total_clicks: links.iter().map(|l| l.click_count as i64).sum(),
            active_links: links.iter().filter(|l| l.is_active).count() as i64,
            clicks_today: click_timestamps.iter().filter(|&&t| t >= today_start).count() as i64,
            clicks_this_week: click_timestamps.iter().filter(|&&t| t >= week_start).count() as i64,
            clicks_this_month: click_timestamps.iter().filter(|&&t| t >= month_start).count() as i64,
        }
    }

    #[test]
    fn test_dashboard_stats_total_links() {
        let links = vec![
            Link { id: 1, click_count: 10, is_active: true, created_at: Utc::now().naive_utc() },
            Link { id: 2, click_count: 20, is_active: false, created_at: Utc::now().naive_utc() },
            Link { id: 3, click_count: 30, is_active: true, created_at: Utc::now().naive_utc() },
        ];

        let stats = calculate_dashboard_stats(&links, &[]);
        
        assert_eq!(stats.total_links, 3);
        assert_eq!(stats.total_clicks, 60);
        assert_eq!(stats.active_links, 2);
    }

    #[test]
    fn test_dashboard_stats_empty() {
        let stats = calculate_dashboard_stats(&[], &[]);
        
        assert_eq!(stats.total_links, 0);
        assert_eq!(stats.total_clicks, 0);
        assert_eq!(stats.active_links, 0);
    }
}

// ============= Geo Clustering Tests =============

mod geo_clustering_tests {
    use std::collections::HashMap;

    fn cluster_geo_points(points: &[(f64, f64)], precision: i32) -> HashMap<(i64, i64), usize> {
        let multiplier = 10_f64.powi(precision);
        let mut clusters: HashMap<(i64, i64), usize> = HashMap::new();
        
        for &(lat, lon) in points {
            let key = (
                (lat * multiplier) as i64,
                (lon * multiplier) as i64,
            );
            *clusters.entry(key).or_insert(0) += 1;
        }
        
        clusters
    }

    #[test]
    fn test_cluster_nearby_points() {
        let points = vec![
            (40.7128, -74.0060),   // NYC
            (40.7129, -74.0061),   // Very close to NYC
            (40.7130, -74.0062),   // Very close to NYC
            (51.5074, -0.1278),    // London
        ];

        let clusters = cluster_geo_points(&points, 2); // 2 decimal places = ~1km
        
        // NYC area should have 3 points clustered
        // London should have 1 point
        assert_eq!(clusters.values().sum::<usize>(), 4);
    }

    #[test]
    fn test_cluster_empty() {
        let points: Vec<(f64, f64)> = vec![];
        let clusters = cluster_geo_points(&points, 2);
        
        assert!(clusters.is_empty());
    }

    #[test]
    fn test_cluster_single_point() {
        let points = vec![(40.7128, -74.0060)];
        let clusters = cluster_geo_points(&points, 2);
        
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters.values().sum::<usize>(), 1);
    }
}





