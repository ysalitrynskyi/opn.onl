//! The per-link guess budget on `POST /{code}/verify` (5 a minute).
//!
//! The rate-limit middleware keyed it on the raw path while the handler looks
//! the link up by the decoded code, so `/%61bcde/verify` and `/abcde/verify`
//! guessed at the same link from separate budgets. Every spelling of the code
//! minted a fresh one, and the 5-a-minute limit was really the much larger
//! per-IP one.

use crate::common;

use common::{mark_email_verified, spawn_real_app, unique_email};
use serde_json::{Value, json};

async fn password_link(server: &axum_test::TestServer, db: &sea_orm::DatabaseConnection) -> String {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let token = body["token"].as_str().unwrap().to_string();
    mark_email_verified(db, body["user_id"].as_i64().unwrap() as i32).await;

    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/secret",
            "password": "correct-horse-battery-staple"
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    res.json::<Value>()["code"].as_str().unwrap().to_string()
}

/// `abcde` -> `%61bcde`: the first character percent-encoded.
fn encode_first_char(code: &str) -> String {
    format!("%{:02X}{}", code.as_bytes()[0], &code[1..])
}

async fn guess(server: &axum_test::TestServer, path: &str) -> u16 {
    server
        .post(path)
        .json(&json!({ "password": "definitely-wrong" }))
        .await
        .status_code()
        .as_u16()
}

#[tokio::test]
async fn an_encoded_spelling_of_the_code_shares_the_per_link_budget() {
    let (server, db) = spawn_real_app().await;
    let code = password_link(&server, &db).await;

    // Five wrong guesses use up the link's budget...
    for attempt in 1..=5 {
        assert_eq!(
            guess(&server, &format!("/{code}/verify")).await,
            401,
            "attempt {attempt}"
        );
    }
    assert_eq!(
        guess(&server, &format!("/{code}/verify")).await,
        429,
        "the sixth guess in a minute is refused"
    );

    // ...and spelling the same code differently does not buy a new one.
    let encoded = encode_first_char(&code);
    assert_eq!(
        guess(&server, &format!("/{encoded}/verify")).await,
        429,
        "/{encoded}/verify is the same link as /{code}/verify"
    );
    let fully_encoded: String = code.bytes().map(|b| format!("%{b:02X}")).collect();
    assert_eq!(
        guess(&server, &format!("/{fully_encoded}/verify")).await,
        429,
        "a fully percent-encoded code is the same link too"
    );
}

/// Control: the limit is per link. Another link's budget is untouched, and the
/// encoded spelling does not make guesses fail that would have been allowed.
#[tokio::test]
async fn another_link_keeps_its_own_budget() {
    let (server, db) = spawn_real_app().await;
    let spent = password_link(&server, &db).await;
    let untouched = password_link(&server, &db).await;

    for _ in 0..5 {
        assert_eq!(guess(&server, &format!("/{spent}/verify")).await, 401);
    }
    assert_eq!(guess(&server, &format!("/{spent}/verify")).await, 429);

    assert_eq!(guess(&server, &format!("/{untouched}/verify")).await, 401);
    let encoded = encode_first_char(&untouched);
    assert_eq!(guess(&server, &format!("/{encoded}/verify")).await, 401);
}
