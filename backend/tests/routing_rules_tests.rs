//! Smart-routing rules: evaluation order, A/B ties and input validation on
//! `PUT /links/{id}/rules`.

mod common;

use axum::http::header;
use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

const IPHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const MOBILE: &str = "https://iana.org/routing-mobile";
const WEB: &str = "https://iana.org/routing-web";

async fn register_verified(server: &axum_test::TestServer, db: &DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    mark_email_verified(db, body["user_id"].as_i64().unwrap() as i32).await;
    body["token"].as_str().unwrap().to_string()
}

/// `(id, code)` of a fresh link.
async fn create_link(server: &axum_test::TestServer, token: &str) -> (i32, String) {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({
            "original_url": "https://iana.org/routing-fallback",
            "custom_alias": unique_code(),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

async fn put_rules(
    server: &axum_test::TestServer,
    token: &str,
    id: i32,
    rules: Value,
) -> axum_test::TestResponse {
    server
        .put(&format!("/links/{id}/rules"))
        .authorization_bearer(token)
        .json(&json!({ "rules": rules }))
        .await
}

async fn stored_rules(server: &axum_test::TestServer, token: &str, id: i32) -> Vec<Value> {
    let res = server
        .get(&format!("/links/{id}/rules"))
        .authorization_bearer(token)
        .await;
    assert_eq!(res.status_code(), 200, "GET rules: {}", res.text());
    res.json::<Vec<Value>>()
}

async fn landing_spot(server: &axum_test::TestServer, code: &str, user_agent: &str) -> String {
    let res = server
        .get(&format!("/{code}"))
        .add_header(header::USER_AGENT, user_agent)
        .await;
    assert_eq!(res.status_code(), 307, "redirect: {}", res.text());
    res.header(header::LOCATION)
        .to_str()
        .expect("utf-8 location")
        .to_string()
}

/// "iPhone -> app, everyone else -> web" saved exactly as the editor sends it
/// (priority = position in the list) must never send an iPhone to the web page.
#[tokio::test]
async fn editor_order_is_the_priority() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([
            { "match_device": "Mobile", "destination_url": MOBILE, "weight": 1, "priority": 0 },
            { "destination_url": WEB, "weight": 1, "priority": 1 },
        ]),
    )
    .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());

    for _ in 0..40 {
        assert_eq!(landing_spot(&server, &code, IPHONE_UA).await, MOBILE);
    }
    for _ in 0..5 {
        assert_eq!(landing_spot(&server, &code, DESKTOP_UA).await, WEB);
    }
}

/// An API client that omits `priority` gets the order it wrote: the first
/// matching rule wins, instead of both rules landing on priority 0 and a coin
/// flip between them.
#[tokio::test]
async fn omitted_priority_defaults_to_list_position() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([
            { "match_device": "Mobile", "destination_url": MOBILE, "weight": 1 },
            { "destination_url": WEB, "weight": 1 },
        ]),
    )
    .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());

    let stored = stored_rules(&server, &token, id).await;
    let priorities: Vec<i64> = stored
        .iter()
        .map(|r| r["priority"].as_i64().unwrap())
        .collect();
    assert_eq!(priorities, vec![0, 1], "{stored:?}");

    for _ in 0..40 {
        assert_eq!(landing_spot(&server, &code, IPHONE_UA).await, MOBILE);
    }
}

/// A client that deliberately sends equal priorities still gets a weighted
/// A/B split between the tied rules.
#[tokio::test]
async fn equal_priorities_split_traffic_by_weight() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([
            { "destination_url": MOBILE, "weight": 1, "priority": 0 },
            { "destination_url": WEB, "weight": 1, "priority": 0 },
        ]),
    )
    .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());

    // 80 fair coin flips all landing on one side: odds of 2^-79.
    let mut seen = std::collections::HashSet::new();
    for _ in 0..80 {
        seen.insert(landing_spot(&server, &code, DESKTOP_UA).await);
    }
    assert_eq!(seen.len(), 2, "both A and B must be served, got {seen:?}");
    assert!(seen.contains(MOBILE) && seen.contains(WEB));
}

/// Rules come back ordered by priority, and ties in the order they were saved,
/// so the editor shows the list the way it was written.
#[tokio::test]
async fn rules_are_listed_by_priority_then_save_order() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, _code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([
            { "destination_url": "https://iana.org/r-c", "priority": 2 },
            { "destination_url": "https://iana.org/r-a1", "priority": 0 },
            { "destination_url": "https://iana.org/r-b", "priority": 1 },
            { "destination_url": "https://iana.org/r-a2", "priority": 0 },
        ]),
    )
    .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());

    let order: Vec<String> = stored_rules(&server, &token, id)
        .await
        .iter()
        .map(|r| r["destination_url"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        order,
        [
            "https://iana.org/r-a1",
            "https://iana.org/r-a2",
            "https://iana.org/r-b",
            "https://iana.org/r-c",
        ]
    );
}

fn rejected_rules() -> Vec<(&'static str, Value)> {
    vec![
        (
            "unknown device",
            json!([{ "destination_url": MOBILE, "match_device": "Phone" }]),
        ),
        (
            "country is not alpha-2",
            json!([{ "destination_url": MOBILE, "match_country": "Germany" }]),
        ),
        (
            "country with digits",
            json!([{ "destination_url": MOBILE, "match_country": "U1" }]),
        ),
        (
            "regional language tag can never match",
            json!([{ "destination_url": MOBILE, "match_lang": "pt-BR" }]),
        ),
        (
            "os far too long",
            json!([{ "destination_url": MOBILE, "match_os": "x".repeat(200) }]),
        ),
        (
            "weight zero",
            json!([{ "destination_url": MOBILE, "weight": 0 }]),
        ),
        (
            "negative weight",
            json!([{ "destination_url": MOBILE, "weight": -5 }]),
        ),
        (
            "weight beyond the cap",
            json!([{ "destination_url": MOBILE, "weight": 1001 }]),
        ),
        (
            "weight that would overflow a sum",
            json!([{ "destination_url": MOBILE, "weight": i32::MAX }]),
        ),
        (
            "negative priority",
            json!([{ "destination_url": MOBILE, "priority": -1 }]),
        ),
        (
            "priority beyond the cap",
            json!([{ "destination_url": MOBILE, "priority": 1001 }]),
        ),
        (
            "more rules than a link may have",
            Value::Array(
                (0..21)
                    .map(|i| json!({ "destination_url": format!("https://iana.org/many-{i}") }))
                    .collect(),
            ),
        ),
        (
            "a later rule is invalid",
            json!([
                { "destination_url": MOBILE },
                { "destination_url": WEB, "match_device": "Fridge" },
            ]),
        ),
    ]
}

/// Every rejected request is a 400, and a rejected replacement leaves the
/// rules the link already had untouched.
#[tokio::test]
async fn invalid_rule_inputs_are_rejected_without_touching_saved_rules() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, _code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([{ "destination_url": WEB, "match_device": "Desktop" }]),
    )
    .await;
    assert_eq!(res.status_code(), 200, "seed rules: {}", res.text());

    for (what, rules) in rejected_rules() {
        // A fresh app (fresh rate limiter) per case: they would otherwise
        // share the 10-per-second API bucket. The token is valid on any app
        // over the same database.
        let (server, _) = spawn_real_app().await;
        let res = put_rules(&server, &token, id, rules).await;
        assert_eq!(res.status_code(), 400, "{what}: {}", res.text());
        let message = res.json::<Value>()["error"].as_str().unwrap().to_string();
        assert!(!message.is_empty(), "{what}: error text");
    }

    let stored = stored_rules(&server, &token, id).await;
    assert_eq!(stored.len(), 1, "{stored:?}");
    assert_eq!(stored[0]["destination_url"], WEB);
}

/// The message says which rule is wrong and why.
#[tokio::test]
async fn validation_error_names_the_rule_and_the_field() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, _code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([
            { "destination_url": MOBILE },
            { "destination_url": WEB, "match_country": "Germany" },
        ]),
    )
    .await;
    assert_eq!(res.status_code(), 400);
    let error = res.json::<Value>()["error"].as_str().unwrap().to_string();
    assert!(error.contains("Rule 2"), "{error}");
    assert!(error.contains("country"), "{error}");
}

/// Controls: the inputs the editor can produce are accepted, normalised, and
/// blank conditions stay wildcards.
#[tokio::test]
async fn valid_rule_inputs_are_accepted_and_normalised() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, _code) = create_link(&server, &token).await;

    let res = put_rules(
        &server,
        &token,
        id,
        json!([
            {
                "destination_url": MOBILE,
                "match_device": " mobile ",
                "match_os": "iOS",
                "match_country": "de ",
                "match_lang": "DE",
                "weight": 1000,
                "priority": 1000,
            },
            {
                "destination_url": WEB,
                "match_device": "",
                "match_os": "  ",
                "match_country": null,
                "match_lang": "",
                "priority": 0,
            },
        ]),
    )
    .await;
    assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());

    let stored = stored_rules(&server, &token, id).await;
    assert_eq!(stored.len(), 2, "{stored:?}");
    // Ordered by priority, so the catch-all (0) comes first.
    assert_eq!(stored[0]["destination_url"], WEB);
    assert_eq!(stored[0]["match_device"], Value::Null);
    assert_eq!(stored[0]["match_os"], Value::Null);
    assert_eq!(stored[0]["match_country"], Value::Null);
    assert_eq!(stored[0]["match_lang"], Value::Null);
    assert_eq!(stored[0]["weight"], 1);
    assert_eq!(stored[1]["match_device"], "Mobile");
    assert_eq!(stored[1]["match_os"], "iOS");
    assert_eq!(stored[1]["match_country"], "DE");
    assert_eq!(stored[1]["match_lang"], "de");
    assert_eq!(stored[1]["weight"], 1000);
    assert_eq!(stored[1]["priority"], 1000);

    // Clearing all rules is still allowed.
    let res = put_rules(&server, &token, id, json!([])).await;
    assert_eq!(res.status_code(), 200, "clear rules: {}", res.text());
    assert!(stored_rules(&server, &token, id).await.is_empty());
}

/// Destinations are held to the same checks as the link's own URL; a bad one
/// rejects the whole replacement.
#[tokio::test]
async fn private_and_malformed_destinations_are_rejected() {
    let (server, db) = spawn_real_app().await;
    let token = register_verified(&server, &db).await;
    let (id, _code) = create_link(&server, &token).await;

    for destination in [
        "http://127.0.0.1/admin",
        "http://169.254.169.254/latest/meta-data",
        "javascript:alert(1)",
        "not a url",
        "",
    ] {
        let res = put_rules(
            &server,
            &token,
            id,
            json!([{ "destination_url": destination }]),
        )
        .await;
        assert_eq!(res.status_code(), 400, "{destination:?}: {}", res.text());
    }
    assert!(stored_rules(&server, &token, id).await.is_empty());
}
