//! Abuse reports: a visitor can flag a live short link, and an admin can
//! queue, dismiss, or resolve it. Resolving can block the destination through
//! the same path as `POST /admin/blocked/links`.

use crate::common;

use std::sync::Arc;

use axum::http::{Method, header};
use axum_test::TestServer;
use common::{mark_email_verified, setup_test_db, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, Statement,
};
use serde_json::{Value, json};

const PASSWORD: &str = "password123";

/// Same router as [`spawn_real_app`], with the 10-per-second gate widened.
/// The mock transport puts every call on one address, and the admin flows
/// make more than ten calls back to back. The hourly report budget is
/// unchanged; [`report_budget_is_five_per_hour`] keeps the real limiters.
async fn spawn_app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let mut limiters = (*state.rate_limiters).clone();
    limiters.per_second = Arc::new(RateLimiter::new(RateLimitConfig::new(10_000, 1)));
    state.rate_limiters = Arc::new(limiters);
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

async fn make_admin(db: &DatabaseConnection, user_id: i32) {
    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db error")
        .expect("user not found");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("promote");
}

async fn register_admin(server: &axum_test::TestServer, db: &DatabaseConnection) -> (String, i32) {
    let (token, user_id) = register(server, &unique_email()).await;
    make_admin(db, user_id).await;
    (token, user_id)
}

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32, String) {
    let email = unique_email();
    let (token, user_id) = register(server, &email).await;
    mark_email_verified(db, user_id).await;
    (token, user_id, email)
}

async fn create_link(server: &axum_test::TestServer, token: &str, url: &str) -> (i32, String) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({ "original_url": url }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

fn report_body(code: &str, reason: &str) -> Value {
    json!({ "code": code, "reason": reason })
}

async fn column_names(db: &DatabaseConnection) -> Vec<String> {
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = 'public' AND table_name = 'abuse_reports' \
             ORDER BY column_name",
            [],
        ))
        .await
        .expect("information_schema");
    rows.into_iter()
        .map(|row| {
            row.try_get::<String>("", "column_name")
                .expect("column_name")
        })
        .collect()
}

async fn reports_for_code(
    server: &axum_test::TestServer,
    admin_token: &str,
    code: &str,
    status: Option<&str>,
) -> Value {
    let mut req = server
        .get("/admin/reports")
        .add_query_param("code", code)
        .authorization_bearer(admin_token);
    if let Some(status) = status {
        req = req.add_query_param("status", status);
    }
    let res = req.await;
    assert_eq!(res.status_code(), 200, "list reports: {}", res.text());
    res.json()
}

#[tokio::test]
async fn anonymous_report_is_stored_without_an_ip_column() {
    let (server, db) = spawn_app().await;
    let columns = column_names(&db).await;
    let expected = [
        "code",
        "created_at",
        "destination_url",
        "details",
        "id",
        "link_id",
        "reason",
        "reporter_email",
        "resolved_at",
        "resolved_by",
        "status",
    ];
    assert_eq!(
        columns, expected,
        "abuse_reports columns drifted: {columns:?}"
    );
    for forbidden in ["ip", "ip_address", "client_ip", "user_agent", "useragent"] {
        assert!(
            !columns.iter().any(|name| name == forbidden),
            "abuse_reports must not store {forbidden}"
        );
    }

    let (admin_token, _) = register_admin(&server, &db).await;
    let (owner_token, _, owner_email) = register_verified(&server, &db).await;
    let url = format!("https://iana.org/abuse-stored-{}", unique_code());
    let (link_id, code) = create_link(&server, &owner_token, &url).await;

    let res = server
        .post("/report")
        .json(&json!({
            "code": format!("  {code}  "),
            "reason": " phishing ",
            "details": "  looks like a bank  ",
            "email": " Bob@Example.COM ",
        }))
        .await;
    assert_eq!(res.status_code(), 201, "{}", res.text());
    assert_eq!(res.json::<Value>()["status"], "received");

    // A second report of the same code and reason is accepted. Deduping is
    // not the point of the intake.
    let again = server
        .post("/report")
        .json(&report_body(&code, "phishing"))
        .await;
    assert_eq!(again.status_code(), 201, "{}", again.text());

    let listed = reports_for_code(&server, &admin_token, &code, Some("open")).await;
    assert_eq!(listed["total"].as_u64(), Some(2));
    let row = listed["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["details"] == "looks like a bank")
        .expect("detailed report");
    assert_eq!(row["code"], code);
    assert_eq!(row["reason"], "phishing");
    assert_eq!(row["details"], "looks like a bank");
    assert_eq!(row["reporter_email"], "bob@example.com");
    assert_eq!(row["status"], "open");
    assert_eq!(row["link_id"].as_i64(), Some(link_id as i64));
    assert_eq!(row["link_status"], "active");
    assert_eq!(row["destination_url"], url);
    assert_eq!(row["current_destination_url"], url);
    assert_eq!(row["owner_email"], owner_email);
    assert!(row["resolved_at"].is_null());
    assert!(row["resolved_by"].is_null());
    assert!(row["created_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn unknown_and_deleted_codes_are_not_found() {
    let (server, db) = spawn_app().await;
    let (owner_token, _, _) = register_verified(&server, &db).await;
    let (_id, live) = create_link(
        &server,
        &owner_token,
        &format!("https://iana.org/abuse-live-{}", unique_code()),
    )
    .await;
    let (deleted_id, deleted) = create_link(
        &server,
        &owner_token,
        &format!("https://iana.org/abuse-gone-{}", unique_code()),
    )
    .await;

    let missing = server
        .post("/report")
        .json(&report_body(&unique_code(), "spam"))
        .await;
    assert_eq!(missing.status_code(), 404, "{}", missing.text());
    assert_eq!(missing.json::<Value>()["error"], "Link not found");

    let removed = server
        .delete(&format!("/links/{deleted_id}"))
        .authorization_bearer(&owner_token)
        .await;
    assert_eq!(removed.status_code(), 200, "delete: {}", removed.text());

    let gone = server
        .post("/report")
        .json(&report_body(&deleted, "spam"))
        .await;
    assert_eq!(gone.status_code(), 404, "{}", gone.text());
    assert_eq!(gone.json::<Value>()["error"], "Link not found");

    // Control: a live code still reports, including one that has expired.
    let (expired_id, expired) = create_link(
        &server,
        &owner_token,
        &format!("https://iana.org/abuse-expired-{}", unique_code()),
    )
    .await;
    let link = links::Entity::find_by_id(expired_id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    let mut active: links::ActiveModel = link.into();
    active.expires_at = Set(Some(
        chrono::Utc::now().naive_utc() - chrono::Duration::hours(2),
    ));
    active.update(&db).await.expect("expire");

    let ok = server
        .post("/report")
        .json(&report_body(&expired, "other"))
        .await;
    assert_eq!(
        ok.status_code(),
        201,
        "expired link is still reportable: {}",
        ok.text()
    );

    let live_ok = server
        .post("/report")
        .json(&report_body(&live, "spam"))
        .await;
    assert_eq!(live_ok.status_code(), 201, "{}", live_ok.text());
}

#[tokio::test]
async fn validation_rejects_bad_input_without_echoing_it() {
    let (server, db) = spawn_app().await;
    let (token, _, _) = register_verified(&server, &db).await;
    let (_id, code) = create_link(
        &server,
        &token,
        &format!("https://iana.org/abuse-valid-{}", unique_code()),
    )
    .await;

    let cases = [
        json!({ "code": "   ", "reason": "spam" }),
        json!({ "code": code, "reason": "SECRET" }),
        json!({
            "code": code,
            "reason": "spam",
            "details": format!("SECRET{}", "x".repeat(2001)),
        }),
        json!({
            "code": code,
            "reason": "spam",
            "email": "not-an-email-SECRET",
        }),
    ];
    for body in cases {
        let res = server.post("/report").json(&body).await;
        assert_eq!(res.status_code(), 400, "{}", res.text());
        let text = res.text();
        assert!(
            !text.contains("SECRET"),
            "validation must not echo the submitted value: {text}"
        );
        assert!(text.contains("\"error\""), "{text}");
    }

    let ok = server
        .post("/report")
        .json(&report_body(&code, "malware"))
        .await;
    assert_eq!(
        ok.status_code(),
        201,
        "valid report still works: {}",
        ok.text()
    );
}

#[tokio::test]
async fn a_bad_authorization_header_is_rejected() {
    let (server, db) = spawn_app().await;
    let (token, _, _) = register_verified(&server, &db).await;
    let (_id, code) = create_link(
        &server,
        &token,
        &format!("https://iana.org/abuse-auth-{}", unique_code()),
    )
    .await;

    let bad = server
        .post("/report")
        .add_header("authorization", "Bearer not-a-token")
        .json(&report_body(&code, "illegal"))
        .await;
    assert_eq!(bad.status_code(), 401, "{}", bad.text());
    assert_eq!(
        bad.json::<Value>()["error"],
        "Invalid or expired credentials"
    );

    let authed = server
        .post("/report")
        .authorization_bearer(&token)
        .json(&report_body(&code, "illegal"))
        .await;
    assert_eq!(authed.status_code(), 201, "valid bearer: {}", authed.text());

    let anon = server
        .post("/report")
        .json(&report_body(&code, "other"))
        .await;
    assert_eq!(anon.status_code(), 201, "anonymous: {}", anon.text());
}

#[tokio::test]
async fn report_budget_is_five_per_hour() {
    let (server, db) = spawn_real_app().await;
    let (token, _, _) = register_verified(&server, &db).await;
    let (_id, code) = create_link(
        &server,
        &token,
        &format!("https://iana.org/abuse-rate-{}", unique_code()),
    )
    .await;

    for n in 1..=5 {
        let res = server
            .post("/report")
            .json(&report_body(&code, "spam"))
            .await;
        assert_eq!(res.status_code(), 201, "report {n}: {}", res.text());
    }

    let limited = server
        .post("/report")
        .json(&report_body(&code, "spam"))
        .await;
    assert_eq!(limited.status_code(), 429, "{}", limited.text());
    let retry_after: u64 = limited
        .header("retry-after")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (1..=3600).contains(&retry_after),
        "Retry-After {retry_after}"
    );
    let body: Value = limited.json();
    assert!(body["retry_after"].as_u64().is_some());
}

#[tokio::test]
async fn admin_can_list_resolve_and_dismiss() {
    let (server, db) = spawn_app().await;
    let (admin_token, admin_id) = register_admin(&server, &db).await;
    let (user_token, _, _) = register_verified(&server, &db).await;
    let (_id, code) = create_link(
        &server,
        &user_token,
        &format!("https://iana.org/abuse-queue-{}", unique_code()),
    )
    .await;
    let (_id, other) = create_link(
        &server,
        &user_token,
        &format!("https://iana.org/abuse-queue-other-{}", unique_code()),
    )
    .await;

    let anonymous = server.get("/admin/reports").await;
    assert_eq!(anonymous.status_code(), 401, "{}", anonymous.text());

    let forbidden = server
        .get("/admin/reports")
        .authorization_bearer(&user_token)
        .await;
    assert_eq!(forbidden.status_code(), 403, "{}", forbidden.text());
    assert_eq!(
        forbidden.json::<Value>()["message"],
        "Admin access required"
    );

    let filed = server
        .post("/report")
        .json(&report_body(&code, "malware"))
        .await;
    assert_eq!(filed.status_code(), 201, "{}", filed.text());
    let filed_other = server
        .post("/report")
        .json(&report_body(&other, "spam"))
        .await;
    assert_eq!(filed_other.status_code(), 201, "{}", filed_other.text());

    let open = reports_for_code(&server, &admin_token, &code, Some("open")).await;
    let report_id = open["reports"][0]["id"].as_i64().expect("id");
    let other_open = reports_for_code(&server, &admin_token, &other, Some("open")).await;
    let other_id = other_open["reports"][0]["id"].as_i64().expect("id");

    let bad_filter = server
        .get("/admin/reports")
        .add_query_param("status", "nope")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(bad_filter.status_code(), 400, "{}", bad_filter.text());

    let resolved = server
        .post(&format!("/admin/reports/{report_id}/resolve"))
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "resolved" }))
        .await;
    assert_eq!(resolved.status_code(), 200, "{}", resolved.text());
    assert_eq!(resolved.json::<Value>()["message"], "Report resolved");

    let still_open = reports_for_code(&server, &admin_token, &code, Some("open")).await;
    assert_eq!(still_open["reports"].as_array().unwrap().len(), 0);
    let closed = reports_for_code(&server, &admin_token, &code, Some("resolved")).await;
    assert_eq!(closed["reports"][0]["status"], "resolved");
    assert_eq!(
        closed["reports"][0]["resolved_by"].as_i64(),
        Some(admin_id as i64)
    );
    assert!(
        closed["reports"][0]["resolved_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );

    let again = server
        .post(&format!("/admin/reports/{report_id}/resolve"))
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "dismissed" }))
        .await;
    assert_eq!(again.status_code(), 400, "{}", again.text());
    assert_eq!(
        again.json::<Value>()["message"],
        "This report is already closed"
    );

    let user_resolve = server
        .post(&format!("/admin/reports/{other_id}/resolve"))
        .authorization_bearer(&user_token)
        .json(&json!({ "status": "dismissed" }))
        .await;
    assert_eq!(user_resolve.status_code(), 403, "{}", user_resolve.text());

    let dismissed = server
        .post(&format!("/admin/reports/{other_id}/resolve"))
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "dismissed" }))
        .await;
    assert_eq!(dismissed.status_code(), 200, "{}", dismissed.text());
    assert_eq!(dismissed.json::<Value>()["message"], "Report dismissed");
    let dismissed_row = reports_for_code(&server, &admin_token, &other, Some("dismissed")).await;
    assert_eq!(dismissed_row["reports"][0]["status"], "dismissed");

    let missing = server
        .post("/admin/reports/2147483646/resolve")
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "resolved" }))
        .await;
    assert_eq!(missing.status_code(), 404, "{}", missing.text());
    assert_eq!(missing.json::<Value>()["message"], "Report not found");
}

#[tokio::test]
async fn resolve_with_block_link_stops_the_redirect() {
    let (server, db) = spawn_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (owner_token, _, _) = register_verified(&server, &db).await;
    let marker = unique_code();
    let url = format!("https://iana.org/abuse-block-{marker}");
    let (_id, code) = create_link(&server, &owner_token, &url).await;

    let before = server.get(&format!("/{code}")).await;
    assert_eq!(
        before.status_code(),
        307,
        "link redirected before the block"
    );

    let filed = server
        .post("/report")
        .json(&json!({
            "code": code,
            "reason": "phishing",
            "email": "reporter@example.com",
            "details": "do not copy this into the block reason",
        }))
        .await;
    assert_eq!(filed.status_code(), 201, "{}", filed.text());
    let listed = reports_for_code(&server, &admin_token, &code, Some("open")).await;
    let report_id = listed["reports"][0]["id"].as_i64().expect("id");

    let resolved = server
        .post(&format!("/admin/reports/{report_id}/resolve"))
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "resolved", "block_link": true }))
        .await;
    assert_eq!(resolved.status_code(), 200, "{}", resolved.text());
    assert_eq!(
        resolved.json::<Value>()["message"],
        "Report resolved and the destination is blocked"
    );

    let after = server.get(&format!("/{code}")).await;
    assert_eq!(
        after.status_code(),
        410,
        "blocked destination no longer redirects"
    );

    let blocked = server
        .get("/admin/blocked/links")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(blocked.status_code(), 200, "{}", blocked.text());
    let rows: Vec<Value> = blocked.json();
    let row = rows
        .iter()
        .find(|row| row["url"].as_str().unwrap_or("").contains(&marker))
        .expect("blocked url present");
    assert_eq!(row["reason"], "phishing");
    assert_ne!(row["reason"], "do not copy this into the block reason");

    // Closing a second report of an already-blocked destination still resolves.
    let filed_again = server
        .post("/report")
        .json(&report_body(&code, "spam"))
        .await;
    assert_eq!(filed_again.status_code(), 201, "{}", filed_again.text());
    let again = reports_for_code(&server, &admin_token, &code, Some("open")).await;
    let again_id = again["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["reason"] == "spam")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let second = server
        .post(&format!("/admin/reports/{again_id}/resolve"))
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "resolved", "block_link": true }))
        .await;
    assert_eq!(second.status_code(), 200, "{}", second.text());
    assert_eq!(
        second.json::<Value>()["message"],
        "Report resolved. The destination was already blocked"
    );
}

async fn open_report_count(server: &axum_test::TestServer, admin_token: &str) -> i64 {
    let res = server
        .get("/admin/stats")
        .authorization_bearer(admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    res.json::<Value>()["open_reports"]
        .as_i64()
        .expect("open_reports")
}

#[tokio::test]
async fn open_reports_counts_the_report_this_test_created() {
    let (server, db) = spawn_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (owner_token, _, _) = register_verified(&server, &db).await;

    // The counter is global and these tests share one database. A sample that
    // moved by anything other than this report was crossed by another test,
    // so it is discarded. The assertion on a quiet sample is still exactly one.
    let mut last = String::new();
    for _ in 0..8 {
        let (_id, code) = create_link(
            &server,
            &owner_token,
            &format!("https://iana.org/abuse-count-{}", unique_code()),
        )
        .await;
        let before_n = open_report_count(&server, &admin_token).await;

        let filed = server
            .post("/report")
            .json(&report_body(&code, "other"))
            .await;
        assert_eq!(filed.status_code(), 201, "{}", filed.text());

        let after_n = open_report_count(&server, &admin_token).await;
        if after_n - before_n != 1 {
            last = format!("filed {before_n} -> {after_n}");
            continue;
        }

        let open = reports_for_code(&server, &admin_token, &code, Some("open")).await;
        assert_eq!(open["total"].as_u64(), Some(1));
        assert_eq!(open["reports"][0]["code"], code);
        let report_id = open["reports"][0]["id"].as_i64().unwrap();

        let resolved = server
            .post(&format!("/admin/reports/{report_id}/resolve"))
            .authorization_bearer(&admin_token)
            .json(&json!({ "status": "resolved" }))
            .await;
        assert_eq!(resolved.status_code(), 200, "{}", resolved.text());

        let gone = reports_for_code(&server, &admin_token, &code, Some("open")).await;
        assert!(gone["reports"].as_array().unwrap().is_empty());

        let closed_n = open_report_count(&server, &admin_token).await;
        if closed_n != before_n {
            last = format!("resolved {after_n} -> {closed_n}, before {before_n}");
            continue;
        }
        assert_eq!(after_n - closed_n, 1);
        return;
    }
    panic!("open_reports never moved by exactly this report ({last})");
}

const IPHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// The code `report` can already exist in this database. Point that one row
/// at `url` and clear anything that would stop a plain redirect.
async fn upsert_report_code(db: &DatabaseConnection, url: &str) {
    use opn_onl_backend::entity::routing_rules;

    let existing = links::Entity::find()
        .filter(links::Column::Code.eq("report"))
        .one(db)
        .await
        .expect("lookup code report");
    let link = if let Some(row) = existing {
        let mut active: links::ActiveModel = row.into();
        active.original_url = Set(url.to_string());
        active.user_id = Set(None);
        active.org_id = Set(None);
        active.deleted_at = Set(None);
        active.password_hash = Set(None);
        active.expires_at = Set(None);
        active.starts_at = Set(None);
        active.max_clicks = Set(None);
        active.burn_after_reading = Set(false);
        active.burned_at = Set(None);
        active.safe_link_interstitial = Set(false);
        active.click_count = Set(0);
        active.update(db).await.expect("reset code report")
    } else {
        links::ActiveModel {
            code: Set("report".to_string()),
            original_url: Set(url.to_string()),
            user_id: Set(None),
            click_count: Set(0),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert code report")
    };
    routing_rules::Entity::delete_many()
        .filter(routing_rules::Column::LinkId.eq(link.id))
        .exec(db)
        .await
        .expect("clear rules on code report");
}

fn location_of(res: &axum_test::TestResponse) -> String {
    res.header(header::LOCATION)
        .to_str()
        .expect("utf-8 location")
        .to_string()
}

/// `GET` and `HEAD /report` are the short code, not the intake. They must
/// keep redirecting, and they must miss the 10-per-second API gate (the
/// 11th call inside one second would be 429 if they were classified as API).
/// `POST /report` stays the intake, and a new alias `report` stays reserved.
#[tokio::test]
async fn get_and_head_report_redirect_an_existing_code() {
    let (server, db) = spawn_real_app().await;
    let url = format!("https://iana.org/code-report-{}", unique_code());
    upsert_report_code(&db, &url).await;

    for _ in 0..12 {
        let res = server.get("/report").await;
        assert_ne!(
            res.status_code(),
            429,
            "GET /report was charged the API per-second budget: {}",
            res.text()
        );
        assert_eq!(res.status_code(), 307, "{}", res.text());
        assert_eq!(location_of(&res), url);
    }

    let head = server.method(Method::HEAD, "/report").await;
    assert_ne!(head.status_code(), 429, "{}", head.text());
    assert_eq!(head.status_code(), 307, "{}", head.text());
    assert_eq!(location_of(&head), url);

    let (token, _, _) = register_verified(&server, &db).await;
    let reserved = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": format!("https://iana.org/not-the-code-{}", unique_code()),
            "custom_alias": "report",
        }))
        .await;
    assert_eq!(reserved.status_code(), 400, "{}", reserved.text());
    assert!(
        reserved.text().to_lowercase().contains("reserved"),
        "{}",
        reserved.text()
    );

    let filed = server
        .post("/report")
        .json(&report_body("report", "phishing"))
        .await;
    assert_eq!(filed.status_code(), 201, "{}", filed.text());
}

/// Blocking from the queue stops every destination the link can still send
/// someone to: the URL stored on the report, the fallback the owner pointed
/// it at afterwards, and each routing rule. A link that was never reported
/// keeps working.
#[tokio::test]
async fn block_and_resolve_stops_the_stored_url_the_new_fallback_and_the_rule() {
    let (server, db) = spawn_app().await;
    let (admin_token, _) = register_admin(&server, &db).await;
    let (owner_token, _, _) = register_verified(&server, &db).await;
    let (bystander_token, _, _) = register_verified(&server, &db).await;
    let marker = unique_code();
    let stored = format!("https://iana.org/abuse-stored-{marker}");
    let moved = format!("https://iana.org/abuse-moved-{marker}");
    let rule = format!("https://iana.org/abuse-rule-{marker}");
    let control_url = format!("https://iana.org/abuse-control-{marker}");

    let (link_id, code) = create_link(&server, &owner_token, &stored).await;
    let (_stored_id, stored_code) = create_link(&server, &bystander_token, &stored).await;
    let (_rule_id, rule_code) = create_link(&server, &bystander_token, &rule).await;
    let (_control_id, control_code) = create_link(&server, &bystander_token, &control_url).await;

    let rules = server
        .put(&format!("/links/{link_id}/rules"))
        .authorization_bearer(&owner_token)
        .json(&json!({
            "rules": [{
                "match_device": "Mobile",
                "destination_url": rule,
                "weight": 1,
                "priority": 0
            }]
        }))
        .await;
    assert_eq!(rules.status_code(), 200, "{}", rules.text());

    let filed = server
        .post("/report")
        .json(&report_body(&code, "phishing"))
        .await;
    assert_eq!(filed.status_code(), 201, "{}", filed.text());

    let edited = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "original_url": moved }))
        .await;
    assert_eq!(edited.status_code(), 200, "{}", edited.text());

    let listed = reports_for_code(&server, &admin_token, &code, Some("open")).await;
    assert_eq!(listed["reports"][0]["destination_url"], stored);
    assert_eq!(listed["reports"][0]["current_destination_url"], moved);
    let report_id = listed["reports"][0]["id"].as_i64().unwrap();

    let desktop = server
        .get(&format!("/{code}"))
        .add_header(header::USER_AGENT, DESKTOP_UA)
        .await;
    assert_eq!(desktop.status_code(), 307, "{}", desktop.text());
    assert_eq!(location_of(&desktop), moved);
    let iphone = server
        .get(&format!("/{code}"))
        .add_header(header::USER_AGENT, IPHONE_UA)
        .await;
    assert_eq!(iphone.status_code(), 307, "{}", iphone.text());
    assert_eq!(location_of(&iphone), rule);

    let resolved = server
        .post(&format!("/admin/reports/{report_id}/resolve"))
        .authorization_bearer(&admin_token)
        .json(&json!({ "status": "resolved", "block_link": true }))
        .await;
    assert_eq!(resolved.status_code(), 200, "{}", resolved.text());
    assert_eq!(
        resolved.json::<Value>()["message"],
        "Report resolved and the destination is blocked"
    );

    for (which, ua) in [("desktop", DESKTOP_UA), ("iphone", IPHONE_UA)] {
        let res = server
            .get(&format!("/{code}"))
            .add_header(header::USER_AGENT, ua)
            .await;
        assert_eq!(res.status_code(), 410, "{which}: {}", res.text());
    }
    let stored_visit = server.get(&format!("/{stored_code}")).await;
    assert_eq!(stored_visit.status_code(), 410, "{}", stored_visit.text());
    let rule_visit = server.get(&format!("/{rule_code}")).await;
    assert_eq!(rule_visit.status_code(), 410, "{}", rule_visit.text());
    let control = server.get(&format!("/{control_code}")).await;
    assert_eq!(control.status_code(), 307, "{}", control.text());
    assert_eq!(location_of(&control), control_url);

    let blocked = server
        .get("/admin/blocked/links")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(blocked.status_code(), 200, "{}", blocked.text());
    let rows: Vec<Value> = blocked.json();
    for url in [&stored, &moved, &rule] {
        let row = rows
            .iter()
            .find(|row| row["url"] == *url)
            .unwrap_or_else(|| panic!("missing block for {url}"));
        assert_eq!(row["reason"], "phishing");
    }
    assert!(
        rows.iter().all(|row| row["url"] != control_url),
        "the control destination was blocked"
    );
}
