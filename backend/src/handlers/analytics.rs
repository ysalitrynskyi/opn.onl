use axum::{
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap},
    Json,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sea_orm::*;
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{links, click_events};
use crate::handlers::links::get_user_id_from_header;

// ============= DTOs =============

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct AnalyticsQuery {
    pub days: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct LinkStatsResponse {
    pub link_id: i32,
    pub code: String,
    pub original_url: String,
    pub total_clicks: i32,
    pub unique_visitors: i32,
    pub clicks_by_day: Vec<DayStats>,
    pub clicks_by_country: Vec<CountryStats>,
    pub clicks_by_city: Vec<CityStats>,
    pub clicks_by_device: Vec<DeviceStats>,
    pub clicks_by_browser: Vec<BrowserStats>,
    pub clicks_by_os: Vec<OsStats>,
    pub clicks_by_referer: Vec<RefererStats>,
    pub recent_clicks: Vec<RecentClick>,
    pub geo_data: Vec<GeoPoint>,
}

#[derive(Serialize, ToSchema)]
pub struct DayStats {
    pub date: String,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct CountryStats {
    pub country: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Serialize, ToSchema)]
pub struct CityStats {
    pub city: String,
    pub country: Option<String>,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Serialize, ToSchema)]
pub struct DeviceStats {
    pub device: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Serialize, ToSchema)]
pub struct BrowserStats {
    pub browser: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Serialize, ToSchema)]
pub struct OsStats {
    pub os: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Serialize, ToSchema)]
pub struct RefererStats {
    pub referer: String,
    pub count: i64,
    pub percentage: f64,
}

#[derive(Serialize, ToSchema)]
pub struct RecentClick {
    pub id: i32,
    pub timestamp: String,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub referer: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
    pub city: Option<String>,
    pub country: Option<String>,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardStats {
    pub total_links: i64,
    pub total_clicks: i64,
    pub active_links: i64,
    pub clicks_today: i64,
    pub clicks_this_week: i64,
    pub clicks_this_month: i64,
    pub top_links: Vec<TopLink>,
    pub clicks_by_day: Vec<DayStats>,
    pub top_countries: Vec<CountryStats>,
    pub top_browsers: Vec<BrowserStats>,
}

#[derive(Serialize, ToSchema)]
pub struct TopLink {
    pub id: i32,
    pub code: String,
    pub original_url: String,
    pub click_count: i32,
}

// ============= Handlers =============

/// Get detailed stats for a specific link
#[utoipa::path(
    get,
    path = "/links/{id}/stats",
    params(
        ("id" = i32, Path, description = "Link ID"),
        AnalyticsQuery
    ),
    responses(
        (status = 200, description = "Link statistics", body = LinkStatsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found"),
    ),
    tag = "Analytics"
)]
pub async fn get_link_stats(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Query(query): Query<AnalyticsQuery>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    let link = match links::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(link)) => link,
        _ => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Link not found"}))).into_response(),
    };

    // Check ownership
    if link.user_id != Some(user_id) && link.org_id.is_none() {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response();
    }

    // Get time range
    let days = query.days.unwrap_or(30);
    let start_date = chrono::Utc::now().naive_utc() - chrono::Duration::days(days);

    // Fetch click events
    let events = click_events::Entity::find()
        .filter(click_events::Column::LinkId.eq(id))
        .filter(click_events::Column::CreatedAt.gte(start_date))
        .order_by_desc(click_events::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let total_clicks = events.len() as i32;
    // Prevent division by zero - use 1 as minimum for percentage calculations
    let total_for_percentage = total_clicks.max(1) as f64;

    // Unique visitors (by IP)
    let unique_ips: std::collections::HashSet<_> = events.iter()
        .filter_map(|e| e.ip_address.clone())
        .collect();
    let unique_visitors = unique_ips.len() as i32;

    // Clicks by day
    let mut clicks_by_day_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let date = event.created_at.format("%Y-%m-%d").to_string();
        *clicks_by_day_map.entry(date).or_insert(0) += 1;
    }
    let mut clicks_by_day: Vec<DayStats> = clicks_by_day_map.into_iter()
        .map(|(date, count)| DayStats { date, count })
        .collect();
    clicks_by_day.sort_by(|a, b| a.date.cmp(&b.date));

    // Clicks by country
    let mut country_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let country = event.country.clone().unwrap_or_else(|| "Unknown".to_string());
        *country_map.entry(country).or_insert(0) += 1;
    }
    let clicks_by_country: Vec<CountryStats> = country_map.into_iter()
        .map(|(country, count)| CountryStats {
            country,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by city
    let mut city_map: HashMap<String, (i64, Option<String>)> = HashMap::new();
    for event in &events {
        let city = event.city.clone().unwrap_or_else(|| "Unknown".to_string());
        let entry = city_map.entry(city).or_insert((0, event.country.clone()));
        entry.0 += 1;
    }
    let clicks_by_city: Vec<CityStats> = city_map.into_iter()
        .map(|(city, (count, country))| CityStats {
            city,
            country,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by device
    let mut device_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let device = event.device.clone().unwrap_or_else(|| "Unknown".to_string());
        *device_map.entry(device).or_insert(0) += 1;
    }
    let clicks_by_device: Vec<DeviceStats> = device_map.into_iter()
        .map(|(device, count)| DeviceStats {
            device,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by browser
    let mut browser_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let browser = event.browser.clone().unwrap_or_else(|| "Unknown".to_string());
        *browser_map.entry(browser).or_insert(0) += 1;
    }
    let clicks_by_browser: Vec<BrowserStats> = browser_map.into_iter()
        .map(|(browser, count)| BrowserStats {
            browser,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by OS
    let mut os_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let os = event.os.clone().unwrap_or_else(|| "Unknown".to_string());
        *os_map.entry(os).or_insert(0) += 1;
    }
    let clicks_by_os: Vec<OsStats> = os_map.into_iter()
        .map(|(os, count)| OsStats {
            os,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by referer
    let mut referer_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let referer = event.referer.clone()
            .map(|r| extract_domain(&r).unwrap_or_else(|| r.clone()))
            .unwrap_or_else(|| "Direct".to_string());
        *referer_map.entry(referer).or_insert(0) += 1;
    }
    let clicks_by_referer: Vec<RefererStats> = referer_map.into_iter()
        .map(|(referer, count)| RefererStats {
            referer,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Recent clicks (last 100)
    let recent_clicks: Vec<RecentClick> = events.iter().take(100).map(|e| RecentClick {
        id: e.id,
        timestamp: e.created_at.to_string(),
        country: e.country.clone(),
        city: e.city.clone(),
        device: e.device.clone(),
        browser: e.browser.clone(),
        os: e.os.clone(),
        referer: e.referer.clone(),
    }).collect();

    // Geo data for map
    let mut geo_map: HashMap<(i64, i64), (f64, f64, Option<String>, Option<String>, i64)> = HashMap::new();
    for event in &events {
        if let (Some(lat), Some(lon)) = (event.latitude, event.longitude) {
            // Round to 2 decimal places for clustering
            let key = ((lat * 100.0) as i64, (lon * 100.0) as i64);
            let entry = geo_map.entry(key).or_insert((lat, lon, event.city.clone(), event.country.clone(), 0));
            entry.4 += 1;
        }
    }
    let geo_data: Vec<GeoPoint> = geo_map.into_values()
        .map(|(lat, lon, city, country, count)| GeoPoint {
            latitude: lat,
            longitude: lon,
            city,
            country,
            count,
        })
        .collect();

    let response = LinkStatsResponse {
        link_id: id,
        code: link.code,
        original_url: link.original_url,
        total_clicks,
        unique_visitors,
        clicks_by_day,
        clicks_by_country,
        clicks_by_city,
        clicks_by_device,
        clicks_by_browser,
        clicks_by_os,
        clicks_by_referer,
        recent_clicks,
        geo_data,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Get dashboard analytics
#[utoipa::path(
    get,
    path = "/analytics/dashboard",
    responses(
        (status = 200, description = "Dashboard statistics", body = DashboardStats),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "Analytics"
)]
pub async fn get_dashboard_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    // Get user's links
    let user_links = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .all(&state.db)
        .await
        .unwrap_or_default();

    let total_links = user_links.len() as i64;
    let total_clicks: i64 = user_links.iter().map(|l| l.click_count as i64).sum();
    let active_links = user_links.iter().filter(|l| l.is_active()).count() as i64;

    let link_ids: Vec<i32> = user_links.iter().map(|l| l.id).collect();

    // Get time boundaries
    let now = chrono::Utc::now().naive_utc();
    let today_start = now.date().and_hms_opt(0, 0, 0).unwrap();
    let week_start = now - chrono::Duration::days(7);
    let month_start = now - chrono::Duration::days(30);

    // Get all clicks in the last 30 days
    let events = click_events::Entity::find()
        .filter(click_events::Column::LinkId.is_in(link_ids))
        .filter(click_events::Column::CreatedAt.gte(month_start))
        .all(&state.db)
        .await
        .unwrap_or_default();

    // Calculate time-based stats
    let clicks_today = events.iter().filter(|e| e.created_at >= today_start).count() as i64;
    let clicks_this_week = events.iter().filter(|e| e.created_at >= week_start).count() as i64;
    let clicks_this_month = events.len() as i64;

    // Top links
    let mut top_links: Vec<TopLink> = user_links.iter()
        .map(|l| TopLink {
            id: l.id,
            code: l.code.clone(),
            original_url: l.original_url.clone(),
            click_count: l.click_count,
        })
        .collect();
    top_links.sort_by(|a, b| b.click_count.cmp(&a.click_count));
    top_links.truncate(10);

    // Clicks by day (last 30 days)
    let mut clicks_by_day_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let date = event.created_at.format("%Y-%m-%d").to_string();
        *clicks_by_day_map.entry(date).or_insert(0) += 1;
    }
    let mut clicks_by_day: Vec<DayStats> = clicks_by_day_map.into_iter()
        .map(|(date, count)| DayStats { date, count })
        .collect();
    clicks_by_day.sort_by(|a, b| a.date.cmp(&b.date));

    // Top countries
    let mut country_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let country = event.country.clone().unwrap_or_else(|| "Unknown".to_string());
        *country_map.entry(country).or_insert(0) += 1;
    }
    let total_for_percentage = events.len().max(1) as f64;
    let mut top_countries: Vec<CountryStats> = country_map.into_iter()
        .map(|(country, count)| CountryStats {
            country,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();
    top_countries.sort_by(|a, b| b.count.cmp(&a.count));
    top_countries.truncate(10);

    // Top browsers
    let mut browser_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let browser = event.browser.clone().unwrap_or_else(|| "Unknown".to_string());
        *browser_map.entry(browser).or_insert(0) += 1;
    }
    let mut top_browsers: Vec<BrowserStats> = browser_map.into_iter()
        .map(|(browser, count)| BrowserStats {
            browser,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();
    top_browsers.sort_by(|a, b| b.count.cmp(&a.count));
    top_browsers.truncate(5);

    let response = DashboardStats {
        total_links,
        total_clicks,
        active_links,
        clicks_today,
        clicks_this_week,
        clicks_this_month,
        top_links,
        clicks_by_day,
        top_countries,
        top_browsers,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Get real-time click count for a link
#[utoipa::path(
    get,
    path = "/links/{id}/clicks/realtime",
    params(
        ("id" = i32, Path, description = "Link ID")
    ),
    responses(
        (status = 200, description = "Current click count"),
        (status = 404, description = "Link not found"),
    ),
    tag = "Analytics"
)]
pub async fn get_realtime_clicks(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let link = links::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .ok()
        .flatten();

    if let Some(link) = link {
        (StatusCode::OK, Json(serde_json::json!({
            "link_id": link.id,
            "click_count": link.click_count,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Link not found"}))).into_response()
    }
}

// Helper function to extract domain from URL
fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(|s| s.to_string()))
}
