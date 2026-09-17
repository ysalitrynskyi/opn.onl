mod common;

#[cfg(test)]
mod tests {
    use super::common;
    use chrono::{Duration, Utc};
    use opn_onl_backend::entity::click_events;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
    use serde_json::{json, Value};

    // Real-router check (replaces the old stub that only hit a fake /health):
    // the analytics dashboard requires authentication.
    #[tokio::test]
    async fn test_analytics_endpoint_requires_auth() {
        let (server, _db) = common::spawn_real_app().await;

        let response = server.get("/analytics/dashboard").await;
        assert_eq!(
            response.status_code(),
            401,
            "unauthenticated /analytics/dashboard must be rejected: {}",
            response.text()
        );
    }

    async fn register_and_link(
        server: &axum_test::TestServer,
        db: &DatabaseConnection,
    ) -> (String, i32) {
        let res = server
            .post("/auth/register")
            .json(&json!({
                "email": common::unique_email(),
                "password": "password123"
            }))
            .await;
        assert_eq!(res.status_code(), 201, "register: {}", res.text());
        let body: Value = res.json();
        let token = body["token"].as_str().unwrap().to_string();
        let user_id = body["user_id"].as_i64().unwrap() as i32;
        common::mark_email_verified(db, user_id).await;

        let link = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": "https://iana.org/stats-window" }))
            .await;
        assert_eq!(link.status_code(), 201, "create link: {}", link.text());
        let link_id = link.json::<Value>()["id"].as_i64().unwrap() as i32;
        (token, link_id)
    }

    async fn insert_click(db: &DatabaseConnection, link_id: i32, days_ago: i64, country: &str) {
        click_events::ActiveModel {
            link_id: Set(link_id),
            created_at: Set(Utc::now().naive_utc() - Duration::days(days_ago)),
            country: Set(Some(country.to_string())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert click event");
    }

    async fn stats_clicks(
        server: &axum_test::TestServer,
        token: &str,
        link_id: i32,
        days: Option<&str>,
    ) -> (u16, i64) {
        let mut req = server
            .get(&format!("/links/{link_id}/stats"))
            .authorization_bearer(token);
        if let Some(days) = days {
            req = req.add_query_param("days", days);
        }
        let res = req.await;
        let status = res.status_code().as_u16();
        let total = res
            .json::<Value>()
            .get("total_clicks")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        (status, total)
    }

    /// `TimeDelta::days` panics on values outside roughly ±1.06e14. The stats
    /// handler used to pass the raw query param through, so one authenticated
    /// request could kill the worker. Out-of-range input must be clamped.
    #[tokio::test]
    async fn link_stats_clamps_out_of_range_days_instead_of_panicking() {
        let (server, db) = common::spawn_real_app().await;
        let (token, link_id) = register_and_link(&server, &db).await;

        insert_click(&db, link_id, 0, "NOW").await;
        insert_click(&db, link_id, 10, "D10").await;
        insert_click(&db, link_id, 80, "D80").await;
        insert_click(&db, link_id, 400, "D400").await;

        // Dashboard "Last 90 days" must keep working.
        let (status, total) = stats_clicks(&server, &token, link_id, Some("90")).await;
        assert_eq!(status, 200, "days=90 must succeed");
        assert_eq!(total, 3, "days=90 includes 0/10/80-day events, not 400");

        // Default window is 30 days.
        let (status, total) = stats_clicks(&server, &token, link_id, None).await;
        assert_eq!(status, 200, "omitted days must succeed");
        assert_eq!(total, 2, "default 30 days includes 0/10-day events, not 80");

        // Just-out-of-bounds for TimeDelta::days: used to panic.
        let (status, total) =
            stats_clicks(&server, &token, link_id, Some("106751991167301")).await;
        assert_eq!(status, 200, "huge days must be clamped, not panic");
        assert_eq!(
            total, 3,
            "clamped window is 366 days: 400-day-old event stays out"
        );

        // i64::MIN: also panics inside TimeDelta::days without a clamp.
        let (status, _) =
            stats_clicks(&server, &token, link_id, Some("-9223372036854775808")).await;
        assert_eq!(status, 200, "i64::MIN days must be clamped, not panic");
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

        let is_mobile =
            ua.to_lowercase().contains("mobile") || ua.to_lowercase().contains("iphone");

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

        let is_safari =
            ua.to_lowercase().contains("safari") && !ua.to_lowercase().contains("chrome");

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
            if ua_lower.contains("mobile")
                || ua_lower.contains("android") && !ua_lower.contains("tablet")
            {
                "Mobile"
            } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
                "Tablet"
            } else {
                "Desktop"
            }
        }

        assert_eq!(
            categorize_device("Mozilla/5.0 (iPhone; CPU iPhone OS) Mobile"),
            "Mobile"
        );
        assert_eq!(
            categorize_device("Mozilla/5.0 (iPad; CPU OS) Safari"),
            "Tablet"
        );
        assert_eq!(
            categorize_device("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            "Desktop"
        );
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
