mod common;

#[cfg(test)]
mod tests {
    use super::common;
    use chrono::{Duration, Utc};
    use opn_onl_backend::entity::click_events;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
    use serde_json::{Value, json};

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

    /// Click times leave the API as RFC 3339 UTC. The Analytics page reads
    /// them with `new Date`, which takes a time without an offset as local
    /// time, so recent clicks showed hours off for anyone outside UTC.
    #[tokio::test]
    async fn recent_click_timestamps_are_rfc3339_utc() {
        let (server, db) = common::spawn_real_app().await;
        let (token, link_id) = register_and_link(&server, &db).await;
        insert_click(&db, link_id, 0, "US").await;

        let stats: Value = server
            .get(&format!("/links/{link_id}/stats"))
            .authorization_bearer(&token)
            .await
            .json();
        let ts = stats["recent_clicks"][0]["timestamp"]
            .as_str()
            .unwrap_or_else(|| panic!("a recent click in {stats}"));
        assert!(ts.ends_with('Z'), "{ts}");
        let parsed = chrono::DateTime::parse_from_rfc3339(ts).expect("RFC 3339");
        let skew = Utc::now() - parsed.with_timezone(&Utc);
        assert!(
            skew.num_minutes().abs() < 5,
            "{ts} is not the click time in UTC"
        );
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
        let (status, total) = stats_clicks(&server, &token, link_id, Some("106751991167301")).await;
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

    /// `total_clicks` must be the window `COUNT(*)`, not the length of the
    /// capped breakdown sample. A link with more clicks than the row cap
    /// used to report the slice size as the total.
    #[tokio::test]
    async fn link_stats_total_clicks_is_window_count_not_truncated_slice() {
        use sea_orm::ConnectionTrait;

        let (server, db) = common::spawn_real_app().await;
        let (token, link_id) = register_and_link(&server, &db).await;

        const OVER_CAP: i64 = 50_001;
        db.execute_unprepared(&format!(
            "INSERT INTO click_events (link_id, created_at) \
             SELECT {link_id}, NOW() FROM generate_series(1, {OVER_CAP})"
        ))
        .await
        .expect("insert clicks over the stats row cap");

        let res = server
            .get(&format!("/links/{link_id}/stats"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(res.status_code(), 200, "stats: {}", res.text());
        let body: Value = res.json();
        assert_eq!(
            body["total_clicks"].as_i64(),
            Some(OVER_CAP),
            "total_clicks must be the window count, not the 50000-row sample: {body}"
        );
        assert_eq!(
            body["truncated"].as_bool(),
            Some(true),
            "window larger than the row cap must set truncated: {body}"
        );
    }

    fn bucket_count(arr: &Value, field: &str, name: &str) -> i64 {
        arr.as_array()
            .unwrap_or(&vec![])
            .iter()
            .find(|v| v[field].as_str() == Some(name))
            .and_then(|v| v["count"].as_i64())
            .unwrap_or(0)
    }

    fn bucket_pct(arr: &Value, field: &str, name: &str) -> f64 {
        arr.as_array()
            .unwrap_or(&vec![])
            .iter()
            .find(|v| v[field].as_str() == Some(name))
            .and_then(|v| v["percentage"].as_f64())
            .unwrap_or(-1.0)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_event(
        db: &DatabaseConnection,
        link_id: i32,
        days_ago: i64,
        country: Option<&str>,
        browser: Option<&str>,
        device: Option<&str>,
        ip: Option<&str>,
        referer: Option<&str>,
        lat: Option<f64>,
        lon: Option<f64>,
    ) {
        click_events::ActiveModel {
            link_id: Set(link_id),
            created_at: Set(Utc::now().naive_utc() - Duration::days(days_ago)),
            country: Set(country.map(str::to_string)),
            browser: Set(browser.map(str::to_string)),
            device: Set(device.map(str::to_string)),
            ip_address: Set(ip.map(str::to_string)),
            referer: Set(referer.map(str::to_string)),
            latitude: Set(lat),
            longitude: Set(lon),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert click event");
    }

    /// `/links/{id}/stats` aggregates the stored click rows: country/browser/
    /// device buckets (null → "Unknown"), unique visitors by IP, day series,
    /// referer host extraction (null → "Direct"), percentages, and geo
    /// clustering at two decimal places.
    #[tokio::test]
    async fn link_stats_aggregates_dimensions_unique_referer_and_geo() {
        let (server, db) = common::spawn_real_app().await;
        let (token, link_id) = register_and_link(&server, &db).await;

        let empty = server
            .get(&format!("/links/{link_id}/stats"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(empty.status_code(), 200, "empty stats: {}", empty.text());
        let empty_body: Value = empty.json();
        assert_eq!(empty_body["total_clicks"], 0);
        assert_eq!(empty_body["unique_visitors"], 0);
        assert!(
            empty_body["clicks_by_country"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(empty_body["geo_data"].as_array().unwrap().is_empty());

        // Two clicks from the same IP on nearby NYC coords (must cluster).
        insert_event(
            &db,
            link_id,
            0,
            Some("USA"),
            Some("Chrome"),
            Some("Desktop"),
            Some("1.1.1.1"),
            Some("https://www.google.com/search?q=test"),
            Some(40.7128),
            Some(-74.0060),
        )
        .await;
        insert_event(
            &db,
            link_id,
            1,
            Some("USA"),
            Some("Chrome"),
            Some("Desktop"),
            Some("1.1.1.1"),
            Some("https://www.google.com/other"),
            Some(40.7129),
            Some(-74.0061),
        )
        .await;
        // Different IP, UK, no referer → Direct.
        insert_event(
            &db,
            link_id,
            0,
            Some("UK"),
            Some("Firefox"),
            Some("Mobile"),
            Some("2.2.2.2"),
            None,
            Some(51.5074),
            Some(-0.1278),
        )
        .await;
        // Null dimensions → Unknown; no IP (does not count as a unique visitor); no geo.
        insert_event(&db, link_id, 0, None, None, None, None, None, None, None).await;

        let res = server
            .get(&format!("/links/{link_id}/stats"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(res.status_code(), 200, "stats: {}", res.text());
        let body: Value = res.json();

        assert_eq!(body["total_clicks"], 4);
        assert_eq!(
            body["unique_visitors"], 2,
            "same IP twice + one other IP; null IP is not a visitor: {body}"
        );

        let countries = &body["clicks_by_country"];
        assert_eq!(bucket_count(countries, "country", "USA"), 2);
        assert_eq!(bucket_count(countries, "country", "UK"), 1);
        assert_eq!(bucket_count(countries, "country", "Unknown"), 1);
        assert!((bucket_pct(countries, "country", "USA") - 50.0).abs() < 0.001);
        assert!((bucket_pct(countries, "country", "UK") - 25.0).abs() < 0.001);

        assert_eq!(
            bucket_count(&body["clicks_by_browser"], "browser", "Chrome"),
            2
        );
        assert_eq!(
            bucket_count(&body["clicks_by_browser"], "browser", "Firefox"),
            1
        );
        assert_eq!(
            bucket_count(&body["clicks_by_browser"], "browser", "Unknown"),
            1
        );
        assert_eq!(
            bucket_count(&body["clicks_by_device"], "device", "Desktop"),
            2
        );
        assert_eq!(
            bucket_count(&body["clicks_by_device"], "device", "Mobile"),
            1
        );
        assert_eq!(
            bucket_count(&body["clicks_by_device"], "device", "Unknown"),
            1
        );

        assert_eq!(
            bucket_count(&body["clicks_by_referer"], "referer", "www.google.com"),
            2,
            "full referer URL must be reduced to host, not a social-network label: {body}"
        );
        assert_eq!(
            bucket_count(&body["clicks_by_referer"], "referer", "Direct"),
            2,
            "null referer is Direct (the unknown-dimension event plus the UK click)"
        );

        let days = body["clicks_by_day"].as_array().unwrap();
        assert_eq!(days.len(), 2, "today + yesterday: {body}");
        let day_total: i64 = days.iter().filter_map(|d| d["count"].as_i64()).sum();
        assert_eq!(day_total, 4);

        let geo = body["geo_data"].as_array().unwrap();
        assert_eq!(
            geo.len(),
            2,
            "NYC pair must cluster at 2 decimal places; London is its own point: {body}"
        );
        let mut geo_counts: Vec<i64> = geo.iter().filter_map(|g| g["count"].as_i64()).collect();
        geo_counts.sort();
        assert_eq!(geo_counts, vec![1, 2]);
    }

    /// Dashboard totals come from the caller's links (not a global count):
    /// `total_links` includes inactive ones, `active_links` uses `is_active()`,
    /// `total_clicks` sums `links.click_count`, and the week/month windows
    /// are rolling 7/30-day UTC buckets. `clicks_today` is covered separately.
    #[tokio::test]
    async fn dashboard_counts_own_links_and_week_month_windows() {
        use opn_onl_backend::entity::links;
        use sea_orm::EntityTrait;

        let (server, db) = common::spawn_real_app().await;
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
        common::mark_email_verified(&db, user_id).await;

        let empty = server
            .get("/analytics/dashboard")
            .authorization_bearer(&token)
            .await;
        assert_eq!(empty.status_code(), 200, "empty dash: {}", empty.text());
        let empty_body: Value = empty.json();
        assert_eq!(empty_body["total_links"], 0);
        assert_eq!(empty_body["total_clicks"], 0);
        assert_eq!(empty_body["active_links"], 0);
        assert_eq!(empty_body["clicks_today"], 0);
        assert_eq!(empty_body["clicks_this_week"], 0);
        assert_eq!(empty_body["clicks_this_month"], 0);

        let live = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": "https://iana.org/dash-live" }))
            .await;
        assert_eq!(live.status_code(), 201, "live: {}", live.text());
        let live_id = live.json::<Value>()["id"].as_i64().unwrap() as i32;

        let dead = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": "https://iana.org/dash-expired" }))
            .await;
        assert_eq!(dead.status_code(), 201, "dead: {}", dead.text());
        let dead_id = dead.json::<Value>()["id"].as_i64().unwrap() as i32;

        let live_row = links::Entity::find_by_id(live_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut live_am: links::ActiveModel = live_row.into();
        live_am.click_count = Set(10);
        live_am.update(&db).await.unwrap();

        let dead_row = links::Entity::find_by_id(dead_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut dead_am: links::ActiveModel = dead_row.into();
        dead_am.click_count = Set(5);
        dead_am.expires_at = Set(Some(Utc::now().naive_utc() - Duration::hours(1)));
        dead_am.update(&db).await.unwrap();

        insert_click(&db, live_id, 0, "TODAY").await;
        insert_click(&db, live_id, 3, "WEEK").await;
        insert_click(&db, live_id, 20, "MONTH").await;

        let dash = server
            .get("/analytics/dashboard")
            .authorization_bearer(&token)
            .await;
        assert_eq!(dash.status_code(), 200, "dash: {}", dash.text());
        let body: Value = dash.json();
        assert_eq!(body["total_links"], 2, "expired links still count: {body}");
        assert_eq!(
            body["active_links"], 1,
            "expired link must drop out of active_links: {body}"
        );
        assert_eq!(
            body["total_clicks"], 15,
            "total_clicks is sum of links.click_count, not event rows: {body}"
        );
        assert_eq!(body["clicks_today"], 1, "{body}");
        assert_eq!(body["clicks_this_week"], 2, "today + 3 days ago: {body}");
        assert_eq!(
            body["clicks_this_month"], 3,
            "today + 3d + 20d (not a 40-day-old event): {body}"
        );
        assert_eq!(bucket_count(&body["top_countries"], "country", "TODAY"), 1);
    }

    /// A redirect stores what the analytics views aggregate: the browser,
    /// device and OS parsed from the visitor's User-Agent, and only the host
    /// of the Referer. The referring URL's path and query can carry search
    /// terms or session tokens, so they must never reach `click_events`.
    #[tokio::test]
    async fn redirect_records_parsed_user_agent_and_host_only_referer() {
        use axum::http::header::{REFERER, USER_AGENT};
        use opn_onl_backend::entity::links;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        const IPHONE_SAFARI: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
             AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

        let (server, db) = common::spawn_real_app().await;
        let (_token, link_id) = register_and_link(&server, &db).await;
        let code = links::Entity::find_by_id(link_id)
            .one(&db)
            .await
            .unwrap()
            .expect("link row")
            .code;

        // Same real router, but this test holds the ClickBuffer so it can
        // flush the recorded click instead of waiting for a background task.
        let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
        let clicks = state.click_buffer.clone();
        let visitor =
            axum_test::TestServer::new(opn_onl_backend::build_router(state)).expect("test server");

        let res = visitor
            .get(&format!("/{code}"))
            .add_header(USER_AGENT, IPHONE_SAFARI)
            .add_header(
                REFERER,
                "https://www.iana.org/search?q=private+terms&session=abc123",
            )
            .await;
        assert_eq!(res.status_code(), 307, "redirect: {}", res.text());
        assert!(
            !clicks.flush(&db).await,
            "the click must be persisted, not requeued"
        );

        let rows = click_events::Entity::find()
            .filter(click_events::Column::LinkId.eq(link_id))
            .all(&db)
            .await
            .expect("click rows");
        assert_eq!(rows.len(), 1, "one redirect, one click row: {rows:?}");
        let click = &rows[0];
        assert_eq!(click.browser.as_deref(), Some("Safari"), "{click:?}");
        assert_eq!(click.device.as_deref(), Some("Mobile"), "{click:?}");
        assert_eq!(click.os.as_deref(), Some("iOS"), "{click:?}");
        assert_eq!(
            click.referer.as_deref(),
            Some("www.iana.org"),
            "referer must be stored host-only, without path or query: {click:?}"
        );
    }
}
