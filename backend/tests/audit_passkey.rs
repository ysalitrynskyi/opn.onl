//! Passkey regression tests.
//!
//! The first test is the data-integrity finding on `cred_id`: the column had no
//! UNIQUE constraint, so a re-registered credential could land as a duplicate
//! row. Migration m20220101_000029 adds a unique index; the test pins it.
//!
//! The rest act out the real WebAuthn ceremonies with a software authenticator
//! (`tests/common/soft_passkey.rs`), because a passkey the server cannot parse
//! behaves like no passkey at all, and a test built on one proves nothing about
//! the real path: login start answers a decoy for it, so a "real vs decoy"
//! comparison ends up comparing the decoy with itself.

mod common;
#[path = "common/soft_passkey.rs"]
mod soft_passkey;

use std::sync::Arc;

use axum_test::{TestResponse, TestServer};
use common::{mark_email_verified, unique_email};
use opn_onl_backend::entity::passkeys;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};
use soft_passkey::{SoftPasskey, junk_assertion};

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
    let (server, db) = app().await;
    let account = account(&server, &db).await;

    let cred_id = format!("unique-cred-{}", account.id);
    passkey(account.id, &cred_id)
        .insert(&db)
        .await
        .expect("first passkey insert should succeed");

    // A second row with the same cred_id must be rejected by the unique index.
    let dup = passkey(account.id, &cred_id).insert(&db).await;
    assert!(
        dup.is_err(),
        "duplicate cred_id must be rejected by the unique index, but the insert succeeded"
    );
}

// ---------------------------------------------------------------- fixtures

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

/// The real router. The sign-in budget (10 a minute) would stop a test that
/// walks through whole ceremonies, and it is not what these tests are about.
async fn app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    soft_passkey::pin_webauthn_environment();
    let db = common::setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        auth: roomy(),
        ..Default::default()
    });
    let server =
        TestServer::new(opn_onl_backend::build_router(state)).expect("failed to start test server");
    (server, db)
}

struct Account {
    id: i32,
    email: String,
    token: String,
}

/// A registered account with a verified address (enrolling a passkey needs one).
async fn account(server: &TestServer, db: &DatabaseConnection) -> Account {
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": &email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let id = body["user_id"].as_i64().expect("user_id") as i32;
    mark_email_verified(db, id).await;
    Account {
        id,
        email,
        token: body["token"].as_str().expect("token").to_string(),
    }
}

/// Run the registration ceremony for `account` with `authenticator`.
async fn enroll(
    server: &TestServer,
    account: &Account,
    authenticator: &SoftPasskey,
) -> TestResponse {
    let start = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(&account.token)
        .json(&json!({ "username": &account.email }))
        .await;
    assert_eq!(start.status_code(), 200, "register start: {}", start.text());
    let options: Value = start.json::<Value>()["options"].clone();
    server
        .post("/auth/passkey/register/finish")
        .authorization_bearer(&account.token)
        .json(&json!({
            "username": &account.email,
            "credential": authenticator.registration(&options),
        }))
        .await
}

/// An account that has registered one passkey.
async fn enrolled(server: &TestServer, db: &DatabaseConnection) -> (Account, SoftPasskey) {
    let account = account(server, db).await;
    let authenticator = SoftPasskey::new();
    let res = enroll(server, &account, &authenticator).await;
    assert_eq!(res.status_code(), 200, "register finish: {}", res.text());
    (account, authenticator)
}

async fn login_start(server: &TestServer, email: &str) -> Value {
    let res = server
        .post("/auth/passkey/login/start")
        .json(&json!({ "username": email }))
        .await;
    assert_eq!(res.status_code(), 200, "login start: {}", res.text());
    res.json()
}

fn login_id(start: &Value) -> &str {
    start["login_id"]
        .as_str()
        .expect("login_id in the response")
}

async fn login_finish(server: &TestServer, login_id: &str, credential: Value) -> TestResponse {
    server
        .post("/auth/passkey/login/finish")
        .json(&json!({ "login_id": login_id, "credential": credential }))
        .await
}

/// Status and body, to compare answers.
fn answer(res: &TestResponse) -> (u16, String) {
    (res.status_code().as_u16(), res.text())
}

async fn stored_passkeys(db: &DatabaseConnection, user_id: i32) -> Vec<passkeys::Model> {
    passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .all(db)
        .await
        .expect("list passkeys")
}

/// A start response with everything random or per-account replaced by its
/// length, so two responses compare equal exactly when they look alike.
fn shape(start: &Value) -> Value {
    let mut shape = start.clone();
    let public_key = &mut shape["options"]["publicKey"];
    let length = |value: &Value| json!(value.as_str().expect("a string").len());
    public_key["challenge"] = length(&public_key["challenge"]);
    for credential in public_key["allowCredentials"]
        .as_array_mut()
        .expect("allowCredentials")
    {
        credential["id"] = length(&credential["id"]);
    }
    shape["login_id"] = length(&shape["login_id"]);
    shape
}

// ------------------------------------------------------------ decoy vs real

/// An address with a passkey, one without and one nobody registered must get
/// the same-looking challenge. The decoy used to say `timeout: 60000` and the
/// host of `FRONTEND_URL` as `rpId`, where a real challenge says 300000 and the
/// RP ID, so one request told an enrolled address from any other.
#[tokio::test]
async fn passkey_login_start_does_not_enumerate_users() {
    let (server, db) = app().await;

    let (enrolled_account, authenticator) = enrolled(&server, &db).await;
    let plain_account = account(&server, &db).await;
    let unknown_email = unique_email();

    let real = login_start(&server, &enrolled_account.email).await;
    let no_passkey = login_start(&server, &plain_account.email).await;
    let unknown = login_start(&server, &unknown_email).await;

    // The real challenge is the baseline: it must really be one.
    assert_eq!(real["options"]["publicKey"]["timeout"], 300_000, "{real}");
    assert_eq!(real["options"]["publicKey"]["rpId"], soft_passkey::RP_ID);
    assert_eq!(
        real["options"]["publicKey"]["allowCredentials"][0]["id"],
        json!(authenticator.id_text()),
        "the enrolled address gets its own credential: {real}"
    );

    assert_eq!(shape(&no_passkey), shape(&real), "no passkey vs real");
    assert_eq!(shape(&unknown), shape(&real), "unknown vs real");

    // A decoy is stable per address, so repeated asks do not give it away.
    let unknown_again = login_start(&server, &unknown_email).await;
    assert_eq!(
        unknown_again["options"]["publicKey"]["allowCredentials"],
        unknown["options"]["publicKey"]["allowCredentials"],
        "decoy allowCredentials must be deterministic for a username"
    );
}

/// After `start`, `finish` must not say whether the attempt was real: an
/// attempt that does not exist, one made for an unknown address and one whose
/// assertion fails to verify all get the same answer.
#[tokio::test]
async fn login_finish_answers_every_failure_the_same_way() {
    let (server, db) = app().await;
    let (account, _) = enrolled(&server, &db).await;

    // A login_id nobody was given.
    let invented = login_finish(&server, "invented-by-the-caller", junk_assertion()).await;
    // A decoy attempt: an address that is not registered.
    let decoy = login_start(&server, &unique_email()).await;
    let for_decoy = login_finish(&server, login_id(&decoy), junk_assertion()).await;
    // A real attempt whose assertion does not verify.
    let real = login_start(&server, &account.email).await;
    let bad_assertion = login_finish(&server, login_id(&real), junk_assertion()).await;

    assert_eq!(bad_assertion.status_code(), 401, "{}", bad_assertion.text());
    assert_eq!(answer(&invented), answer(&bad_assertion), "invented id");
    assert_eq!(answer(&for_decoy), answer(&bad_assertion), "decoy id");
}

// ------------------------------------------------- attempts belong to a browser

/// Two sign-ins as the same user (two tabs, two devices) must not clobber each
/// other: the pending challenge used to be kept per email, so the second
/// `start` replaced the first and the first browser's answer was refused.
#[tokio::test]
async fn a_second_start_for_the_same_address_does_not_invalidate_the_first() {
    let (server, db) = app().await;
    let (account, mut authenticator) = enrolled(&server, &db).await;

    let first = login_start(&server, &account.email).await;
    let second = login_start(&server, &account.email).await;
    assert_ne!(login_id(&first), login_id(&second));

    let first_answer = authenticator.assertion(&first["options"]);
    let res = login_finish(&server, login_id(&first), first_answer).await;
    assert_eq!(res.status_code(), 200, "first attempt: {}", res.text());
    assert!(res.json::<Value>()["token"].is_string());

    let second_answer = authenticator.assertion(&second["options"]);
    let res = login_finish(&server, login_id(&second), second_answer).await;
    assert_eq!(res.status_code(), 200, "second attempt: {}", res.text());
}

/// Someone who knows only the victim's address can neither consume nor replace
/// the victim's pending challenge: it is kept under an id only the browser that
/// started the attempt holds.
#[tokio::test]
async fn a_stranger_cannot_use_up_someone_elses_pending_login() {
    let (server, db) = app().await;
    let (victim, mut authenticator) = enrolled(&server, &db).await;

    let pending = login_start(&server, &victim.email).await;

    // The attacker starts attempts of their own for the victim's address and
    // finishes with whatever they can think of.
    let attackers = login_start(&server, &victim.email).await;
    let guess = login_finish(&server, &victim.email, junk_assertion()).await;
    let theirs = login_finish(&server, login_id(&attackers), junk_assertion()).await;
    let old_style = server
        .post("/auth/passkey/login/finish")
        .json(&json!({ "username": &victim.email, "credential": junk_assertion() }))
        .await;
    for res in [&guess, &theirs] {
        assert_eq!(res.status_code(), 401, "{}", res.text());
    }
    assert!(
        old_style.status_code().is_client_error(),
        "finish without a login_id must be refused: {}",
        old_style.text()
    );

    let answer = authenticator.assertion(&pending["options"]);
    let res = login_finish(&server, login_id(&pending), answer).await;
    assert_eq!(
        res.status_code(),
        200,
        "the victim signs in: {}",
        res.text()
    );
}

/// A challenge is good for one answer, right or wrong, so a captured
/// assertion cannot be replayed under the same id.
#[tokio::test]
async fn a_pending_login_can_be_answered_once() {
    let (server, db) = app().await;
    let (account, mut authenticator) = enrolled(&server, &db).await;

    let start = login_start(&server, &account.email).await;
    let answer = authenticator.assertion(&start["options"]);
    let first = login_finish(&server, login_id(&start), answer.clone()).await;
    assert_eq!(first.status_code(), 200, "{}", first.text());
    let replay = login_finish(&server, login_id(&start), answer).await;
    assert_eq!(replay.status_code(), 401, "{}", replay.text());

    // A wrong answer uses the attempt up as well.
    let start = login_start(&server, &account.email).await;
    let wrong = login_finish(&server, login_id(&start), junk_assertion()).await;
    assert_eq!(wrong.status_code(), 401);
    let right = authenticator.assertion(&start["options"]);
    let late = login_finish(&server, login_id(&start), right).await;
    assert_eq!(late.status_code(), 401, "{}", late.text());
}

// --------------------------------------------------------- how ids are stored

/// The `Debug` text the credential id was stored as before it was written as
/// base64url: `HumanBinaryData([1, 2, 3])`.
fn legacy_key(authenticator: &SoftPasskey) -> String {
    format!("HumanBinaryData({:?})", authenticator.id)
}

/// New passkeys are stored under the base64url id a browser reports, not the
/// `Debug` text of the library's type, which a dependency upgrade may change.
#[tokio::test]
async fn a_new_passkey_is_stored_under_its_base64url_id() {
    let (server, db) = app().await;
    let (account, authenticator) = enrolled(&server, &db).await;

    let rows = stored_passkeys(&db, account.id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cred_id, authenticator.id_text());
}

/// Rows written before that hold the old `Debug` text. They are live
/// passkeys, so signing in with one must keep working, and so must the
/// duplicate check.
#[tokio::test]
async fn passkeys_stored_in_the_old_format_still_sign_in() {
    let (server, db) = app().await;
    let (account, mut authenticator) = enrolled(&server, &db).await;

    let row = stored_passkeys(&db, account.id).await.remove(0);
    let mut legacy: passkeys::ActiveModel = row.into();
    legacy.cred_id = Set(legacy_key(&authenticator));
    legacy.update(&db).await.expect("rewrite as an old row");

    let start = login_start(&server, &account.email).await;
    let answer = authenticator.assertion(&start["options"]);
    let res = login_finish(&server, login_id(&start), answer).await;
    assert_eq!(res.status_code(), 200, "old-format row: {}", res.text());

    let rows = stored_passkeys(&db, account.id).await;
    assert!(rows[0].last_used.is_some(), "the use is recorded");
    assert_eq!(rows[0].counter, 1, "the counter moves");

    // Registering the same authenticator again is refused whichever way the
    // first copy is stored.
    let again = enroll(&server, &account, &authenticator).await;
    assert_eq!(again.status_code(), 409, "{}", again.text());
    assert_eq!(stored_passkeys(&db, account.id).await.len(), 1);
}

/// Control for the above: registering the same authenticator twice with the
/// new format is refused too.
#[tokio::test]
async fn the_same_passkey_cannot_be_registered_twice() {
    let (server, db) = app().await;
    let (account, authenticator) = enrolled(&server, &db).await;

    let again = enroll(&server, &account, &authenticator).await;
    assert_eq!(again.status_code(), 409, "{}", again.text());
    assert_eq!(stored_passkeys(&db, account.id).await.len(), 1);
}
