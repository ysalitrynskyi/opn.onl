mod common;

// ============= WebSocket Event Tests =============

#[cfg(test)]
mod websocket_event_tests {
    use serde::{Deserialize, Serialize};
    use chrono::{NaiveDateTime, Utc};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(tag = "type")]
    enum WsEvent {
        Click(ClickEvent),
        LinkCreated(LinkEvent),
        LinkUpdated(LinkEvent),
        LinkDeleted(LinkDeletedEvent),
        Ping,
        Pong,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct ClickEvent {
        link_id: i32,
        link_code: String,
        click_count: i64,
        country: Option<String>,
        city: Option<String>,
        device: Option<String>,
        browser: Option<String>,
        timestamp: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct LinkEvent {
        link_id: i32,
        code: String,
        original_url: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct LinkDeletedEvent {
        link_id: i32,
        code: String,
    }

    #[test]
    fn test_click_event_serialization() {
        let event = WsEvent::Click(ClickEvent {
            link_id: 1,
            link_code: "abc123".to_string(),
            click_count: 42,
            country: Some("US".to_string()),
            city: Some("New York".to_string()),
            device: Some("Desktop".to_string()),
            browser: Some("Chrome".to_string()),
            timestamp: Utc::now().to_rfc3339(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Click"));
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_click_event_deserialization() {
        let json = r#"{"type":"Click","link_id":1,"link_code":"xyz789","click_count":100,"country":"UK","city":"London","device":"Mobile","browser":"Safari","timestamp":"2024-01-01T00:00:00Z"}"#;
        
        let event: WsEvent = serde_json::from_str(json).unwrap();
        match event {
            WsEvent::Click(click) => {
                assert_eq!(click.link_id, 1);
                assert_eq!(click.link_code, "xyz789");
                assert_eq!(click.click_count, 100);
            }
            _ => panic!("Expected Click event"),
        }
    }

    #[test]
    fn test_link_created_event() {
        let event = WsEvent::LinkCreated(LinkEvent {
            link_id: 5,
            code: "new123".to_string(),
            original_url: "https://example.com".to_string(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("LinkCreated"));
        assert!(json.contains("new123"));
    }

    #[test]
    fn test_link_updated_event() {
        let event = WsEvent::LinkUpdated(LinkEvent {
            link_id: 10,
            code: "upd456".to_string(),
            original_url: "https://updated.com".to_string(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("LinkUpdated"));
    }

    #[test]
    fn test_link_deleted_event() {
        let event = WsEvent::LinkDeleted(LinkDeletedEvent {
            link_id: 15,
            code: "del789".to_string(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("LinkDeleted"));
    }

    #[test]
    fn test_ping_pong_events() {
        let ping = WsEvent::Ping;
        let pong = WsEvent::Pong;

        let ping_json = serde_json::to_string(&ping).unwrap();
        let pong_json = serde_json::to_string(&pong).unwrap();

        assert!(ping_json.contains("Ping"));
        assert!(pong_json.contains("Pong"));
    }

    #[test]
    fn test_click_event_with_null_fields() {
        let event = WsEvent::Click(ClickEvent {
            link_id: 1,
            link_code: "abc123".to_string(),
            click_count: 1,
            country: None,
            city: None,
            device: None,
            browser: None,
            timestamp: Utc::now().to_rfc3339(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("null") || !json.contains("country"));
    }
}

// ============= WebSocket Connection State Tests =============

#[cfg(test)]
mod websocket_state_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    struct Connection {
        user_id: Option<i32>,
        subscribed_links: Vec<i32>,
    }

    struct WsState {
        connections: Arc<RwLock<HashMap<String, Connection>>>,
    }

    impl WsState {
        fn new() -> Self {
            Self {
                connections: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        async fn add_connection(&self, id: &str, user_id: Option<i32>) {
            let mut conns = self.connections.write().await;
            conns.insert(id.to_string(), Connection {
                user_id,
                subscribed_links: vec![],
            });
        }

        async fn remove_connection(&self, id: &str) {
            let mut conns = self.connections.write().await;
            conns.remove(id);
        }

        async fn connection_count(&self) -> usize {
            self.connections.read().await.len()
        }

        async fn subscribe_to_link(&self, conn_id: &str, link_id: i32) {
            let mut conns = self.connections.write().await;
            if let Some(conn) = conns.get_mut(conn_id) {
                if !conn.subscribed_links.contains(&link_id) {
                    conn.subscribed_links.push(link_id);
                }
            }
        }

        async fn get_subscribers(&self, link_id: i32) -> Vec<String> {
            let conns = self.connections.read().await;
            conns.iter()
                .filter(|(_, conn)| conn.subscribed_links.contains(&link_id))
                .map(|(id, _)| id.clone())
                .collect()
        }
    }

    #[tokio::test]
    async fn test_add_connection() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        
        assert_eq!(state.connection_count().await, 1);
    }

    #[tokio::test]
    async fn test_remove_connection() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        state.remove_connection("conn1").await;
        
        assert_eq!(state.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_multiple_connections() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        state.add_connection("conn2", Some(2)).await;
        state.add_connection("conn3", None).await;
        
        assert_eq!(state.connection_count().await, 3);
    }

    #[tokio::test]
    async fn test_subscribe_to_link() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        state.subscribe_to_link("conn1", 42).await;
        
        let subs = state.get_subscribers(42).await;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], "conn1");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        state.add_connection("conn2", Some(2)).await;
        
        state.subscribe_to_link("conn1", 42).await;
        state.subscribe_to_link("conn2", 42).await;
        
        let subs = state.get_subscribers(42).await;
        assert_eq!(subs.len(), 2);
    }

    #[tokio::test]
    async fn test_no_subscribers() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        state.subscribe_to_link("conn1", 42).await;
        
        let subs = state.get_subscribers(99).await;
        assert_eq!(subs.len(), 0);
    }

    #[tokio::test]
    async fn test_duplicate_subscription() {
        let state = WsState::new();
        
        state.add_connection("conn1", Some(1)).await;
        state.subscribe_to_link("conn1", 42).await;
        state.subscribe_to_link("conn1", 42).await;
        
        let subs = state.get_subscribers(42).await;
        assert_eq!(subs.len(), 1);
    }
}

// ============= WebSocket Message Broadcasting Tests =============

#[cfg(test)]
mod broadcast_tests {
    use std::collections::HashSet;

    struct BroadcastResult {
        sent_to: HashSet<String>,
        failed: HashSet<String>,
    }

    fn simulate_broadcast(
        recipients: &[&str],
        connected: &HashSet<&str>,
    ) -> BroadcastResult {
        let mut sent_to = HashSet::new();
        let mut failed = HashSet::new();

        for recipient in recipients {
            if connected.contains(recipient) {
                sent_to.insert(recipient.to_string());
            } else {
                failed.insert(recipient.to_string());
            }
        }

        BroadcastResult { sent_to, failed }
    }

    #[test]
    fn test_broadcast_to_all_connected() {
        let connected: HashSet<&str> = ["conn1", "conn2", "conn3"].iter().cloned().collect();
        let recipients = vec!["conn1", "conn2", "conn3"];

        let result = simulate_broadcast(&recipients, &connected);
        
        assert_eq!(result.sent_to.len(), 3);
        assert_eq!(result.failed.len(), 0);
    }

    #[test]
    fn test_broadcast_with_disconnected() {
        let connected: HashSet<&str> = ["conn1", "conn2"].iter().cloned().collect();
        let recipients = vec!["conn1", "conn2", "conn3"];

        let result = simulate_broadcast(&recipients, &connected);
        
        assert_eq!(result.sent_to.len(), 2);
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed.contains("conn3"));
    }

    #[test]
    fn test_broadcast_to_none() {
        let connected: HashSet<&str> = HashSet::new();
        let recipients = vec!["conn1", "conn2"];

        let result = simulate_broadcast(&recipients, &connected);
        
        assert_eq!(result.sent_to.len(), 0);
        assert_eq!(result.failed.len(), 2);
    }

    #[test]
    fn test_empty_broadcast() {
        let connected: HashSet<&str> = ["conn1"].iter().cloned().collect();
        let recipients: Vec<&str> = vec![];

        let result = simulate_broadcast(&recipients, &connected);
        
        assert_eq!(result.sent_to.len(), 0);
        assert_eq!(result.failed.len(), 0);
    }
}

// ============= WebSocket Rate Limiting Tests =============

#[cfg(test)]
mod ws_rate_limit_tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    struct WsRateLimiter {
        limits: HashMap<String, (u32, Instant)>,
        max_messages: u32,
        window: Duration,
    }

    impl WsRateLimiter {
        fn new(max_messages: u32, window_secs: u64) -> Self {
            Self {
                limits: HashMap::new(),
                max_messages,
                window: Duration::from_secs(window_secs),
            }
        }

        fn allow(&mut self, conn_id: &str) -> bool {
            let now = Instant::now();
            
            if let Some((count, start)) = self.limits.get_mut(conn_id) {
                if now.duration_since(*start) >= self.window {
                    *count = 1;
                    *start = now;
                    return true;
                }
                
                if *count >= self.max_messages {
                    return false;
                }
                
                *count += 1;
                true
            } else {
                self.limits.insert(conn_id.to_string(), (1, now));
                true
            }
        }
    }

    #[test]
    fn test_allows_under_limit() {
        let mut limiter = WsRateLimiter::new(10, 60);
        
        for _ in 0..10 {
            assert!(limiter.allow("conn1"));
        }
    }

    #[test]
    fn test_blocks_over_limit() {
        let mut limiter = WsRateLimiter::new(5, 60);
        
        for _ in 0..5 {
            assert!(limiter.allow("conn1"));
        }
        
        assert!(!limiter.allow("conn1"));
    }

    #[test]
    fn test_separate_connections_independent() {
        let mut limiter = WsRateLimiter::new(2, 60);
        
        assert!(limiter.allow("conn1"));
        assert!(limiter.allow("conn1"));
        assert!(!limiter.allow("conn1"));
        
        assert!(limiter.allow("conn2"));
    }
}

// ============= WebSocket Authentication Tests =============

#[cfg(test)]
mod ws_auth_tests {
    fn validate_ws_token(token: &str) -> Result<i32, &'static str> {
        // Simplified token validation
        if token.is_empty() {
            return Err("Empty token");
        }
        if token == "valid-token" {
            return Ok(1);
        }
        if token.starts_with("user-") {
            if let Ok(id) = token.strip_prefix("user-").unwrap().parse::<i32>() {
                return Ok(id);
            }
        }
        Err("Invalid token")
    }

    #[test]
    fn test_valid_token() {
        let result = validate_ws_token("valid-token");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_user_token() {
        let result = validate_ws_token("user-42");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_empty_token() {
        let result = validate_ws_token("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty token");
    }

    #[test]
    fn test_invalid_token() {
        let result = validate_ws_token("invalid");
        assert!(result.is_err());
    }
}
