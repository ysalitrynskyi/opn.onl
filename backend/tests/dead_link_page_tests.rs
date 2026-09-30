//! A short link that will not redirect answers a browser with a small HTML
//! page and everything else with the plain reason, as before. Real router +
//! real Postgres.

mod common;

use axum::http::{HeaderValue, header};
use chrono::{Duration, Utc};
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use serde_json::{Value, json};

const BROWSER_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

fn header_value(res: &axum_test::TestResponse, name: header::HeaderName) -> String {
    res.headers()
        .get(&name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn missing_code_is_a_page_for_browsers_and_text_for_clients() {
    let (server, _db) = spawn_real_app().await;
    let code = format!("zz{}", unique_code());

    let browser = server
        .get(&format!("/{code}"))
        .add_header(header::ACCEPT, HeaderValue::from_static(BROWSER_ACCEPT))
        .await;
    assert_eq!(browser.status_code(), 404, "{}", browser.text());
    assert!(
        header_value(&browser, header::CONTENT_TYPE).starts_with("text/html"),
        "{}",
        header_value(&browser, header::CONTENT_TYPE)
    );
    assert_eq!(header_value(&browser, header::CACHE_CONTROL), "no-store");
    let html = browser.text();
    assert!(html.contains("<h1>Link not found</h1>"), "{html}");
    assert!(
        html.contains(r#"<meta name="robots" content="noindex">"#),
        "{html}"
    );
    assert!(
        !html.contains(&code),
        "the page must not echo the code: {html}"
    );

    let client = server.get(&format!("/{code}")).await;
    assert_eq!(client.status_code(), 404);
    assert_eq!(client.text(), "Link not found");
}

#[tokio::test]
async fn expired_link_page_gives_the_reason() {
    let (server, db) = spawn_real_app().await;
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "{}", res.text());
    let body: Value = res.json();
    mark_email_verified(&db, body["user_id"].as_i64().unwrap() as i32).await;
    let token = body["token"].as_str().unwrap();

    let created = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({
            "original_url": "https://iana.org/dead-link-page",
            "expires_at": (Utc::now() - Duration::hours(1)).to_rfc3339(),
        }))
        .await;
    assert_eq!(created.status_code(), 201, "{}", created.text());
    let code = created.json::<Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    let browser = server
        .get(&format!("/{code}"))
        .add_header(header::ACCEPT, HeaderValue::from_static(BROWSER_ACCEPT))
        .await;
    assert_eq!(browser.status_code(), 410, "{}", browser.text());
    let html = browser.text();
    assert!(html.contains("<h1>Link unavailable</h1>"), "{html}");
    assert!(html.contains("<p>Link has expired.</p>"), "{html}");
}
