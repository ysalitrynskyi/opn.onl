use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::AppState;
use crate::entity::{click_events, links};
use crate::handlers::link_access::{LinkAction, live_link_for, own_links};
use crate::handlers::links::get_user_id_from_header;
use crate::utils::time::utc_rfc3339;

/// Default stats window when `days` is omitted. The dashboard's "Last 90 days"
/// option must stay inside the clamp below.
const DEFAULT_STATS_DAYS: i64 = 30;
const MIN_STATS_DAYS: i64 = 1;
const MAX_STATS_DAYS: i64 = 366;
/// Hard cap on click rows loaded for one stats request.
const MAX_STATS_EVENTS: u64 = 50_000;

fn clamp_stats_days(days: Option<i64>) -> i64 {
    days.unwrap_or(DEFAULT_STATS_DAYS)
        .clamp(MIN_STATS_DAYS, MAX_STATS_DAYS)
}

/// One entry per UTC day from `first` through today, zero where no click
/// landed. A series with only the days that had clicks makes a chart's
/// categorical axis join days that are weeks apart.
fn zero_filled_days(
    first: chrono::NaiveDate,
    clicks: &HashMap<chrono::NaiveDate, i64>,
) -> Vec<DayStats> {
    let today = chrono::Utc::now().date_naive();
    // A click stamped after "today" (clock skew between instances) keeps its day.
    let last = clicks
        .keys()
        .copied()
        .max()
        .map_or(today, |newest| newest.max(today));
    first
        .iter_days()
        .take_while(|day| *day <= last)
        .map(|day| DayStats {
            date: day.format("%Y-%m-%d").to_string(),
            count: clicks.get(&day).copied().unwrap_or(0),
        })
        .collect()
}

/// Aggregated geo bucket value: (latitude, longitude, city, country, hit count).
type GeoAggregate = (f64, f64, Option<String>, Option<String>, i64);

// ============= DTOs =============

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct AnalyticsQuery {
    /// Stats window in days. Omitted defaults to 30; values are clamped to 1..=366.
    pub days: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct LinkStatsResponse {
    pub link_id: i32,
    pub code: String,
    pub original_url: String,
    /// Clicks in the requested window, from a `COUNT(*)` over that window.
    /// Breakdown maps below may be computed from a newest-first sample when
    /// `truncated` is true.
    pub total_clicks: i32,
    /// True when the window contained more click rows than the per-request
    /// cap, so country/city/day percentages describe the newest sample.
    pub truncated: bool,
    pub unique_visitors: i32,
    /// One entry per UTC day, oldest first, from the day the window opens
    /// through today (days without clicks have `count` 0). On a truncated
    /// response the series starts at the oldest click in the sample.
    pub clicks_by_day: Vec<DayStats>,
    pub clicks_by_country: Vec<CountryStats>,
    pub clicks_by_city: Vec<CityStats>,
    pub clicks_by_device: Vec<DeviceStats>,
    pub clicks_by_browser: Vec<BrowserStats>,
    pub clicks_by_os: Vec<OsStats>,
    pub clicks_by_referer: Vec<RefererStats>,
    /// Clicks per URL they were redirected to, most clicks first: the result
    /// of an A/B split or of routing rules. Only links with routing rules
    /// record a destination; every other click counts toward the link's
    /// current `original_url`. From the same sample as the other breakdowns.
    pub clicks_by_destination: Vec<DestinationStats>,
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
    /// Country the city is in. Cities are counted per city and country, so
    /// the same name in two countries is two entries. `None` for clicks
    /// without a resolved city.
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
pub struct DestinationStats {
    pub destination: String,
    pub clicks: i64,
}

#[derive(Serialize, ToSchema)]
pub struct RecentClick {
    pub id: i64,
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
    /// Clicks since 00:00:00 UTC of the current UTC day. Not the caller's local day.
    pub clicks_today: i64,
    pub clicks_this_week: i64,
    pub clicks_this_month: i64,
    pub top_links: Vec<TopLink>,
    /// One entry per UTC day for the last 30 days through today, oldest first;
    /// days without clicks have `count` 0.
    pub clicks_by_day: Vec<DayStats>,
    pub top_countries: Vec<CountryStats>,
    pub top_browsers: Vec<BrowserStats>,
}

#[derive(Serialize, ToSchema)]
pub struct TopLink {
    pub id: i32,
    pub code: String,
    pub original_url: String,
    pub click_count: i64,
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
        (status = 403, description = "Forbidden, also for a link that does not exist"),
    ),
    tag = "Analytics",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_link_stats(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Query(query): Query<AnalyticsQuery>,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let link = match live_link_for(&state.db, id, user_id, LinkAction::Read).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    // Get time range. Clamp so a huge/negative `days` cannot panic inside
    // `TimeDelta::days` or load a link's entire click history into memory.
    let days = clamp_stats_days(query.days);
    let start_date = chrono::Utc::now().naive_utc()
        - chrono::Duration::try_days(days).unwrap_or(chrono::Duration::days(MAX_STATS_DAYS));

    // Fetch click events. Newest-first with a hard row cap so a busy link's
    // year of clicks cannot OOM the worker; aggregates then describe the most
    // recent events in the window.
    let events = click_events::Entity::find()
        .filter(click_events::Column::LinkId.eq(id))
        .filter(click_events::Column::CreatedAt.gte(start_date))
        .order_by_desc(click_events::Column::CreatedAt)
        .limit(MAX_STATS_EVENTS)
        .all(&state.db)
        .await
        .unwrap_or_default();

    // Below the row cap the sample is the whole window, so its length is the
    // total. Counting with a second query let a click flush that committed
    // between the two reads into the total but not into the sample, and the
    // answer claimed clicks that its series, breakdowns and recent clicks did
    // not show. Only a capped sample needs the count of the rest.
    let total_clicks = if (events.len() as u64) < MAX_STATS_EVENTS {
        events.len() as u64
    } else {
        click_events::Entity::find()
            .filter(click_events::Column::LinkId.eq(id))
            .filter(click_events::Column::CreatedAt.gte(start_date))
            .count(&state.db)
            .await
            .unwrap_or(0)
    };
    let truncated = total_clicks > events.len() as u64;
    let total_clicks = total_clicks.min(i32::MAX as u64) as i32;
    // Percentages describe the loaded sample, not the full window, so a
    // truncated response still adds to 100% of what is shown.
    let total_for_percentage = (events.len() as i32).max(1) as f64;

    // Unique visitors (by IP)
    let unique_ips: std::collections::HashSet<_> =
        events.iter().filter_map(|e| e.ip_address.clone()).collect();
    let unique_visitors = unique_ips.len() as i32;

    // Clicks by day, zero-filled. A truncated sample reaches back only to its
    // oldest event; the days before that were not loaded, not quiet.
    let mut clicks_per_day: HashMap<chrono::NaiveDate, i64> = HashMap::new();
    for event in &events {
        *clicks_per_day.entry(event.created_at.date()).or_insert(0) += 1;
    }
    let first_day = match events.last() {
        Some(oldest) if truncated => oldest.created_at,
        _ => start_date,
    };
    let clicks_by_day = zero_filled_days(first_day.date(), &clicks_per_day);

    // Clicks by country
    let mut country_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let country = event
            .country
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        *country_map.entry(country).or_insert(0) += 1;
    }
    let clicks_by_country: Vec<CountryStats> = country_map
        .into_iter()
        .map(|(country, count)| CountryStats {
            country,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by city. A city name alone is ambiguous (Paris, France or Texas),
    // so cities are keyed by city and country; clicks without a city share one
    // "Unknown" row whatever their country.
    let mut city_map: HashMap<(String, Option<String>), i64> = HashMap::new();
    for event in &events {
        let key = match &event.city {
            Some(city) => (city.clone(), event.country.clone()),
            None => ("Unknown".to_string(), None),
        };
        *city_map.entry(key).or_insert(0) += 1;
    }
    let clicks_by_city: Vec<CityStats> = city_map
        .into_iter()
        .map(|((city, country), count)| CityStats {
            city,
            country,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by device
    let mut device_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let device = event
            .device
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        *device_map.entry(device).or_insert(0) += 1;
    }
    let clicks_by_device: Vec<DeviceStats> = device_map
        .into_iter()
        .map(|(device, count)| DeviceStats {
            device,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by browser
    let mut browser_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let browser = event
            .browser
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        *browser_map.entry(browser).or_insert(0) += 1;
    }
    let clicks_by_browser: Vec<BrowserStats> = browser_map
        .into_iter()
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
    let clicks_by_os: Vec<OsStats> = os_map
        .into_iter()
        .map(|(os, count)| OsStats {
            os,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    // Clicks by referer
    let mut referer_map: HashMap<String, i64> = HashMap::new();
    for event in &events {
        let referer = event
            .referer
            .clone()
            .map(|r| extract_domain(&r).unwrap_or_else(|| r.clone()))
            .unwrap_or_else(|| "Direct".to_string());
        *referer_map.entry(referer).or_insert(0) += 1;
    }
    let clicks_by_referer: Vec<RefererStats> = referer_map
        .into_iter()
        .map(|(referer, count)| RefererStats {
            referer,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect();

    let clicks_by_destination = destination_breakdown(&events, &link.original_url);

    // Recent clicks (last 100)
    let recent_clicks: Vec<RecentClick> = events
        .iter()
        .take(100)
        .map(|e| RecentClick {
            id: e.id,
            timestamp: utc_rfc3339(e.created_at),
            country: e.country.clone(),
            city: e.city.clone(),
            device: e.device.clone(),
            browser: e.browser.clone(),
            os: e.os.clone(),
            referer: e.referer.clone(),
        })
        .collect();

    // Geo data for map
    let mut geo_map: HashMap<(i64, i64), GeoAggregate> = HashMap::new();
    for event in &events {
        if let (Some(lat), Some(lon)) = (event.latitude, event.longitude) {
            // Round to 2 decimal places for clustering
            let key = ((lat * 100.0) as i64, (lon * 100.0) as i64);
            let entry = geo_map.entry(key).or_insert((
                lat,
                lon,
                event.city.clone(),
                event.country.clone(),
                0,
            ));
            entry.4 += 1;
        }
    }
    let geo_data: Vec<GeoPoint> = geo_map
        .into_values()
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
        truncated,
        unique_visitors,
        clicks_by_day,
        clicks_by_country,
        clicks_by_city,
        clicks_by_device,
        clicks_by_browser,
        clicks_by_os,
        clicks_by_referer,
        clicks_by_destination,
        recent_clicks,
        geo_data,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Clicks per destination, most first, ties by URL so the order is stable.
/// A click without a destination went to the link's own URL, so it is counted
/// under `original_url` together with routed clicks that fell through to it.
fn destination_breakdown(
    events: &[click_events::Model],
    original_url: &str,
) -> Vec<DestinationStats> {
    let mut destination_map: HashMap<&str, i64> = HashMap::new();
    for event in events {
        let destination = event.destination.as_deref().unwrap_or(original_url);
        *destination_map.entry(destination).or_insert(0) += 1;
    }
    let mut breakdown: Vec<DestinationStats> = destination_map
        .into_iter()
        .map(|(destination, clicks)| DestinationStats {
            destination: destination.to_string(),
            clicks,
        })
        .collect();
    breakdown.sort_by(|a, b| {
        b.clicks
            .cmp(&a.clicks)
            .then_with(|| a.destination.cmp(&b.destination))
    });
    breakdown
}

/// `ids` are i32 primary keys from our own query, never request text.
fn sql_int_list(ids: &[i32]) -> String {
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

async fn dashboard_window_counts(
    db: &DatabaseConnection,
    link_ids: &[i32],
    today_start: chrono::NaiveDateTime,
    week_start: chrono::NaiveDateTime,
    month_start: chrono::NaiveDateTime,
) -> Result<(i64, i64, i64), DbErr> {
    if link_ids.is_empty() {
        return Ok((0, 0, 0));
    }
    let sql = format!(
        "SELECT COUNT(*) FILTER (WHERE created_at >= $1)::bigint AS today, \
                COUNT(*) FILTER (WHERE created_at >= $2)::bigint AS week, \
                COUNT(*)::bigint AS month \
         FROM click_events \
         WHERE link_id IN ({}) AND created_at >= $3",
        sql_int_list(link_ids)
    );
    let row = db
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [today_start.into(), week_start.into(), month_start.into()],
        ))
        .await?;
    match row {
        Some(row) => Ok((
            row.try_get::<i64>("", "today")?,
            row.try_get::<i64>("", "week")?,
            row.try_get::<i64>("", "month")?,
        )),
        None => Ok((0, 0, 0)),
    }
}

async fn dashboard_clicks_by_day(
    db: &DatabaseConnection,
    link_ids: &[i32],
    month_start: chrono::NaiveDateTime,
) -> Result<Vec<DayStats>, DbErr> {
    let first_day = month_start.date();
    if link_ids.is_empty() {
        return Ok(zero_filled_days(first_day, &HashMap::new()));
    }
    let sql = format!(
        "SELECT created_at::date AS day, COUNT(*)::bigint AS cnt \
         FROM click_events \
         WHERE link_id IN ({}) AND created_at >= $1 \
         GROUP BY 1",
        sql_int_list(link_ids)
    );
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [month_start.into()],
        ))
        .await?;
    let clicks_per_day: HashMap<chrono::NaiveDate, i64> = rows
        .into_iter()
        .filter_map(|row| {
            let day = row.try_get::<chrono::NaiveDate>("", "day").ok()?;
            let count = row.try_get::<i64>("", "cnt").ok()?;
            Some((day, count))
        })
        .collect();
    Ok(zero_filled_days(first_day, &clicks_per_day))
}

async fn dashboard_grouped_label_counts(
    db: &DatabaseConnection,
    link_ids: &[i32],
    month_start: chrono::NaiveDateTime,
    column: click_events::Column,
) -> Result<Vec<(String, i64)>, DbErr> {
    if link_ids.is_empty() {
        return Ok(vec![]);
    }
    let rows: Vec<(Option<String>, i64)> = click_events::Entity::find()
        .select_only()
        .column(column)
        .column_as(click_events::Column::Id.count(), "cnt")
        // Inline like the other dashboard queries: one bind parameter per link
        // would fail outright past Postgres's 65,535-parameter limit.
        .filter(sea_query::Expr::cust(format!(
            "link_id IN ({})",
            sql_int_list(link_ids)
        )))
        .filter(click_events::Column::CreatedAt.gte(month_start))
        .group_by(column)
        .into_tuple()
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(label, count)| (label.unwrap_or_else(|| "Unknown".to_string()), count))
        .collect())
}

async fn dashboard_top_countries(
    db: &DatabaseConnection,
    link_ids: &[i32],
    month_start: chrono::NaiveDateTime,
    total_for_percentage: f64,
) -> Result<Vec<CountryStats>, DbErr> {
    let mut rows =
        dashboard_grouped_label_counts(db, link_ids, month_start, click_events::Column::Country)
            .await?;
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    rows.truncate(10);
    Ok(rows
        .into_iter()
        .map(|(country, count)| CountryStats {
            country,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect())
}

async fn dashboard_top_browsers(
    db: &DatabaseConnection,
    link_ids: &[i32],
    month_start: chrono::NaiveDateTime,
    total_for_percentage: f64,
) -> Result<Vec<BrowserStats>, DbErr> {
    let mut rows =
        dashboard_grouped_label_counts(db, link_ids, month_start, click_events::Column::Browser)
            .await?;
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    rows.truncate(5);
    Ok(rows
        .into_iter()
        .map(|(browser, count)| BrowserStats {
            browser,
            count,
            percentage: (count as f64 / total_for_percentage) * 100.0,
        })
        .collect())
}

/// Get dashboard analytics
#[utoipa::path(
    get,
    path = "/analytics/dashboard",
    responses(
        (status = 200, description = "Dashboard statistics. clicks_today is the UTC calendar day.", body = DashboardStats),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "A statistics query failed. Never answered as zero clicks"),
    ),
    tag = "Analytics",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_dashboard_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    match dashboard_stats(&state.db, user_id).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => {
            // Zeros here would read as "no clicks" next to a real total.
            tracing::error!("Dashboard statistics query failed: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            )
                .into_response()
        }
    }
}

/// Top links on the dashboard.
const DASHBOARD_TOP_LINKS: u64 = 10;

async fn dashboard_stats(db: &DatabaseConnection, user_id: i32) -> Result<DashboardStats, DbErr> {
    // The caller's own links (exclude deleted). A link of an organization they
    // have left no longer counts: it is not theirs to read any more. Only the
    // columns that decide whether a link is live are loaded: an account can
    // hold a hundred thousand links, each with up to 5,000 characters of notes.
    type ActivityRow = (
        i32,
        Option<chrono::NaiveDateTime>,
        Option<chrono::NaiveDateTime>,
        Option<chrono::NaiveDateTime>,
        Option<i32>,
        bool,
        i64,
    );
    let rows: Vec<ActivityRow> = links::Entity::find()
        .select_only()
        .column(links::Column::Id)
        .column(links::Column::BurnedAt)
        .column(links::Column::StartsAt)
        .column(links::Column::ExpiresAt)
        .column(links::Column::MaxClicks)
        .column(links::Column::BurnAfterReading)
        .column(links::Column::ClickCount)
        .filter(own_links(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .into_tuple()
        .all(db)
        .await?;

    let total_links = rows.len() as i64;
    let total_clicks: i64 = rows.iter().map(|row| row.6).sum();
    let active_links = rows
        .iter()
        .filter(
            |(_, burned_at, starts_at, expires_at, max_clicks, burn_after_reading, click_count)| {
                links::LinkActivity {
                    deleted_at: None,
                    burned_at: *burned_at,
                    starts_at: *starts_at,
                    expires_at: *expires_at,
                    max_clicks: *max_clicks,
                    burn_after_reading: *burn_after_reading,
                    click_count: *click_count,
                }
                .is_active()
            },
        )
        .count() as i64;

    let link_ids: Vec<i32> = rows.iter().map(|row| row.0).collect();

    // Get time boundaries. "Today" is the UTC calendar day; this endpoint
    // does not take a timezone, so the field is documented as UTC.
    let now = chrono::Utc::now().naive_utc();
    let today_start = now.date().and_hms_opt(0, 0, 0).unwrap();
    let week_start = now - chrono::Duration::days(7);
    let month_start = now - chrono::Duration::days(30);

    // Aggregates stay in SQL. Loading every click in the window used to OOM
    // the worker on a busy account; a truncated row cap would silently under-
    // count, so COUNT / GROUP BY over the window is the correct shape.
    let (clicks_today, clicks_this_week, clicks_this_month) =
        dashboard_window_counts(db, &link_ids, today_start, week_start, month_start).await?;
    let clicks_by_day = dashboard_clicks_by_day(db, &link_ids, month_start).await?;
    let total_for_percentage = clicks_this_month.max(1) as f64;
    let top_countries =
        dashboard_top_countries(db, &link_ids, month_start, total_for_percentage).await?;
    let top_browsers =
        dashboard_top_browsers(db, &link_ids, month_start, total_for_percentage).await?;

    // Top links: the database sorts and keeps ten, so only those rows carry
    // their URL.
    let top_links: Vec<TopLink> = links::Entity::find()
        .select_only()
        .column(links::Column::Id)
        .column(links::Column::Code)
        .column(links::Column::OriginalUrl)
        .column(links::Column::ClickCount)
        .filter(own_links(user_id))
        .filter(links::Column::DeletedAt.is_null())
        .order_by_desc(links::Column::ClickCount)
        .order_by_desc(links::Column::Id)
        .limit(DASHBOARD_TOP_LINKS)
        .into_tuple::<(i32, String, String, i64)>()
        .all(db)
        .await?
        .into_iter()
        .map(|(id, code, original_url, click_count)| TopLink {
            id,
            code,
            original_url,
            click_count,
        })
        .collect();

    Ok(DashboardStats {
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
    })
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
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden, also for a link that does not exist"),
    ),
    tag = "Analytics",
    security(("bearer_auth" = []), ("api_key" = []))
)]
pub async fn get_realtime_clicks(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match get_user_id_from_header(&state.db, &headers).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let link = match live_link_for(&state.db, id, user_id, LinkAction::Read).await {
        Ok(link) => link,
        Err(denied) => return denied.into_response(),
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "link_id": link.id,
            "click_count": link.click_count,
            "timestamp": utc_rfc3339(chrono::Utc::now().naive_utc())
        })),
    )
        .into_response()
}

// Helper function to extract domain from URL
fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
}
