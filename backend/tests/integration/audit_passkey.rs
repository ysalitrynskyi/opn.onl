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

use crate::common;
use crate::common::soft_passkey;

use std::sync::Arc;

use axum_test::{TestResponse, TestServer};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::{mark_email_verified, unique_email};
use opn_onl_backend::entity::{passkeys, users};
use opn_onl_backend::handlers::passkeys::MAX_PASSKEYS_PER_ACCOUNT;
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

/// Every credential id a start response lists, in order.
fn listed_ids(start: &Value) -> Vec<String> {
    start["options"]["publicKey"]["allowCredentials"]
        .as_array()
        .expect("allowCredentials")
        .iter()
        .map(|credential| credential["id"].as_str().expect("an id").to_string())
        .collect()
}

/// An account that has registered `count` passkeys.
async fn enrolled_with(
    server: &TestServer,
    db: &DatabaseConnection,
    count: usize,
) -> (Account, Vec<SoftPasskey>) {
    let account = account(server, db).await;
    let mut authenticators = Vec::new();
    for n in 0..count {
        let authenticator = SoftPasskey::new();
        let res = enroll(server, &account, &authenticator).await;
        assert_eq!(res.status_code(), 200, "passkey {n}: {}", res.text());
        authenticators.push(authenticator);
    }
    (account, authenticators)
}

/// A JSON document with each value replaced by its kind and the keys of every
/// object kept in the order they were sent. `serde_json::Value` sorts keys, so
/// it cannot show two answers that differ only in order.
#[derive(Debug, PartialEq)]
enum Shape {
    Object(Vec<(String, Shape)>),
    Array(Vec<Shape>),
    String,
    Number,
    Bool,
    Null,
}

impl<'de> serde::Deserialize<'de> for Shape {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ShapeVisitor;
        impl<'de> serde::de::Visitor<'de> for ShapeVisitor {
            type Value = Shape;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_bool<E>(self, _: bool) -> Result<Shape, E> {
                Ok(Shape::Bool)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Shape, E> {
                Ok(Shape::Number)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Shape, E> {
                Ok(Shape::Number)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Shape, E> {
                Ok(Shape::Number)
            }
            fn visit_str<E>(self, _: &str) -> Result<Shape, E> {
                Ok(Shape::String)
            }
            fn visit_unit<E>(self) -> Result<Shape, E> {
                Ok(Shape::Null)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Shape, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Shape::Array(items))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Shape, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, Shape>()? {
                    entries.push(entry);
                }
                Ok(Shape::Object(entries))
            }
        }
        deserializer.deserialize_any(ShapeVisitor)
    }
}

/// Store `transports` on the user's passkeys, as webauthn-rs does when an
/// authenticator with packed or TPM attestation reports them at registration
/// (the software authenticator attests with `none`, whose transports are
/// discarded).
async fn give_transports(db: &DatabaseConnection, user_id: i32) {
    for row in stored_passkeys(db, user_id).await {
        let mut blob: Value = serde_json::from_str(&row.cred_public_key).expect("passkey blob");
        blob["cred"]["transports"] = json!(["internal", "hybrid"]);
        let mut row: passkeys::ActiveModel = row.into();
        row.cred_public_key = Set(blob.to_string());
        row.update(db).await.expect("store transports");
    }
}

async fn disable(db: &DatabaseConnection, user_id: i32) {
    users::ActiveModel {
        id: Set(user_id),
        disabled_at: Set(Some(chrono::Utc::now().naive_utc())),
        ..Default::default()
    }
    .update(db)
    .await
    .expect("disable the account");
}

// ------------------------------------------------------------ decoy vs real

/// Whatever the address, `login/start` must answer in one shape: the same keys
/// in the same order at every level, exactly `MAX_PASSKEYS_PER_ACCOUNT`
/// credentials of `type` and `id` alone, and the same timeout, RP ID and
/// policy. The decoy used to list one 32-byte id where an account lists one
/// id per passkey, with `transports` when the authenticator reported them,
/// so the answer told whether an address had passkeys and how many.
#[tokio::test]
async fn every_login_start_answer_has_one_shape() {
    let (server, db) = app().await;

    let unknown = unique_email();
    let (disabled, disabled_passkeys) = enrolled_with(&server, &db, 1).await;
    disable(&db, disabled.id).await;
    let no_passkey = account(&server, &db).await;
    let (one, mut one_passkey) = enrolled_with(&server, &db, 1).await;
    give_transports(&db, one.id).await;
    let (three, three_passkeys) = enrolled_with(&server, &db, 3).await;

    let ids = |authenticators: &[SoftPasskey]| -> Vec<String> {
        authenticators.iter().map(SoftPasskey::id_text).collect()
    };
    let cases = [
        ("unknown address", unknown, vec![]),
        ("disabled account", disabled.email.clone(), vec![]),
        ("no passkey", no_passkey.email, vec![]),
        ("one passkey", one.email.clone(), ids(&one_passkey)),
        ("three passkeys", three.email, ids(&three_passkeys)),
    ];

    let mut baseline: Option<(&str, Shape)> = None;
    for (case, email, real) in &cases {
        let res = server
            .post("/auth/passkey/login/start")
            .json(&json!({ "username": email }))
            .await;
        assert_eq!(res.status_code(), 200, "{case}: {}", res.text());
        let text = res.text();
        let body: Value = serde_json::from_str(&text).expect("JSON");
        let options = &body["options"]["publicKey"];

        assert_eq!(options["timeout"], 300_000, "{case}: {body}");
        assert_eq!(options["rpId"], soft_passkey::RP_ID, "{case}: {body}");
        assert_eq!(options["userVerification"], "required", "{case}: {body}");
        let credentials = options["allowCredentials"].as_array().expect("allow list");
        assert_eq!(
            credentials.len(),
            MAX_PASSKEYS_PER_ACCOUNT,
            "{case}: every answer lists the same number of credentials: {body}"
        );
        for credential in credentials {
            let keys: Vec<&String> = credential.as_object().expect("an object").keys().collect();
            assert_eq!(keys, ["id", "type"], "{case}: no transports: {body}");
            assert_eq!(credential["type"], "public-key", "{case}");
            let id = URL_SAFE_NO_PAD
                .decode(credential["id"].as_str().expect("an id"))
                .expect("base64url id");
            assert!(
                (16..=1023).contains(&id.len()),
                "{case}: a credential id of {} bytes",
                id.len()
            );
        }

        let listed = listed_ids(&body);
        for id in real {
            assert!(listed.contains(id), "{case}: its own passkey is listed");
        }
        let again = login_start(&server, email).await;
        assert_eq!(
            listed_ids(&again),
            listed,
            "{case}: the same list each time"
        );

        let shape: Shape = serde_json::from_str(&text).expect("shape");
        match &baseline {
            None => baseline = Some((case, shape)),
            Some((first, expected)) => {
                assert_eq!(&shape, expected, "{case} vs {first}: {text}")
            }
        }
    }

    // A disabled account gets decoys only, not its own passkey.
    let disabled_answer = login_start(&server, &disabled.email).await;
    assert!(!listed_ids(&disabled_answer).contains(&disabled_passkeys[0].id_text()));

    // The passkey whose transports were dropped from the list still signs in.
    let start = login_start(&server, &one.email).await;
    let answer = one_passkey[0].assertion(&start["options"]);
    let res = login_finish(&server, login_id(&start), answer).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
}

/// The account's own ids are mixed in among the decoys at slots fixed per
/// address, not put first, where they would stand out from the padding.
#[tokio::test]
async fn real_passkeys_are_not_always_listed_first() {
    let (server, db) = app().await;
    let mut slots = std::collections::BTreeSet::new();
    for _ in 0..8 {
        let (account, authenticators) = enrolled_with(&server, &db, 1).await;
        let start = login_start(&server, &account.email).await;
        let slot = listed_ids(&start)
            .iter()
            .position(|id| *id == authenticators[0].id_text())
            .expect("the passkey is listed");
        slots.insert(slot);
    }
    assert!(slots.len() > 1, "every real id sat in slot {slots:?}");
}

/// Each of an account's passkeys signs in through the padded list.
#[tokio::test]
async fn every_passkey_signs_in_through_the_padded_list() {
    let (server, db) = app().await;
    let (account, mut authenticators) = enrolled_with(&server, &db, 3).await;
    for authenticator in &mut authenticators {
        let start = login_start(&server, &account.email).await;
        assert_eq!(listed_ids(&start).len(), MAX_PASSKEYS_PER_ACCOUNT);
        let answer = authenticator.assertion(&start["options"]);
        let res = login_finish(&server, login_id(&start), answer).await;
        assert_eq!(res.status_code(), 200, "{}", res.text());
        assert!(res.json::<Value>()["token"].is_string());
    }
}

// ------------------------------------------------------------- the cap

/// One passkey past the cap is refused before a ceremony starts, with a JSON
/// error that says why.
#[tokio::test]
async fn an_account_holds_at_most_the_cap() {
    let (server, db) = app().await;
    let (account, _) = enrolled_with(&server, &db, MAX_PASSKEYS_PER_ACCOUNT).await;

    let res = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(&account.token)
        .json(&json!({ "username": &account.email }))
        .await;
    assert_eq!(res.status_code(), 403, "{}", res.text());
    let error = res.json::<Value>()["error"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        error.contains(&format!("maximum of {MAX_PASSKEYS_PER_ACCOUNT} passkeys")),
        "{error}"
    );
    assert_eq!(
        stored_passkeys(&db, account.id).await.len(),
        MAX_PASSKEYS_PER_ACCOUNT
    );
}

/// The cap holds for a ceremony that started below it, when the account
/// reached it before the ceremony finished.
#[tokio::test]
async fn the_cap_is_checked_again_when_registration_finishes() {
    let (server, db) = app().await;
    let (account, _) = enrolled_with(&server, &db, MAX_PASSKEYS_PER_ACCOUNT - 1).await;

    let start = server
        .post("/auth/passkey/register/start")
        .authorization_bearer(&account.token)
        .json(&json!({ "username": &account.email }))
        .await;
    assert_eq!(start.status_code(), 200, "{}", start.text());
    let options = start.json::<Value>()["options"].clone();

    // The last free place is taken meanwhile. The id has a length real
    // passkeys use, so it does not change the lengths decoys draw from.
    passkey(account.id, &SoftPasskey::new().id_text())
        .insert(&db)
        .await
        .expect("fill the last place");

    let res = server
        .post("/auth/passkey/register/finish")
        .authorization_bearer(&account.token)
        .json(&json!({
            "username": &account.email,
            "credential": SoftPasskey::new().registration(&options),
        }))
        .await;
    assert_eq!(res.status_code(), 403, "{}", res.text());
    assert_eq!(
        stored_passkeys(&db, account.id).await.len(),
        MAX_PASSKEYS_PER_ACCOUNT
    );
}

/// An account that had more passkeys than the cap before it existed keeps
/// signing in with every one of them: its list holds them all, padded to the
/// next multiple of the cap.
#[tokio::test]
async fn an_account_over_the_cap_signs_in_with_every_passkey() {
    let (server, db) = app().await;
    let (account, mut authenticators) = enrolled_with(&server, &db, MAX_PASSKEYS_PER_ACCOUNT).await;
    // Registration now refuses an eleventh, so move one in from another account.
    let (donor, donated) = enrolled_with(&server, &db, 1).await;
    let row = stored_passkeys(&db, donor.id).await.remove(0);
    let mut row: passkeys::ActiveModel = row.into();
    row.user_id = Set(account.id);
    row.update(&db).await.expect("move the passkey");
    authenticators.extend(donated);

    let start = login_start(&server, &account.email).await;
    let listed = listed_ids(&start);
    assert_eq!(listed.len(), 2 * MAX_PASSKEYS_PER_ACCOUNT);
    for authenticator in &authenticators {
        assert!(listed.contains(&authenticator.id_text()));
    }

    let last = authenticators.last_mut().expect("eleven passkeys");
    let answer = last.assertion(&start["options"]);
    let res = login_finish(&server, login_id(&start), answer).await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
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
