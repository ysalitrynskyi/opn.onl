//! Regression test for the passkey cred_id data-integrity finding: the column
//! had no UNIQUE constraint, so a re-registered credential could land as a
//! duplicate row. Migration m20220101_000029 adds a unique index; this pins it.
//! (The register_finish HTTP path also rejects a known cred_id with 409, but the
//! WebAuthn ceremony can't run headless, so the DB constraint — the real safety
//! net — is what's tested here.)

mod common;

use common::{spawn_real_app, unique_email};
use opn_onl_backend::entity::passkeys;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::{json, Value};

fn passkey(user_id: i32, cred_id: &str) -> passkeys::ActiveModel {
    passkeys::ActiveModel {
        user_id: Set(user_id),
        cred_id: Set(cred_id.to_string()),
        cred_public_key: Set("test-public-key".to_string()),
        counter: Set(0),
        name: Set(Some("test".to_string())),
        created_at: Set(chrono::Utc::now().naive_utc()),
        last_used: Set(None),
        ..Default::default()
    }
}

#[tokio::test]
async fn passkey_cred_id_must_be_unique() {
    let (server, db) = spawn_real_app().await;
    let reg = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(reg.status_code(), 201, "register: {}", reg.text());
    let user_id = reg.json::<Value>()["user_id"].as_i64().unwrap() as i32;

    let cred_id = format!("unique-cred-{user_id}");
    passkey(user_id, &cred_id)
        .insert(&db)
        .await
        .expect("first passkey insert should succeed");

    // A second row with the same cred_id must be rejected by the unique index.
    let dup = passkey(user_id, &cred_id).insert(&db).await;
    assert!(
        dup.is_err(),
        "duplicate cred_id must be rejected by the unique index, but the insert succeeded"
    );
}

fn passkey_login_start_shape(body: &Value) -> (bool, Option<&Vec<Value>>) {
    let public_key = body.get("options").and_then(|o| o.get("publicKey"));
    let has_challenge = public_key.and_then(|pk| pk.get("challenge")).is_some();
    let allow = public_key
        .and_then(|pk| pk.get("allowCredentials"))
        .and_then(|a| a.as_array());
    (has_challenge, allow)
}

#[tokio::test]
async fn passkey_login_start_does_not_enumerate_users() {
    std::env::set_var("ENABLE_PASSKEYS", "true");
    let (server, db) = spawn_real_app().await;

    let no_passkey_email = unique_email();
    let no_passkey_reg = server
        .post("/auth/register")
        .json(&json!({ "email": &no_passkey_email, "password": "password123" }))
        .await;
    assert_eq!(
        no_passkey_reg.status_code(),
        201,
        "register: {}",
        no_passkey_reg.text()
    );

    let with_passkey_email = unique_email();
    let with_passkey_reg = server
        .post("/auth/register")
        .json(&json!({ "email": &with_passkey_email, "password": "password123" }))
        .await;
    assert_eq!(
        with_passkey_reg.status_code(),
        201,
        "register: {}",
        with_passkey_reg.text()
    );
    let with_passkey_id = with_passkey_reg.json::<Value>()["user_id"]
        .as_i64()
        .unwrap() as i32;
    passkey(with_passkey_id, &format!("audit-pk-{with_passkey_id}"))
        .insert(&db)
        .await
        .expect("insert passkey");

    let unknown_email = unique_email();

    let post = |email: &str| {
        server
            .post("/auth/passkey/login/start")
            .json(&json!({ "username": email }))
    };

    let unknown = post(&unknown_email).await;
    let no_passkey = post(&no_passkey_email).await;
    let with_passkey = post(&with_passkey_email).await;

    assert_eq!(unknown.status_code(), 200, "unknown: {}", unknown.text());
    assert_eq!(
        no_passkey.status_code(),
        200,
        "no passkeys: {}",
        no_passkey.text()
    );
    assert_eq!(
        with_passkey.status_code(),
        200,
        "with passkeys: {}",
        with_passkey.text()
    );

    let unknown_body: Value = unknown.json();
    let no_passkey_body: Value = no_passkey.json();
    let with_passkey_body: Value = with_passkey.json();

    let (unknown_challenge, unknown_allow) = passkey_login_start_shape(&unknown_body);
    let (no_passkey_challenge, no_passkey_allow) = passkey_login_start_shape(&no_passkey_body);
    let (with_passkey_challenge, with_passkey_allow) =
        passkey_login_start_shape(&with_passkey_body);

    assert!(
        unknown_challenge && unknown_allow.is_some(),
        "unknown must look like a WebAuthn challenge: {unknown_body}"
    );
    assert_eq!(
        (no_passkey_challenge, no_passkey_allow.is_some()),
        (unknown_challenge, unknown_allow.is_some()),
        "no-passkey vs unknown must be indistinguishable"
    );
    assert_eq!(
        (with_passkey_challenge, with_passkey_allow.is_some()),
        (unknown_challenge, unknown_allow.is_some()),
        "with-passkey vs unknown must be indistinguishable"
    );

    let unknown_again: Value = post(&unknown_email).await.json();
    assert_eq!(
        unknown_again["options"]["publicKey"]["allowCredentials"],
        unknown_body["options"]["publicKey"]["allowCredentials"],
        "decoy allowCredentials must be deterministic for a username"
    );

    let finish = server
        .post("/auth/passkey/login/finish")
        .json(&json!({
            "username": unknown_email,
            "credential": {
                "id": "AAAA",
                "rawId": "AAAA",
                "type": "public-key",
                "response": {
                    "authenticatorData": "AAAA",
                    "clientDataJSON": "AAAA",
                    "signature": "AAAA"
                }
            }
        }))
        .await;
    assert_eq!(
        finish.status_code(),
        400,
        "unknown user must not leave pending auth state: {}",
        finish.text()
    );
    assert!(
        finish.text().contains("Authentication state not found"),
        "unknown finish: {}",
        finish.text()
    );
}
