//! Email addresses are case-insensitive: `Bob@x.com` and `bob@x.com` are one
//! account, one login and one mailbox. Registration stores the lowercase form,
//! every lookup compares `lower(email)`, and accounts that already collide are
//! reported to the admin instead of being merged.
//!
//! Most tests share the suite's database. The migration and the collision
//! report need a database of their own: a case collision cannot exist next to
//! the unique index on `lower(email)`, and their counts are global.

use crate::common;

use common::{mark_email_verified, spawn_real_app, unique_code};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, QueryResult, Statement};
use serde_json::{Value, json};

const PASSWORD: &str = "password123";

fn mixed_email() -> String {
    format!(
        "Bob.X{}@users.opn.onl",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

async fn try_register(server: &axum_test::TestServer, email: &str) -> (u16, Value) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .await;
    (
        res.status_code().as_u16(),
        serde_json::from_str(&res.text()).unwrap_or(Value::Null),
    )
}

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let (status, body) = try_register(server, email).await;
    assert_eq!(status, 201, "register {email}: {body}");
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user id") as i32,
    )
}

async fn login(server: &axum_test::TestServer, email: &str, password: &str) -> (u16, Value) {
    let res = server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await;
    (
        res.status_code().as_u16(),
        serde_json::from_str(&res.text()).unwrap_or(Value::Null),
    )
}

fn pg(sql: &str, values: Vec<sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, sql, values)
}

/// Insert a live user straight into the table, with whatever spelling the
/// caller wants (the API only ever stores lowercase now).
async fn insert_user(
    db: &DatabaseConnection,
    email: &str,
    password: &str,
    age_days: i32,
    deleted: bool,
) -> i32 {
    let hash = bcrypt::hash(password, 4).expect("hash");
    let row = db
        .query_one(pg(
            "INSERT INTO users (email, password_hash, created_at, deleted_at) \
             VALUES ($1, $2, now() - make_interval(days => $3), \
                     CASE WHEN $4 THEN now() ELSE NULL END) \
             RETURNING id",
            vec![email.into(), hash.into(), age_days.into(), deleted.into()],
        ))
        .await
        .expect("insert user")
        .expect("inserted row");
    row.try_get_by_index::<i32>(0).expect("id")
}

async fn stored_email(db: &DatabaseConnection, id: i32) -> String {
    let row: QueryResult = db
        .query_one(pg("SELECT email FROM users WHERE id = $1", vec![id.into()]))
        .await
        .expect("select")
        .expect("user row");
    row.try_get_by_index::<String>(0).expect("email")
}

async fn live_rows_for(db: &DatabaseConnection, address: &str) -> usize {
    db.query_all(pg(
        "SELECT id FROM users WHERE lower(email) = lower($1) AND deleted_at IS NULL",
        vec![address.into()],
    ))
    .await
    .expect("select")
    .len()
}

/// A text column of one user row (`None` when NULL).
async fn column_text(db: &DatabaseConnection, column: &str, id: i32) -> Option<String> {
    let sql = format!("SELECT {column} FROM users WHERE id = $1");
    db.query_one(pg(&sql, vec![id.into()]))
        .await
        .expect("select")
        .expect("user row")
        .try_get_by_index::<Option<String>>(0)
        .expect("text column")
}

async fn promote_admin(db: &DatabaseConnection, user_id: i32) {
    db.execute(pg(
        "UPDATE users SET is_admin = TRUE WHERE id = $1",
        vec![user_id.into()],
    ))
    .await
    .expect("promote");
}

async fn index_exists(db: &DatabaseConnection, name: &str) -> bool {
    !db.query_all(pg(
        "SELECT 1 FROM pg_indexes WHERE tablename = 'users' AND indexname = $1",
        vec![name.into()],
    ))
    .await
    .expect("pg_indexes")
    .is_empty()
}

// ------------------------------------------------------------ shared database

#[tokio::test]
async fn registration_stores_the_address_in_lowercase() {
    let (server, db) = spawn_real_app().await;
    let mixed = mixed_email();
    let lower = mixed.to_lowercase();
    assert_ne!(mixed, lower);

    let (status, body) = try_register(&server, &mixed).await;
    assert_eq!(status, 201, "register: {body}");
    assert_eq!(body["email"], lower.as_str());

    let id = body["user_id"].as_i64().unwrap() as i32;
    assert_eq!(stored_email(&db, id).await, lower);
}

#[tokio::test]
async fn a_case_variant_of_a_registered_address_is_a_duplicate() {
    let (server, db) = spawn_real_app().await;
    let mixed = mixed_email();
    register(&server, &mixed).await;

    // Control: the identical spelling is refused, with this exact answer.
    let (status, exact) = try_register(&server, &mixed).await;
    assert_eq!(status, 409, "exact duplicate: {exact}");
    assert_eq!(exact["error"], "Email already exists");

    for variant in [mixed.to_lowercase(), mixed.to_uppercase()] {
        let (status, body) = try_register(&server, &variant).await;
        assert_eq!(status, 409, "{variant:?} while {mixed:?} exists: {body}");
        assert_eq!(body, exact, "the answer must match an exact duplicate");
    }
    assert_eq!(live_rows_for(&db, &mixed).await, 1);

    // Control: an unrelated address still registers.
    register(&server, &mixed_email()).await;
}

#[tokio::test]
async fn login_matches_the_address_in_any_case() {
    let (server, _db) = spawn_real_app().await;
    let mixed = mixed_email();
    let (_, user_id) = register(&server, &mixed).await;

    for typed in [mixed.clone(), mixed.to_lowercase(), mixed.to_uppercase()] {
        let (status, body) = login(&server, &typed, PASSWORD).await;
        assert_eq!(status, 200, "login as {typed:?}: {body}");
        assert_eq!(body["user_id"].as_i64(), Some(user_id as i64));
        assert_eq!(body["email"], mixed.to_lowercase().as_str());
    }

    // Control: case-insensitive address, case-sensitive password.
    let (status, _) = login(&server, &mixed.to_lowercase(), "PASSWORD123").await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn forgot_password_and_resend_find_the_account_in_any_case() {
    let (server, db) = spawn_real_app().await;
    let mixed = mixed_email();
    let (token, user_id) = register(&server, &mixed).await;
    assert!(
        column_text(&db, "password_reset_token", user_id)
            .await
            .is_none()
    );
    let verification_before = column_text(&db, "verification_token", user_id).await;

    let res = server
        .post("/auth/forgot-password")
        .json(&json!({ "email": mixed.to_lowercase() }))
        .await;
    assert_eq!(res.status_code(), 200, "forgot: {}", res.text());
    let known_body = res.text();
    assert!(
        column_text(&db, "password_reset_token", user_id)
            .await
            .is_some(),
        "forgot-password typed in lowercase did not find the account"
    );

    // No SMTP: the resend is refused and the stored token stays. Finding the
    // row is proved below, once the owner asks about an address that is
    // already verified — an unknown address would not get that answer.
    let res = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": mixed.to_uppercase() }))
        .await;
    assert_eq!(res.status_code(), 503, "resend: {}", res.text());
    assert_eq!(
        res.json::<Value>()["error"],
        "Email is not configured on this instance"
    );
    assert_eq!(
        column_text(&db, "verification_token", user_id).await,
        verification_before,
        "resend without SMTP must not rotate the token"
    );
    mark_email_verified(&db, user_id).await;
    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer(&token)
        .json(&json!({ "email": mixed.to_uppercase() }))
        .await;
    assert_eq!(res.status_code(), 200, "owner resend: {}", res.text());
    assert!(
        res.text().contains("already verified"),
        "resend-verification typed in uppercase did not find the account: {}",
        res.text()
    );

    // Anti-enumeration is unchanged: an unknown address gets the same answer.
    let res = server
        .post("/auth/forgot-password")
        .json(&json!({ "email": mixed_email() }))
        .await;
    assert_eq!(res.status_code(), 200);
    assert_eq!(res.text(), known_body);
}

#[tokio::test]
async fn an_account_stored_in_mixed_case_is_still_found_in_any_case() {
    // Rows from before addresses were folded (and accounts an admin restores)
    // can still be mixed-case, so lookups must not rely on plain equality.
    let (server, db) = spawn_real_app().await;
    let legacy = mixed_email();
    let id = insert_user(&db, &legacy, PASSWORD, 30, false).await;

    let (status, body) = login(&server, &legacy.to_lowercase(), PASSWORD).await;
    assert_eq!(status, 200, "login: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(id as i64));
    let token = body["token"].as_str().expect("token").to_string();

    let res = server
        .post("/auth/forgot-password")
        .json(&json!({ "email": legacy.to_lowercase() }))
        .await;
    assert_eq!(res.status_code(), 200);
    assert!(
        column_text(&db, "password_reset_token", id).await.is_some(),
        "forgot-password did not find the mixed-case row"
    );

    let res = server
        .post("/auth/resend-verification")
        .json(&json!({ "email": legacy.to_lowercase() }))
        .await;
    assert_eq!(res.status_code(), 503, "resend: {}", res.text());
    assert!(
        column_text(&db, "verification_token", id).await.is_none(),
        "resend without SMTP must not invent a token"
    );
    mark_email_verified(&db, id).await;
    let res = server
        .post("/auth/resend-verification")
        .authorization_bearer(&token)
        .json(&json!({ "email": legacy.to_uppercase() }))
        .await;
    assert_eq!(res.status_code(), 200, "owner resend: {}", res.text());
    assert!(
        res.text().contains("already verified"),
        "resend-verification did not find the mixed-case row: {}",
        res.text()
    );

    // And a new sign-up cannot take the same address in another spelling.
    let (server, _) = spawn_real_app().await;
    let (status, body) = try_register(&server, &legacy.to_lowercase()).await;
    assert_eq!(status, 409, "register over a mixed-case row: {body}");
}

#[tokio::test]
async fn org_invites_match_the_address_in_any_case() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, owner_id) = register(&server, &mixed_email()).await;
    mark_email_verified(&db, owner_id).await;
    let slug = format!("case-{}", unique_code().to_lowercase());
    let org = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({ "name": "Case folding", "slug": slug }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap();
    let members = format!("/orgs/{org_id}/members");

    let (_, registered_id) = register(&server, &mixed_email()).await;
    let registered = stored_email(&db, registered_id).await;
    let legacy = mixed_email();
    let legacy_id = insert_user(&db, &legacy, PASSWORD, 30, false).await;

    for (typed, user_id, shown) in [
        (registered.to_uppercase(), registered_id, registered.clone()),
        (legacy.to_lowercase(), legacy_id, legacy.clone()),
    ] {
        let res = server
            .post(&members)
            .authorization_bearer(&owner_token)
            .json(&json!({ "email": typed, "role": "viewer" }))
            .await;
        assert_eq!(res.status_code(), 201, "invite {typed:?}: {}", res.text());
        let body: Value = res.json();
        assert_eq!(body["user_id"].as_i64(), Some(user_id as i64));
        assert_eq!(body["email"], shown.as_str());
    }
}

#[tokio::test]
async fn a_blocked_email_domain_matches_whatever_the_spelling() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &mixed_email()).await;
    promote_admin(&db, admin_id).await;
    let block = |domain: String| {
        let (server, token) = (&server, admin_token.clone());
        async move {
            let res = server
                .post("/admin/blocked/email-domains")
                .authorization_bearer(&token)
                .json(&json!({ "domain": domain }))
                .await;
            assert_eq!(res.status_code(), 201, "block domain: {}", res.text());
        }
    };

    let domain = format!("blocked-{}.iana.org", unique_code().to_lowercase());
    block(domain.clone()).await;
    let (status, body) =
        try_register(&server, &format!("Some.Body@{}", domain.to_uppercase())).await;
    assert_eq!(status, 400, "blocked domain in capitals: {body}");
    assert_eq!(body["error"], "This email domain is blocked");

    // A block entered with a Unicode label is stored as punycode, and catches
    // the address typed in Unicode as well.
    let label = unique_code().to_lowercase();
    block(format!("b\u{fc}cher-{label}.iana.org")).await;
    let (status, body) = try_register(
        &server,
        &format!("Some.Body@B\u{dc}CHER-{}.IANA.ORG", label.to_uppercase()),
    )
    .await;
    assert_eq!(status, 400, "blocked Unicode domain: {body}");
    assert_eq!(body["error"], "This email domain is blocked");
}

#[tokio::test]
async fn unicode_and_punycode_spellings_of_a_domain_are_one_account() {
    let (server, db) = spawn_real_app().await;
    let local = format!("idn.{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let unicode = format!("{local}@b\u{fc}cher.de");
    let punycode = format!("{local}@xn--bcher-kva.de");

    let (status, body) = try_register(&server, &unicode).await;
    assert_eq!(status, 201, "register: {body}");
    assert_eq!(body["email"], punycode.as_str());
    let user_id = body["user_id"].as_i64().unwrap();
    assert_eq!(stored_email(&db, user_id as i32).await, punycode);

    for variant in [
        punycode.clone(),
        format!("{local}@B\u{dc}CHER.de"),
        format!("{}@XN--BCHER-KVA.DE", local.to_uppercase()),
    ] {
        let (status, body) = try_register(&server, &variant).await;
        assert_eq!(status, 409, "{variant:?} while {unicode:?} exists: {body}");
    }

    // One account, reachable under every spelling.
    let (server, _) = spawn_real_app().await;
    for typed in [&unicode, &punycode, &format!("{local}@B\u{dc}CHER.de")] {
        let (status, body) = login(&server, typed, PASSWORD).await;
        assert_eq!(status, 200, "login as {typed:?}: {body}");
        assert_eq!(body["user_id"].as_i64(), Some(user_id));
    }
    assert_eq!(live_rows_for(&db, &punycode).await, 1);
}

#[tokio::test]
async fn restoring_an_account_folds_its_address() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &mixed_email()).await;
    promote_admin(&db, admin_id).await;
    let hex = &uuid::Uuid::new_v4().simple().to_string()[..8];

    // Deleted before addresses were folded: mixed case, Unicode domain.
    let deleted = format!("Gone.{hex}@B\u{fc}cher.de");
    let id = insert_user(&db, &deleted, PASSWORD, 30, true).await;
    let res = server
        .post(&format!("/admin/users/{id}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "restore: {}", res.text());
    let folded = format!("gone.{hex}@xn--bcher-kva.de");
    assert_eq!(stored_email(&db, id).await, folded);

    let (server, _) = spawn_real_app().await;
    let (status, body) = login(&server, &folded, PASSWORD).await;
    assert_eq!(status, 200, "login after restore: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(id as i64));

    // The address was registered again in the meantime, in another case:
    // refused like any other restore onto a taken address.
    let taken = format!("Taken.{hex}@users.opn.onl");
    let old = insert_user(&db, &taken, PASSWORD, 30, true).await;
    insert_user(&db, &taken.to_lowercase(), PASSWORD, 1, false).await;
    let res = server
        .post(&format!("/admin/users/{old}/restore"))
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 409, "restore onto a taken address");
}

#[tokio::test]
async fn passkey_login_start_treats_case_variants_as_one_address() {
    // An unknown address gets a decoy challenge derived from the address. If
    // the two spellings were different usernames the decoys would differ, and
    // so would the pending-state key a real challenge is stored under.
    common::set_env("ENABLE_PASSKEYS", "true");
    let (server, _db) = spawn_real_app().await;
    let mixed = mixed_email();

    let mut allow_lists = Vec::new();
    for typed in [mixed.clone(), mixed.to_lowercase(), mixed.to_uppercase()] {
        let res = server
            .post("/auth/passkey/login/start")
            .json(&json!({ "username": typed }))
            .await;
        assert_eq!(res.status_code(), 200, "start {typed:?}: {}", res.text());
        let body: Value = res.json();
        allow_lists.push(body["options"]["publicKey"]["allowCredentials"].clone());
    }
    assert!(
        allow_lists[0].as_array().is_some_and(|ids| !ids.is_empty()),
        "decoy credential ids missing: {}",
        allow_lists[0]
    );
    assert_eq!(allow_lists[0], allow_lists[1]);
    assert_eq!(allow_lists[0], allow_lists[2]);
}

// ------------------------------------------------------- database of its own

/// A throwaway database next to the suite's one, dropped (connections and all)
/// when the value goes out of scope, including when the test panics.
struct ScratchDb {
    db: DatabaseConnection,
    admin_url: String,
    name: String,
}

impl ScratchDb {
    /// Create an empty database and migrate it to `steps` migrations
    /// (`None` = all of them).
    async fn migrated(steps: Option<u32>) -> Self {
        dotenvy::dotenv().ok();
        let admin_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let name = format!("opn_scratch_{}", uuid::Uuid::new_v4().simple());
        let admin = Database::connect(&admin_url).await.expect("connect");
        admin
            .execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create scratch database");
        let mut url = url::Url::parse(&admin_url).expect("DATABASE_URL");
        url.set_path(&format!("/{name}"));
        let db = Database::connect(url.as_str()).await.expect("connect");
        let scratch = Self {
            db,
            admin_url,
            name,
        };
        Migrator::up(&scratch.db, steps).await.expect("migrate");
        scratch
    }

    async fn app(&self) -> axum_test::TestServer {
        common::set_env("FORCE_HTTPS", "false");
        common::set_env("TRUST_PROXY_HEADERS", "false");
        if std::env::var("JWT_SECRET").is_err() {
            common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
        }
        let state = opn_onl_backend::AppState::for_tests(self.db.clone()).await;
        axum_test::TestServer::new(opn_onl_backend::build_router(state))
            .expect("failed to start test server")
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        let (url, name) = (self.admin_url.clone(), self.name.clone());
        // A fresh runtime on its own thread: Drop cannot await, and the test's
        // runtime may be the one running us.
        let _ = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                if let Ok(admin) = Database::connect(&url).await {
                    let _ = admin
                        .execute_unprepared(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
                        .await;
                }
            });
        })
        .join();
    }
}

const LOWER_INDEX: &str = "idx_users_email_lower_live";

#[tokio::test]
async fn migration_folds_live_addresses_and_leaves_collisions_alone() {
    let scratch = ScratchDb::migrated(Some(36)).await;
    let db = &scratch.db;

    let folded = insert_user(db, "Mixed.Case@users.opn.onl", PASSWORD, 5, false).await;
    let loud_domain = insert_user(db, "Upper.Domain@USERS.OPN.ONL", PASSWORD, 5, false).await;
    let already_lower = insert_user(db, "plain@users.opn.onl", PASSWORD, 5, false).await;
    let dup_a = insert_user(db, "Dup.Person@users.opn.onl", PASSWORD, 5, false).await;
    let dup_b = insert_user(db, "DUP.PERSON@users.opn.onl", PASSWORD, 4, false).await;
    // A deleted account keeps its spelling, and does not make its live
    // namesake a collision.
    let gone = insert_user(db, "Gone.Person@users.opn.onl", PASSWORD, 9, true).await;
    let namesake = insert_user(db, "gone.person@users.opn.onl", PASSWORD, 3, false).await;
    // Unicode domains are folded to punycode when nothing else holds the
    // address, in either spelling...
    let idn_solo = insert_user(db, "Idn.Solo@B\u{fc}cher.de", PASSWORD, 5, false).await;
    // ...and left alone when something does.
    let idn_pair_unicode = insert_user(db, "Idn.Pair@b\u{fc}cher.de", PASSWORD, 5, false).await;
    let idn_pair_ascii = insert_user(db, "idn.pair@xn--bcher-kva.de", PASSWORD, 4, false).await;
    let idn_twin_a = insert_user(db, "Idn.Twin@b\u{fc}cher.de", PASSWORD, 5, false).await;
    let idn_twin_b = insert_user(db, "IDN.TWIN@B\u{dc}CHER.de", PASSWORD, 4, false).await;
    let idn_mixed_ascii = insert_user(db, "Idn.Mixed@Xn--Bcher-Kva.de", PASSWORD, 5, false).await;
    let idn_mixed_unicode = insert_user(db, "idn.mixed@b\u{fc}cher.de", PASSWORD, 4, false).await;

    Migrator::up(db, None).await.expect("migrate to latest");

    assert_eq!(stored_email(db, folded).await, "mixed.case@users.opn.onl");
    assert_eq!(
        stored_email(db, loud_domain).await,
        "upper.domain@users.opn.onl"
    );
    assert_eq!(stored_email(db, already_lower).await, "plain@users.opn.onl");
    assert_eq!(stored_email(db, dup_a).await, "Dup.Person@users.opn.onl");
    assert_eq!(stored_email(db, dup_b).await, "DUP.PERSON@users.opn.onl");
    assert_eq!(stored_email(db, gone).await, "Gone.Person@users.opn.onl");
    assert_eq!(
        stored_email(db, namesake).await,
        "gone.person@users.opn.onl"
    );
    assert_eq!(
        stored_email(db, idn_solo).await,
        "idn.solo@xn--bcher-kva.de"
    );
    assert_eq!(
        stored_email(db, idn_pair_unicode).await,
        "Idn.Pair@b\u{fc}cher.de"
    );
    assert_eq!(
        stored_email(db, idn_pair_ascii).await,
        "idn.pair@xn--bcher-kva.de"
    );
    assert_eq!(
        stored_email(db, idn_twin_a).await,
        "Idn.Twin@b\u{fc}cher.de"
    );
    assert_eq!(
        stored_email(db, idn_twin_b).await,
        "IDN.TWIN@B\u{dc}CHER.de"
    );
    // The ASCII half of a pair is only lowercased; the Unicode half stays.
    assert_eq!(
        stored_email(db, idn_mixed_ascii).await,
        "idn.mixed@xn--bcher-kva.de"
    );
    assert_eq!(
        stored_email(db, idn_mixed_unicode).await,
        "idn.mixed@b\u{fc}cher.de"
    );
    assert!(
        !index_exists(db, LOWER_INDEX).await,
        "the unique index cannot be built while two live accounts collide"
    );
}

#[tokio::test]
async fn migration_adds_the_unique_index_when_nothing_collides() {
    let scratch = ScratchDb::migrated(Some(36)).await;
    let db = &scratch.db;

    let folded = insert_user(db, "Mixed.Case@users.opn.onl", PASSWORD, 5, false).await;
    let gone = insert_user(db, "Gone.Person@users.opn.onl", PASSWORD, 9, true).await;
    insert_user(db, "gone.person@users.opn.onl", PASSWORD, 3, false).await;
    let idn_solo = insert_user(db, "Idn.Solo@B\u{fc}cher.de", PASSWORD, 5, false).await;
    let idn_gone = insert_user(db, "Idn.Gone@b\u{fc}cher.de", PASSWORD, 5, true).await;
    // A Unicode address that cannot be folded differs textually from its
    // punycode twin, so it does not keep the index from being built.
    insert_user(db, "Idn.Pair@b\u{fc}cher.de", PASSWORD, 5, false).await;
    insert_user(db, "idn.pair@xn--bcher-kva.de", PASSWORD, 4, false).await;

    // Exactly this migration (036 -> 037), so `down(1)` below undoes it and not
    // a later one.
    Migrator::up(db, Some(1)).await.expect("migrate to 037");

    assert_eq!(stored_email(db, folded).await, "mixed.case@users.opn.onl");
    assert_eq!(stored_email(db, gone).await, "Gone.Person@users.opn.onl");
    assert_eq!(
        stored_email(db, idn_solo).await,
        "idn.solo@xn--bcher-kva.de"
    );
    assert_eq!(stored_email(db, idn_gone).await, "Idn.Gone@b\u{fc}cher.de");
    assert!(index_exists(db, LOWER_INDEX).await);
    assert_eq!(
        opn_onl_backend::utils::email_domain_policy::count_email_case_collisions(db)
            .await
            .unwrap(),
        1,
        "the Unicode/punycode pair is the one collision left"
    );

    // The index now refuses a live case-variant, from any writer...
    let refused = db
        .execute(pg(
            "INSERT INTO users (email, password_hash) VALUES ($1, 'x')",
            vec!["MIXED.CASE@users.opn.onl".into()],
        ))
        .await;
    assert!(refused.is_err(), "a live case-variant must be refused");
    // ...but not one that is deleted.
    insert_user(db, "MIXED.CASE@users.opn.onl", PASSWORD, 1, true).await;

    // down() drops the index and leaves the folded data as it is.
    Migrator::down(db, Some(1)).await.expect("migrate down");
    assert!(!index_exists(db, LOWER_INDEX).await);
    assert_eq!(stored_email(db, folded).await, "mixed.case@users.opn.onl");
}

#[tokio::test]
async fn accounts_that_collide_are_reported_and_resolved_predictably() {
    let scratch = ScratchDb::migrated(None).await;
    let db = &scratch.db;
    // The migration could not have built this index on data like the data
    // below, which is the state this test models.
    db.execute_unprepared(&format!("DROP INDEX {LOWER_INDEX}"))
        .await
        .expect("drop index");

    let admin_app = scratch.app().await;
    // First live account on an empty table becomes the admin.
    let (admin_token, _) = register(&admin_app, "admin@users.opn.onl").await;

    let older = insert_user(db, "Dup.Person@users.opn.onl", "older-password", 5, false).await;
    let newer = insert_user(db, "DUP.PERSON@users.opn.onl", "newer-password", 4, false).await;
    // A deleted namesake and an unrelated address do not count.
    insert_user(db, "Dup.Person@Users.Opn.Onl", PASSWORD, 9, true).await;
    insert_user(db, "other@users.opn.onl", PASSWORD, 3, false).await;
    // The same address as Unicode and as punycode is a collision of its own.
    let idn_unicode =
        insert_user(db, "Idn.Pair@b\u{fc}cher.de", "unicode-password", 5, false).await;
    let idn_ascii = insert_user(db, "idn.pair@xn--bcher-kva.de", "ascii-password", 4, false).await;

    let stats = async || {
        let res = admin_app
            .get("/admin/stats")
            .authorization_bearer(&admin_token)
            .await;
        assert_eq!(res.status_code(), 200, "stats: {}", res.text());
        res.json::<Value>()
    };
    assert_eq!(stats().await["email_case_collisions"], 2);

    // The admin finds every account behind the address with a lowercase query.
    let res = admin_app
        .get("/admin/users")
        .add_query_param("search", "dup.person@users.opn.onl")
        .authorization_bearer(&admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "search: {}", res.text());
    let found = res.json::<Value>();
    let mut ids: Vec<i64> = found["users"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|u| u["deleted_at"].is_null())
        .map(|u| u["id"].as_i64().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, [older as i64, newer as i64]);

    // Sign-in: the account registered under exactly the spelling typed wins...
    let app = scratch.app().await;
    let (status, body) = login(&app, "DUP.PERSON@users.opn.onl", "newer-password").await;
    assert_eq!(status, 200, "exact spelling of the newer account: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(newer as i64));
    let (status, body) = login(&app, "Dup.Person@users.opn.onl", "older-password").await;
    assert_eq!(status, 200, "exact spelling of the older account: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(older as i64));
    // ...and that account's password is the only one that works.
    let (status, _) = login(&app, "DUP.PERSON@users.opn.onl", "older-password").await;
    assert_eq!(status, 401);

    // Any other spelling reaches the oldest account.
    let (status, body) = login(&app, "dup.person@users.opn.onl", "older-password").await;
    assert_eq!(status, 200, "lowercase spelling: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(older as i64));
    let (status, _) = login(&app, "dup.person@users.opn.onl", "newer-password").await;
    assert_eq!(status, 401);

    // The Unicode-domain account is reachable under the spelling it signed up
    // with, the other one under punycode.
    let (status, body) = login(&app, "Idn.Pair@b\u{fc}cher.de", "unicode-password").await;
    assert_eq!(status, 200, "Unicode spelling: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(idn_unicode as i64));
    let (status, body) = login(&app, "idn.pair@xn--bcher-kva.de", "ascii-password").await;
    assert_eq!(status, 200, "punycode spelling: {body}");
    assert_eq!(body["user_id"].as_i64(), Some(idn_ascii as i64));

    // A new sign-up cannot add a third account, whatever the index says.
    let signup_app = scratch.app().await;
    let (status, body) = try_register(&signup_app, "dup.person@users.opn.onl").await;
    assert_eq!(status, 409, "third spelling: {body}");

    // Resolving a collision (an admin deletes one account) clears it from the
    // report.
    for (id, left) in [(newer, 1), (idn_ascii, 0)] {
        db.execute(pg(
            "UPDATE users SET deleted_at = now() WHERE id = $1",
            vec![id.into()],
        ))
        .await
        .expect("delete one account");
        assert_eq!(stats().await["email_case_collisions"], left);
    }

    // The startup check runs against the same data without failing.
    opn_onl_backend::warn_on_email_case_collisions(db).await;
}
