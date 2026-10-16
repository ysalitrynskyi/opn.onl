//! The two 429s that used to skip the rate-limit shape: shortening one URL
//! too often, and a wrong `X-Link-Password` on `GET /{code}`.
//!
//! Both must answer like `too_many_requests`: JSON `error`, `retry_after` and
//! `message`, plus `Retry-After` and `X-RateLimit-*`. `Retry-After` is the
//! real wait, not a fixed number.

mod common;

use chrono::{Duration, Utc};
use common::{mark_email_verified, set_env, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::{Value, json};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &sea_orm::DatabaseConnection,
) -> (i32, String) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (user_id, body["token"].as_str().unwrap().to_string())
}

fn header_secs(res: &axum_test::TestResponse, name: &str) -> u64 {
    res.header(name)
        .to_str()
        .unwrap_or_else(|_| panic!("{name} is not text"))
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not an integer"))
}

fn assert_rate_limit_shape(res: &axum_test::TestResponse, limit: u64, message: &str) {
    assert_eq!(res.status_code(), 429, "{}", res.text());
    let content_type = res.header("content-type").to_str().unwrap_or("").to_owned();
    assert!(
        content_type.starts_with("application/json"),
        "429 must be JSON, got {content_type}"
    );
    let retry_after = header_secs(res, "retry-after");
    assert!(retry_after >= 1, "Retry-After {retry_after}");
    assert_eq!(header_secs(res, "x-ratelimit-limit"), limit);
    assert_eq!(header_secs(res, "x-ratelimit-remaining"), 0);

    let body: Value = res.json();
    assert_eq!(body["error"], "Too many requests");
    assert_eq!(body["retry_after"], retry_after);
    assert_eq!(body["message"], message);
    assert_eq!(
        body.as_object().unwrap().len(),
        3,
        "429 body has only the documented fields: {body}"
    );
}

/// Seconds until `backdated + 10 minutes`, rounded up the way the handler does.
fn expected_retry(backdated: chrono::NaiveDateTime) -> u64 {
    let remaining =
        (backdated + Duration::minutes(10)).signed_duration_since(Utc::now().naive_utc());
    if remaining <= Duration::zero() {
        return 1;
    }
    let secs = u64::try_from(remaining.num_seconds()).unwrap_or(u64::MAX);
    secs.saturating_add(u64::from(remaining.subsec_nanos() > 0))
        .max(1)
}

#[tokio::test]
async fn the_same_url_is_refused_with_the_rate_limit_shape_and_a_real_wait() {
    // A developer shell may cap links far below ten. This limit is per URL.
    set_env("MAX_LINKS_PER_USER", "0");
    let (server, db) = spawn_real_app().await;
    let (user_id, token) = register_verified(&server, &db).await;
    let url = format!("https://iana.org/same-{}", uuid::Uuid::new_v4());
    // Nine minutes ago, so the refusal must wait about a minute, not the whole
    // ten. The other eight are fresh and still inside the window.
    let backdated = Utc::now().naive_utc() - Duration::minutes(9);

    for i in 0..9 {
        links::ActiveModel {
            code: Set(unique_code()),
            original_url: Set(url.clone()),
            user_id: Set(Some(user_id)),
            created_at: Set(if i == 0 {
                backdated
            } else {
                Utc::now().naive_utc()
            }),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert recent link");
    }

    let create = |body: Value| {
        server
            .post("/links")
            .authorization_bearer(&token)
            .json(&body)
    };

    // Ten is still allowed. The hourly create budget, not the same-URL cap,
    // is what this response reports.
    let tenth = create(json!({ "original_url": url })).await;
    assert_eq!(tenth.status_code(), 201, "tenth create: {}", tenth.text());
    assert_eq!(header_secs(&tenth, "x-ratelimit-limit"), 100);

    let refused = create(json!({ "original_url": url })).await;
    assert_rate_limit_shape(
        &refused,
        10,
        "You have shortened this URL too many times. Please wait a few minutes.",
    );
    let retry_after = header_secs(&refused, "retry-after");
    let expected = expected_retry(backdated);
    assert!(
        retry_after + 2 >= expected && retry_after <= expected + 2,
        "Retry-After {retry_after} should be when the nine-minute-old link leaves the window (about {expected})"
    );

    // The cap is per URL. Another destination still goes through.
    let other = format!("https://iana.org/other-{}", uuid::Uuid::new_v4());
    let allowed = create(json!({ "original_url": other })).await;
    assert_eq!(allowed.status_code(), 201, "other url: {}", allowed.text());
}

#[tokio::test]
async fn a_wrong_password_header_is_refused_with_the_rate_limit_shape() {
    let (server, db) = spawn_real_app().await;
    let (_user_id, token) = register_verified(&server, &db).await;
    let destination = format!("https://iana.org/secret-{}", uuid::Uuid::new_v4());
    let password = "correct-horse-battery-staple";

    let mut codes = Vec::new();
    for _ in 0..2 {
        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": destination, "password": password }))
            .await;
        assert_eq!(res.status_code(), 201, "create: {}", res.text());
        codes.push(res.json::<Value>()["code"].as_str().unwrap().to_string());
    }
    let spent = &codes[0];
    let untouched = &codes[1];

    let guess = |code: &str, password: Option<&str>| {
        let mut req = server.get(&format!("/{code}"));
        if let Some(password) = password {
            req = req.add_header("x-link-password", password);
        }
        req
    };

    // Five wrong headers are still answered. The sixth is the refusal.
    for attempt in 1..=5 {
        let res = guess(spent, Some("nope")).await;
        assert_eq!(res.status_code(), 401, "attempt {attempt}: {}", res.text());
        assert_eq!(res.text(), "Invalid password");
        assert!(
            !res.header("content-type")
                .to_str()
                .unwrap_or("")
                .contains("json"),
            "a wrong password stays plain text"
        );
    }
    let refused = guess(spent, Some("nope")).await;
    let retry_after = {
        assert_eq!(refused.status_code(), 429, "{}", refused.text());
        header_secs(&refused, "retry-after")
    };
    assert!(
        (1..=60).contains(&retry_after),
        "the password budget is one minute, Retry-After was {retry_after}"
    );
    assert_rate_limit_shape(
        &refused,
        5,
        &format!("Rate limit exceeded. Please try again in {retry_after} seconds."),
    );

    // No header is the password page, not a 401 and not this budget.
    let page = guess(spent, None).await;
    assert_eq!(page.status_code(), 307, "{}", page.text());
    let location = page.header("location").to_str().unwrap().to_string();
    assert!(
        location.ends_with(&format!("/password/{spent}")),
        "missing header redirects to the password page, got {location}"
    );

    // Another link keeps its own budget, and the right password still redirects.
    let other = guess(untouched, Some("nope")).await;
    assert_eq!(other.status_code(), 401, "untouched link: {}", other.text());
    let opened = guess(untouched, Some(password)).await;
    assert_eq!(
        opened.status_code(),
        307,
        "right password: {}",
        opened.text()
    );
    assert_eq!(opened.header("location").to_str().unwrap(), destination);
}
