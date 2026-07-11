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
