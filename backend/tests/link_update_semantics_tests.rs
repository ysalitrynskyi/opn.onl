//! What updating, searching, exporting and cloning a link leaves behind.
//!
//! * burn-after-reading and the click cap it rides on change together;
//! * bulk update keeps `starts_at` before `expires_at`;
//! * a folder, a title and notes can be cleared;
//! * `GET /links?search=` is case-insensitive and treats `%` and `_` literally;
//! * a failed read is a 500, not an empty 200;
//! * a clone carries everything that makes the original behave as it does;
//! * `GET /tags/{id}/links` returns each link's tags and `added` counts rows;
//! * the CSV export is safe to open in a spreadsheet;
//! * `update_link` never writes a stale `click_count`.
//!
//! Real router, real Postgres.

mod common;

use axum_test::TestServer;
use chrono::{Duration, SubsecRound, Utc};
use common::{spawn_real_app, unique_code, unique_email};
use opn_onl_backend::AppState;
use opn_onl_backend::entity::{blocked_links, link_tags, links, routing_rules, tags, users};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, Database, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use serde_json::{Value, json};
use std::future::IntoFuture;
use std::time::Duration as StdDuration;

// ---------------------------------------------------------------- fixtures

/// A fresh app for one request. Every app has its own rate limiters, and the
/// mock transport puts all callers in one per-second bucket, so a test that
/// makes many calls takes a new app for each rather than tripping it.
async fn app() -> TestServer {
    spawn_real_app().await.0
}

struct Person {
    id: i32,
    token: String,
}

/// A verified user and a valid JWT, inserted directly (no HTTP, no bcrypt).
async fn person(db: &DatabaseConnection) -> Person {
    let email = unique_email();
    let user = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &email, user.token_version).expect("mint token");
    Person { id: user.id, token }
}

fn unique_url() -> String {
    format!("https://iana.org/lus-{}", uuid::Uuid::new_v4().simple())
}

/// A live personal link with nothing special about it; tests change the fields
/// they care about before inserting.
fn new_link(user_id: i32) -> links::ActiveModel {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set(unique_url()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
}

async fn insert(db: &DatabaseConnection, link: links::ActiveModel) -> links::Model {
    link.insert(db).await.expect("insert link")
}

async fn row(db: &DatabaseConnection, id: i32) -> links::Model {
    links::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .expect("link row exists")
}

async fn send(
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> axum_test::TestResponse {
    let server = app().await;
    let request = match method {
        "GET" => server.get(path),
        "POST" => server.post(path),
        "PUT" => server.put(path),
        other => panic!("unsupported method {other}"),
    }
    .authorization_bearer(token);
    match body {
        Some(body) => request.json(&body).await,
        None => request.await,
    }
}

async fn put_link(token: &str, id: i32, body: Value) -> axum_test::TestResponse {
    send(token, "PUT", &format!("/links/{id}"), Some(body)).await
}

/// Status of a visit to the short link, from a fresh app (own rate limits).
async fn visit(code: &str) -> u16 {
    app()
        .await
        .get(&format!("/{code}"))
        .await
        .status_code()
        .as_u16()
}

fn link_ids(list: &Value) -> Vec<i64> {
    list.as_array()
        .expect("array")
        .iter()
        .filter_map(|l| l["id"].as_i64())
        .collect()
}

// ------------------------------------------------- burn-after-reading and cap

/// Create a burn link the way the API does, so the stored pair is exactly what
/// create writes: burn on, cap 1.
async fn create_burn_link(db: &DatabaseConnection) -> (Person, links::Model) {
    let user = person(db).await;
    let res = send(
        &user.token,
        "POST",
        "/links",
        Some(json!({ "original_url": unique_url(), "burn_after_reading": true })),
    )
    .await;
    assert_eq!(res.status_code(), 201, "create: {}", res.text());
    let link = row(db, res.json::<Value>()["id"].as_i64().unwrap() as i32).await;
    assert!(link.burn_after_reading, "precondition: burn flag set");
    assert_eq!(link.max_clicks, Some(1), "precondition: create gives cap 1");
    (user, link)
}

#[tokio::test]
async fn removing_the_cap_of_a_burn_link_turns_burn_off() {
    let (_, db) = spawn_real_app().await;
    let (user, link) = create_burn_link(&db).await;

    let res = put_link(&user.token, link.id, json!({ "remove_max_clicks": true })).await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["burn_after_reading"], false, "{body}");
    assert_eq!(body["max_clicks"], Value::Null, "{body}");

    let stored = row(&db, link.id).await;
    assert!(
        !stored.burn_after_reading,
        "a one-time link cannot be uncapped"
    );
    assert_eq!(stored.max_clicks, None);

    // The link now behaves like any uncapped link: it keeps redirecting.
    let visits = (visit(&link.code).await, visit(&link.code).await);
    assert_eq!(visits, (307, 307));
}

#[tokio::test]
async fn unticking_burn_in_the_editor_removes_the_cap_it_implied() {
    let (_, db) = spawn_real_app().await;
    let (user, link) = create_burn_link(&db).await;

    // What the dashboard's edit form sends on save.
    let res = put_link(
        &user.token,
        link.id,
        json!({
            "burn_after_reading": false,
            "safe_link_interstitial": false,
            "bio_visible": false,
        }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["burn_after_reading"], false, "{body}");
    assert_eq!(body["max_clicks"], Value::Null, "{body}");

    let stored = row(&db, link.id).await;
    assert!(!stored.burn_after_reading);
    assert_eq!(stored.max_clicks, None);
    let visits = (visit(&link.code).await, visit(&link.code).await);
    assert_eq!(
        visits,
        (307, 307),
        "an un-burned link must not die after its first visit"
    );
}

#[tokio::test]
async fn unticking_burn_keeps_a_cap_the_request_names() {
    let (_, db) = spawn_real_app().await;
    let (user, link) = create_burn_link(&db).await;

    let res = put_link(
        &user.token,
        link.id,
        json!({ "burn_after_reading": false, "max_clicks": 3 }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let stored = row(&db, link.id).await;
    assert!(!stored.burn_after_reading);
    assert_eq!(stored.max_clicks, Some(3));
}

#[tokio::test]
async fn saving_the_form_does_not_wipe_a_cap_on_a_link_that_is_not_burn() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut capped = new_link(user.id);
    capped.max_clicks = Set(Some(5));
    let capped = insert(&db, capped).await;

    // The form sends the burn flag on every save, ticked or not.
    let res = put_link(
        &user.token,
        capped.id,
        json!({ "burn_after_reading": false }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let stored = row(&db, capped.id).await;
    assert!(!stored.burn_after_reading);
    assert_eq!(stored.max_clicks, Some(5), "the cap is not the burn link's");
}

#[tokio::test]
async fn saving_a_burn_link_unchanged_keeps_its_cap() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut burn = new_link(user.id);
    burn.burn_after_reading = Set(true);
    burn.max_clicks = Set(Some(4));
    let burn = insert(&db, burn).await;

    let res = put_link(&user.token, burn.id, json!({ "burn_after_reading": true })).await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let stored = row(&db, burn.id).await;
    assert!(stored.burn_after_reading);
    assert_eq!(stored.max_clicks, Some(4));
}

#[tokio::test]
async fn ticking_burn_gives_the_link_a_one_time_cap_that_is_enforced() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let uncapped = insert(&db, new_link(user.id)).await;

    let res = put_link(
        &user.token,
        uncapped.id,
        json!({ "burn_after_reading": true }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let stored = row(&db, uncapped.id).await;
    assert!(stored.burn_after_reading);
    assert_eq!(stored.max_clicks, Some(1));

    assert_eq!(visit(&uncapped.code).await, 307, "first visit opens it");
    assert_eq!(
        visit(&uncapped.code).await,
        410,
        "second visit finds it burned"
    );
}

#[tokio::test]
async fn burn_over_a_capped_link_keeps_the_existing_cap() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut capped = new_link(user.id);
    capped.max_clicks = Set(Some(5));
    let capped = insert(&db, capped).await;

    let res = put_link(
        &user.token,
        capped.id,
        json!({ "burn_after_reading": true }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(row(&db, capped.id).await.max_clicks, Some(5));
}

#[tokio::test]
async fn removing_the_cap_of_a_plain_capped_link_still_works() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut capped = new_link(user.id);
    capped.max_clicks = Set(Some(5));
    let capped = insert(&db, capped).await;

    let res = put_link(&user.token, capped.id, json!({ "remove_max_clicks": true })).await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let stored = row(&db, capped.id).await;
    assert_eq!(stored.max_clicks, None);
    assert!(!stored.burn_after_reading);
}

// ------------------------------------------------------------- bulk schedule

#[tokio::test]
async fn bulk_update_never_ends_a_link_before_it_starts() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    // Whole seconds: the database keeps microseconds, and the test compares.
    let now = Utc::now().trunc_subsecs(0);
    let starts = (now + Duration::days(2)).naive_utc();
    let expires = (now + Duration::days(10)).naive_utc();

    let mut scheduled = new_link(user.id);
    scheduled.starts_at = Set(Some(starts));
    scheduled.expires_at = Set(Some(expires));
    let scheduled = insert(&db, scheduled).await;
    let unscheduled = insert(&db, new_link(user.id)).await;

    // Control: the single-link update rejects exactly this.
    let earlier = (now + Duration::days(1)).to_rfc3339();
    let single = put_link(&user.token, scheduled.id, json!({ "expires_at": earlier })).await;
    assert_eq!(
        single.status_code(),
        400,
        "single update: {}",
        single.text()
    );

    // An expiry before the link's own start is skipped, one that is fine is
    // applied, and the same request carries both kinds of link.
    let res = send(
        &user.token,
        "POST",
        "/links/bulk/update",
        Some(json!({ "ids": [scheduled.id, unscheduled.id], "expires_at": earlier })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "bulk update: {}", res.text());
    assert_eq!(res.json::<Value>()["updated"], 1);
    let after = row(&db, scheduled.id).await;
    assert_eq!(
        after.expires_at,
        Some(expires),
        "the schedule must be untouched"
    );
    assert!(row(&db, unscheduled.id).await.expires_at.is_some());

    // The start itself is not "before" an expiry equal to it.
    let at_start = starts.and_utc().to_rfc3339();
    let res = send(
        &user.token,
        "POST",
        "/links/bulk/update",
        Some(json!({ "ids": [scheduled.id], "expires_at": at_start })),
    )
    .await;
    assert_eq!(res.json::<Value>()["updated"], 0);

    // Control: an expiry after the start is accepted.
    let later = (now + Duration::days(20)).to_rfc3339();
    let res = send(
        &user.token,
        "POST",
        "/links/bulk/update",
        Some(json!({ "ids": [scheduled.id], "expires_at": later })),
    )
    .await;
    assert_eq!(res.json::<Value>()["updated"], 1);
    let moved = row(&db, scheduled.id).await;
    assert!(moved.expires_at.unwrap() > starts);

    // Removing the expiry is always fine.
    let res = send(
        &user.token,
        "POST",
        "/links/bulk/update",
        Some(json!({ "ids": [scheduled.id], "remove_expiration": true })),
    )
    .await;
    assert_eq!(res.json::<Value>()["updated"], 1);
    assert_eq!(row(&db, scheduled.id).await.expires_at, None);
}

// ----------------------------------------------------------- clearing fields

async fn create_folder(token: &str, name: &str) -> i32 {
    let res = send(token, "POST", "/folders", Some(json!({ "name": name }))).await;
    assert_eq!(res.status_code(), 201, "create folder: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

#[tokio::test]
async fn remove_folder_takes_a_link_out_of_its_folder() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let folder = create_folder(&user.token, "keep").await;
    let mut filed = new_link(user.id);
    filed.folder_id = Set(Some(folder));
    let filed = insert(&db, filed).await;

    // null is "not sent", as it always was.
    let res = put_link(&user.token, filed.id, json!({ "folder_id": null })).await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(row(&db, filed.id).await.folder_id, Some(folder));

    let res = put_link(&user.token, filed.id, json!({ "remove_folder": true })).await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(res.json::<Value>()["folder_id"], Value::Null);
    assert_eq!(row(&db, filed.id).await.folder_id, None);

    // The folder itself, and moving back in, still work.
    let res = put_link(&user.token, filed.id, json!({ "folder_id": folder })).await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(row(&db, filed.id).await.folder_id, Some(folder));

    // Asking for both removes the link from the folder, like the other
    // `remove_*` flags win over the value they remove.
    let res = put_link(
        &user.token,
        filed.id,
        json!({ "folder_id": folder, "remove_folder": true }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(row(&db, filed.id).await.folder_id, None);
}

#[tokio::test]
async fn bulk_remove_folder_clears_every_owned_link_and_skips_the_rest() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let stranger = person(&db).await;
    let folder = create_folder(&user.token, "bulk").await;
    let stranger_folder = create_folder(&stranger.token, "theirs").await;

    let mut ids = Vec::new();
    for _ in 0..2 {
        let mut filed = new_link(user.id);
        filed.folder_id = Set(Some(folder));
        ids.push(insert(&db, filed).await.id);
    }
    let mut foreign = new_link(stranger.id);
    foreign.folder_id = Set(Some(stranger_folder));
    let foreign = insert(&db, foreign).await;
    let mut all = ids.clone();
    all.push(foreign.id);

    let res = send(
        &user.token,
        "POST",
        "/links/bulk/update",
        Some(json!({ "ids": all, "remove_folder": true })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "bulk update: {}", res.text());
    assert_eq!(res.json::<Value>()["updated"], 2);
    for id in ids {
        assert_eq!(row(&db, id).await.folder_id, None);
    }
    assert_eq!(row(&db, foreign.id).await.folder_id, Some(stranger_folder));
}

#[tokio::test]
async fn an_empty_title_or_notes_clears_the_field() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut labelled = new_link(user.id);
    labelled.title = Set(Some("Launch".to_string()));
    labelled.notes = Set(Some("remember".to_string()));
    let labelled = insert(&db, labelled).await;

    let res = put_link(
        &user.token,
        labelled.id,
        json!({ "title": "", "notes": "" }),
    )
    .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    let body: Value = res.json();
    assert_eq!(body["title"], Value::Null, "{body}");
    assert_eq!(body["notes"], Value::Null, "{body}");
    let stored = row(&db, labelled.id).await;
    assert_eq!((stored.title, stored.notes), (None, None));

    // Control: a value is stored as sent.
    let res = put_link(&user.token, labelled.id, json!({ "title": "Again" })).await;
    assert_eq!(res.json::<Value>()["title"], "Again");
}

// -------------------------------------------------------------------- search

#[tokio::test]
async fn link_search_is_case_insensitive_and_takes_wildcards_literally() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;

    let mut titled = new_link(user.id);
    titled.original_url = Set("https://iana.org/plain-one".to_string());
    titled.title = Set(Some("My Title".to_string()));
    let titled = insert(&db, titled).await;
    let mut underscore = new_link(user.id);
    underscore.original_url = Set("https://iana.org/qz_zq".to_string());
    let underscore = insert(&db, underscore).await;
    let mut lookalike = new_link(user.id);
    lookalike.original_url = Set("https://iana.org/qzXzq".to_string());
    insert(&db, lookalike).await;
    let mut noted = new_link(user.id);
    noted.notes = Set(Some("Quarterly Report".to_string()));
    let noted = insert(&db, noted).await;
    let mut backslash = new_link(user.id);
    backslash.original_url = Set("https://iana.org/back\\slash".to_string());
    let backslash = insert(&db, backslash).await;
    let mixed_case_code = format!("MiXed{}", unique_code());
    let mut coded = new_link(user.id);
    coded.code = Set(mixed_case_code.clone());
    let coded = insert(&db, coded).await;

    let search = |q: &str| {
        let token = user.token.clone();
        let q = q.to_string();
        async move {
            let res = app()
                .await
                .get("/links")
                .add_query_param("search", &q)
                .authorization_bearer(&token)
                .await;
            assert_eq!(res.status_code(), 200, "list: {}", res.text());
            link_ids(&res.json::<Value>())
        }
    };

    assert_eq!(
        search("%").await,
        Vec::<i64>::new(),
        "% is a literal percent sign"
    );
    assert_eq!(
        search("qz_zq").await,
        vec![underscore.id as i64],
        "_ is not a wildcard"
    );
    assert_eq!(search("qz_").await, vec![underscore.id as i64]);
    assert_eq!(
        search("my title").await,
        vec![titled.id as i64],
        "case-insensitive title"
    );
    assert_eq!(
        search("QUARTERLY").await,
        vec![noted.id as i64],
        "case-insensitive notes"
    );
    assert_eq!(
        search(&mixed_case_code.to_lowercase()).await,
        vec![coded.id as i64],
        "case-insensitive code"
    );
    assert_eq!(
        search("PLAIN-ONE").await,
        vec![titled.id as i64],
        "case-insensitive URL"
    );
    assert_eq!(
        search("\\").await,
        vec![backslash.id as i64],
        "a backslash is literal"
    );
}

// -------------------------------------------------------- failed reads are 5xx

/// A database of its own, created for one test, whose tables a test can break
/// after the user and links exist. Sign-in keeps working (it only reads
/// `users`), so what is left to fail is the read the endpoint under test does.
struct Scratch {
    base: DatabaseConnection,
    db: DatabaseConnection,
    name: String,
}

impl Scratch {
    async fn new() -> Scratch {
        common::set_env("FORCE_HTTPS", "false");
        common::set_env("TRUST_PROXY_HEADERS", "false");
        if std::env::var("JWT_SECRET").is_err() {
            common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
        }
        let base = common::setup_test_db().await;
        let name = format!("opn_scratch_{}", uuid::Uuid::new_v4().simple());
        base.execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create scratch database");
        let mut url = url::Url::parse(&std::env::var("DATABASE_URL").expect("DATABASE_URL"))
            .expect("DATABASE_URL is a URL");
        url.set_path(&format!("/{name}"));
        let db = Database::connect(url.as_str())
            .await
            .expect("connect to scratch database");
        {
            use migration::{Migrator, MigratorTrait};
            Migrator::up(&db, None)
                .await
                .expect("migrate scratch database");
        }
        Scratch { base, db, name }
    }

    /// A new app over the scratch database (own rate limiters).
    async fn app(&self) -> TestServer {
        let state = AppState::for_tests(self.db.clone()).await;
        TestServer::new(opn_onl_backend::build_router(state)).expect("start test server")
    }

    async fn exec(&self, sql: &str) {
        self.db
            .execute_unprepared(sql)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
    }

    async fn drop_database(self) {
        let Scratch { base, db, name } = self;
        let _ = db.close().await;
        base.execute_unprepared(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .await
            .expect("drop scratch database");
    }
}

fn is_json_error(res: &axum_test::TestResponse) -> bool {
    res.maybe_content_type()
        .is_some_and(|t| t.starts_with("application/json"))
        && res.json::<Value>()["error"].is_string()
}

#[tokio::test]
async fn a_failed_read_is_a_500_not_an_empty_answer() {
    let scratch = Scratch::new().await;
    let user = person(&scratch.db).await;

    let tag = tags::ActiveModel {
        name: Set("alpha".to_string()),
        user_id: Set(Some(user.id)),
        ..Default::default()
    }
    .insert(&scratch.db)
    .await
    .expect("insert tag");
    let other_tag = tags::ActiveModel {
        name: Set("beta".to_string()),
        user_id: Set(Some(user.id)),
        ..Default::default()
    }
    .insert(&scratch.db)
    .await
    .expect("insert tag");
    let folder = {
        let res = scratch
            .app()
            .await
            .post("/folders")
            .authorization_bearer(&user.token)
            .json(&json!({ "name": "inbox" }))
            .await;
        assert_eq!(res.status_code(), 201, "folder: {}", res.text());
        res.json::<Value>()["id"].as_i64().unwrap() as i32
    };
    let mut link = new_link(user.id);
    link.folder_id = Set(Some(folder));
    let link = insert(&scratch.db, link).await;
    link_tags::ActiveModel {
        link_id: Set(link.id),
        tag_id: Set(tag.id),
        ..Default::default()
    }
    .insert(&scratch.db)
    .await
    .expect("attach tag");

    let get = |path: String| {
        let token = user.token.clone();
        let scratch = &scratch;
        async move {
            scratch
                .app()
                .await
                .get(&path)
                .authorization_bearer(&token)
                .await
        }
    };
    let mut failures: Vec<String> = Vec::new();
    let mut expect_500 = |what: &str, res: axum_test::TestResponse| {
        if res.status_code() != 500 || !is_json_error(&res) {
            failures.push(format!(
                "{what}: expected a 500 with a JSON error, got {} {}",
                res.status_code(),
                res.text().chars().take(120).collect::<String>()
            ));
        }
    };

    // Control: everything answers while the tables are intact.
    for path in [
        "/links".to_string(),
        "/links/export".to_string(),
        "/tags".to_string(),
        format!("/tags/{}", tag.id),
        format!("/tags/{}/links", tag.id),
        "/folders".to_string(),
        format!("/folders/{folder}/links"),
    ] {
        let res = get(path.clone()).await;
        assert_eq!(res.status_code(), 200, "control {path}: {}", res.text());
    }

    // The tag assignments cannot be read.
    scratch
        .exec("ALTER TABLE link_tags RENAME COLUMN tag_id TO tag_id_gone")
        .await;
    expect_500("GET /links without tag data", get("/links".into()).await);
    expect_500("GET /tags without tag data", get("/tags".into()).await);
    expect_500(
        "GET /tags/{id} without tag data",
        get(format!("/tags/{}", tag.id)).await,
    );
    expect_500(
        "GET /folders/{id}/links without tag data",
        get(format!("/folders/{folder}/links")).await,
    );
    expect_500(
        "PUT /tags/{id} without tag data",
        scratch
            .app()
            .await
            .put(&format!("/tags/{}", tag.id))
            .authorization_bearer(&user.token)
            .json(&json!({ "color": "#112233" }))
            .await,
    );
    expect_500(
        "POST /links/{id}/tags without tag data",
        scratch
            .app()
            .await
            .post(&format!("/links/{}/tags", link.id))
            .authorization_bearer(&user.token)
            .json(&json!({ "tag_ids": [other_tag.id] }))
            .await,
    );
    scratch
        .exec("ALTER TABLE link_tags RENAME COLUMN tag_id_gone TO tag_id")
        .await;

    // The links themselves cannot be read.
    scratch
        .exec("ALTER TABLE links RENAME COLUMN notes TO notes_gone")
        .await;
    expect_500("GET /links without link data", get("/links".into()).await);
    expect_500(
        "GET /links/export without link data",
        get("/links/export".into()).await,
    );
    scratch
        .exec("ALTER TABLE links RENAME COLUMN notes_gone TO notes")
        .await;

    // The per-folder counts cannot be read.
    scratch
        .exec("ALTER TABLE links RENAME COLUMN folder_id TO folder_id_gone")
        .await;
    expect_500("GET /folders without counts", get("/folders".into()).await);
    scratch
        .exec("ALTER TABLE links RENAME COLUMN folder_id_gone TO folder_id")
        .await;

    // Control: back to normal once the tables are readable again.
    let res = get("/links".into()).await;
    let listed_after = (res.status_code().as_u16(), link_ids(&res.json::<Value>()));

    scratch.drop_database().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(listed_after, (200, vec![link.id as i64]));
}

// --------------------------------------------------------------------- clone

async fn add_tag(db: &DatabaseConnection, user_id: i32, name: &str) -> tags::Model {
    tags::ActiveModel {
        name: Set(name.to_string()),
        user_id: Set(Some(user_id)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert tag")
}

async fn attach(db: &DatabaseConnection, link_id: i32, tag_id: i32) {
    link_tags::ActiveModel {
        link_id: Set(link_id),
        tag_id: Set(tag_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("attach tag");
}

async fn put_rules(token: &str, link_id: i32, rules: Value) {
    let res = send(
        token,
        "PUT",
        &format!("/links/{link_id}/rules"),
        Some(json!({ "rules": rules })),
    )
    .await;
    assert_eq!(res.status_code(), 200, "put rules: {}", res.text());
}

async fn rules_of(db: &DatabaseConnection, link_id: i32) -> Vec<routing_rules::Model> {
    routing_rules::Entity::find()
        .filter(routing_rules::Column::LinkId.eq(link_id))
        .order_by_asc(routing_rules::Column::Priority)
        .all(db)
        .await
        .unwrap()
}

async fn tag_ids_of(db: &DatabaseConnection, link_id: i32) -> Vec<i32> {
    let mut ids: Vec<i32> = link_tags::Entity::find()
        .filter(link_tags::Column::LinkId.eq(link_id))
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|lt| lt.tag_id)
        .collect();
    ids.sort_unstable();
    ids
}

async fn clone_of(token: &str, id: i32) -> axum_test::TestResponse {
    send(token, "POST", &format!("/links/{id}/clone"), None).await
}

#[tokio::test]
async fn a_clone_keeps_everything_that_makes_the_original_behave_as_it_does() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let folder = create_folder(&user.token, "campaign").await;
    let first = add_tag(&db, user.id, "first").await;
    let second = add_tag(&db, user.id, "second").await;
    let now = Utc::now();

    let mut original = new_link(user.id);
    original.title = Set(Some("Spring".to_string()));
    original.notes = Set(Some("keep me".to_string()));
    original.password_hash = Set(Some("stored-hash".to_string()));
    original.expires_at = Set(Some((now + Duration::days(30)).naive_utc()));
    original.starts_at = Set(Some((now - Duration::days(1)).naive_utc()));
    original.max_clicks = Set(Some(100));
    original.folder_id = Set(Some(folder));
    original.safe_link_interstitial = Set(true);
    original.is_pinned = Set(true);
    original.click_count = Set(42);
    let original = insert(&db, original).await;
    attach(&db, original.id, first.id).await;
    attach(&db, original.id, second.id).await;
    put_rules(
        &user.token,
        original.id,
        json!([
            { "priority": 1, "match_device": "mobile", "destination_url": unique_url(), "weight": 3 },
            { "priority": 2, "match_country": "DE", "destination_url": unique_url(), "weight": 1 },
        ]),
    )
    .await;

    let res = clone_of(&user.token, original.id).await;
    assert_eq!(res.status_code(), 201, "clone: {}", res.text());
    let clone = row(&db, res.json::<Value>()["id"].as_i64().unwrap() as i32).await;

    assert_ne!(clone.code, original.code);
    assert_eq!(clone.user_id, Some(user.id));
    assert_eq!(clone.original_url, original.original_url);
    assert_eq!(clone.title.as_deref(), Some("Spring (copy)"));
    assert_eq!(clone.notes, original.notes);
    assert_eq!(clone.password_hash, original.password_hash);
    assert_eq!(clone.expires_at, original.expires_at);
    assert_eq!(clone.starts_at, original.starts_at);
    assert_eq!(clone.max_clicks, Some(100));
    assert_eq!(clone.folder_id, Some(folder));
    assert!(
        clone.safe_link_interstitial,
        "the leaving-site warning must survive"
    );
    assert!(!clone.is_pinned);
    assert_eq!(clone.click_count, 0);

    assert_eq!(tag_ids_of(&db, clone.id).await, {
        let mut both = vec![first.id, second.id];
        both.sort_unstable();
        both
    });

    let original_rules = rules_of(&db, original.id).await;
    let cloned_rules = rules_of(&db, clone.id).await;
    assert_eq!(
        cloned_rules.len(),
        2,
        "the clone must route like the original"
    );
    for (a, b) in original_rules.iter().zip(&cloned_rules) {
        assert_eq!(
            (
                &a.match_device,
                &a.match_os,
                &a.match_country,
                &a.match_lang
            ),
            (
                &b.match_device,
                &b.match_os,
                &b.match_country,
                &b.match_lang
            )
        );
        assert_eq!((a.priority, a.weight), (b.priority, b.weight));
        assert_eq!(a.destination_url, b.destination_url);
        assert_ne!(a.id, b.id, "the rules are copies, not shared rows");
    }
    // The original is untouched.
    assert_eq!(tag_ids_of(&db, original.id).await.len(), 2);
    assert_eq!(rules_of(&db, original.id).await.len(), 2);
}

#[tokio::test]
async fn cloning_a_used_burn_link_makes_a_fresh_one_time_link() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut burned = new_link(user.id);
    burned.burn_after_reading = Set(true);
    burned.max_clicks = Set(Some(1));
    burned.click_count = Set(1);
    burned.burned_at = Set(Some(Utc::now().naive_utc()));
    let burned = insert(&db, burned).await;
    assert!(!burned.is_active(), "precondition: the original is spent");

    let res = clone_of(&user.token, burned.id).await;
    assert_eq!(res.status_code(), 201, "clone: {}", res.text());
    let clone = row(&db, res.json::<Value>()["id"].as_i64().unwrap() as i32).await;
    assert!(clone.burn_after_reading);
    assert_eq!(clone.max_clicks, Some(1));
    assert_eq!(clone.click_count, 0);
    assert_eq!(clone.burned_at, None);
    assert!(clone.is_active());

    assert_eq!(visit(&clone.code).await, 307, "first visit opens the clone");
    assert_eq!(visit(&clone.code).await, 410, "and it burns after that");
}

#[tokio::test]
async fn cloning_a_burn_link_that_lost_its_cap_restores_the_one_time_cap() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    // The state `remove_max_clicks` used to leave behind.
    let mut broken = new_link(user.id);
    broken.burn_after_reading = Set(true);
    let broken = insert(&db, broken).await;
    assert_eq!(broken.max_clicks, None);

    let res = clone_of(&user.token, broken.id).await;
    assert_eq!(res.status_code(), 201, "clone: {}", res.text());
    let clone = row(&db, res.json::<Value>()["id"].as_i64().unwrap() as i32).await;
    assert!(clone.burn_after_reading);
    assert_eq!(clone.max_clicks, Some(1));
}

#[tokio::test]
async fn a_clone_has_no_title_when_the_original_has_none() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut blank = new_link(user.id);
    blank.title = Set(Some(String::new()));
    let blank = insert(&db, blank).await;

    let res = clone_of(&user.token, blank.id).await;
    assert_eq!(res.status_code(), 201, "clone: {}", res.text());
    let clone = row(&db, res.json::<Value>()["id"].as_i64().unwrap() as i32).await;
    assert_eq!(clone.title, None, "not \" (copy)\"");
}

#[tokio::test]
async fn a_blocked_destination_is_not_cloned_into_a_new_code() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let plain = insert(&db, new_link(user.id)).await;
    let routed = insert(&db, new_link(user.id)).await;
    let rule_destination = unique_url();
    put_rules(
        &user.token,
        routed.id,
        json!([{ "priority": 1, "match_device": "mobile", "destination_url": rule_destination }]),
    )
    .await;

    // Control: both clone while nothing is blocked.
    for id in [plain.id, routed.id] {
        let res = clone_of(&user.token, id).await;
        assert_eq!(res.status_code(), 201, "control clone: {}", res.text());
    }
    let owned_before = links::Entity::find()
        .filter(links::Column::UserId.eq(user.id))
        .count(&db)
        .await
        .unwrap();

    let mut blocked_urls = Vec::new();
    for url in [plain.original_url.clone(), rule_destination.clone()] {
        let blocked = blocked_links::ActiveModel {
            url: Set(url),
            reason: Set(Some("blocked after the link was made".to_string())),
            created_at: Set(Utc::now().naive_utc()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("block url");
        blocked_urls.push(blocked.id);
    }

    let direct = clone_of(&user.token, plain.id).await;
    let via_rule = clone_of(&user.token, routed.id).await;
    let owned_after = links::Entity::find()
        .filter(links::Column::UserId.eq(user.id))
        .count(&db)
        .await
        .unwrap();
    blocked_links::Entity::delete_many()
        .filter(blocked_links::Column::Id.is_in(blocked_urls))
        .exec(&db)
        .await
        .unwrap();

    assert_eq!(
        direct.status_code(),
        403,
        "blocked destination: {}",
        direct.text()
    );
    assert_eq!(
        via_rule.status_code(),
        403,
        "blocked rule destination: {}",
        via_rule.text()
    );
    assert_eq!(owned_after, owned_before, "no link may be created");
}

#[tokio::test]
async fn a_clone_that_cannot_be_completed_leaves_nothing_behind() {
    let scratch = Scratch::new().await;
    let user = person(&scratch.db).await;
    let tag = add_tag(&scratch.db, user.id, "tagged").await;
    let tagged = insert(&scratch.db, new_link(user.id)).await;
    attach(&scratch.db, tagged.id, tag.id).await;
    let routed = insert(&scratch.db, new_link(user.id)).await;
    put_rules_on(&scratch, &user.token, routed.id).await;

    let count = |db: DatabaseConnection, user_id: i32| async move {
        links::Entity::find()
            .filter(links::Column::UserId.eq(user_id))
            .count(&db)
            .await
            .unwrap()
    };
    let clone = |id: i32| {
        let scratch = &scratch;
        let token = user.token.clone();
        async move {
            scratch
                .app()
                .await
                .post(&format!("/links/{id}/clone"))
                .authorization_bearer(&token)
                .await
        }
    };

    // Control: with nothing in the way the clone is made (and deleted again).
    let control = clone(tagged.id).await;
    assert_eq!(control.status_code(), 201, "control: {}", control.text());
    let control_id = control.json::<Value>()["id"].as_i64().unwrap() as i32;
    links::Entity::delete_by_id(control_id)
        .exec(&scratch.db)
        .await
        .unwrap();
    let before = count(scratch.db.clone(), user.id).await;

    // Tag rows can only be written for the original: copying them fails.
    scratch
        .exec(&format!(
            "ALTER TABLE link_tags ADD CONSTRAINT only_the_original CHECK (link_id = {})",
            tagged.id
        ))
        .await;
    let tags_fail = clone(tagged.id).await;
    let after_tags = count(scratch.db.clone(), user.id).await;
    scratch
        .exec("ALTER TABLE link_tags DROP CONSTRAINT only_the_original")
        .await;

    // Same for routing rules.
    scratch
        .exec(&format!(
            "ALTER TABLE routing_rules ADD CONSTRAINT only_the_original CHECK (link_id = {})",
            routed.id
        ))
        .await;
    let rules_fail = clone(routed.id).await;
    let after_rules = count(scratch.db.clone(), user.id).await;

    scratch.drop_database().await;

    assert_eq!(tags_fail.status_code(), 500, "{}", tags_fail.text());
    assert_eq!(
        after_tags, before,
        "a clone without its tags must not exist"
    );
    assert_eq!(rules_fail.status_code(), 500, "{}", rules_fail.text());
    assert_eq!(
        after_rules, before,
        "a clone without its rules must not exist"
    );
}

async fn put_rules_on(scratch: &Scratch, token: &str, link_id: i32) {
    let res = scratch
        .app()
        .await
        .put(&format!("/links/{link_id}/rules"))
        .authorization_bearer(token)
        .json(&json!({ "rules": [
            { "priority": 1, "match_device": "mobile", "destination_url": unique_url() },
        ]}))
        .await;
    assert_eq!(res.status_code(), 200, "put rules: {}", res.text());
}

// ----------------------------------------------------------------------- tags

#[tokio::test]
async fn the_links_of_a_tag_come_back_with_their_tags() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let alpha = add_tag(&db, user.id, "alpha").await;
    let beta = add_tag(&db, user.id, "beta").await;

    let both = insert(&db, new_link(user.id)).await;
    let only_alpha = insert(&db, new_link(user.id)).await;
    let untagged = insert(&db, new_link(user.id)).await;
    attach(&db, both.id, alpha.id).await;
    attach(&db, both.id, beta.id).await;
    attach(&db, only_alpha.id, alpha.id).await;

    let res = send(
        &user.token,
        "GET",
        &format!("/tags/{}/links", alpha.id),
        None,
    )
    .await;
    assert_eq!(res.status_code(), 200, "tag links: {}", res.text());
    let listed: Value = res.json();
    let by_id = |id: i32| {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"].as_i64() == Some(id as i64))
            .unwrap_or_else(|| panic!("link {id} missing from {listed}"))
    };
    let tag_ids = |link: &Value| {
        let mut ids: Vec<i64> = link["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_i64().unwrap())
            .collect();
        ids.sort_unstable();
        ids
    };
    assert_eq!(
        tag_ids(by_id(both.id)),
        vec![alpha.id as i64, beta.id as i64]
    );
    assert_eq!(tag_ids(by_id(only_alpha.id)), vec![alpha.id as i64]);
    assert_eq!(
        link_ids(&listed).len(),
        2,
        "the untagged link is not listed"
    );
    assert!(!link_ids(&listed).contains(&(untagged.id as i64)));
    assert!(by_id(both.id)["tags"][0]["name"].is_string());
}

#[tokio::test]
async fn added_counts_the_tags_a_request_really_attached() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let stranger = person(&db).await;
    let a = add_tag(&db, user.id, "a").await;
    let b = add_tag(&db, user.id, "b").await;
    let c = add_tag(&db, user.id, "c").await;
    let foreign = add_tag(&db, stranger.id, "foreign").await;
    let link = insert(&db, new_link(user.id)).await;

    let add = |ids: Vec<i32>| {
        let token = user.token.clone();
        async move {
            let res = send(
                &token,
                "POST",
                &format!("/links/{}/tags", link.id),
                Some(json!({ "tag_ids": ids })),
            )
            .await;
            assert_eq!(res.status_code(), 200, "add tags: {}", res.text());
            res.json::<Value>()["added"].as_i64().unwrap()
        }
    };

    assert_eq!(add(vec![a.id, b.id]).await, 2);
    assert_eq!(
        add(vec![a.id, b.id, c.id]).await,
        1,
        "a and b were already on it"
    );
    assert_eq!(add(vec![a.id, b.id, c.id]).await, 0);
    assert_eq!(
        add(vec![foreign.id, 987_654_321]).await,
        0,
        "out of scope or unknown"
    );
    assert_eq!(tag_ids_of(&db, link.id).await.len(), 3);
}

/// A request that attaches a tag another request attached a moment earlier
/// (after this one looked and found it missing) must neither fail nor claim
/// the tag. The database plays the other request: a trigger writes the row
/// just before the handler's own insert reaches it.
#[tokio::test]
async fn a_tag_attached_by_a_concurrent_request_is_not_counted_or_an_error() {
    let scratch = Scratch::new().await;
    let user = person(&scratch.db).await;
    let tag = add_tag(&scratch.db, user.id, "contended").await;
    let link = insert(&scratch.db, new_link(user.id)).await;
    scratch
        .exec(
            "CREATE FUNCTION attach_first() RETURNS trigger AS $$
             BEGIN
                 IF pg_trigger_depth() = 1 THEN
                     INSERT INTO link_tags (link_id, tag_id) VALUES (NEW.link_id, NEW.tag_id)
                         ON CONFLICT DO NOTHING;
                 END IF;
                 RETURN NEW;
             END $$ LANGUAGE plpgsql",
        )
        .await;
    scratch
        .exec(
            "CREATE TRIGGER attach_first BEFORE INSERT ON link_tags
             FOR EACH ROW EXECUTE FUNCTION attach_first()",
        )
        .await;

    let res = scratch
        .app()
        .await
        .post(&format!("/links/{}/tags", link.id))
        .authorization_bearer(&user.token)
        .json(&json!({ "tag_ids": [tag.id] }))
        .await;
    let assigned = tag_ids_of(&scratch.db, link.id).await;
    scratch.drop_database().await;

    assert_eq!(res.status_code(), 200, "add tags: {}", res.text());
    assert_eq!(
        res.json::<Value>()["added"],
        0,
        "the other request attached it"
    );
    assert_eq!(assigned, vec![tag.id]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn racing_requests_for_the_same_tag_add_it_once_and_all_succeed() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let tag = add_tag(&db, user.id, "contended").await;
    let link = insert(&db, new_link(user.id)).await;

    // One app for the burst: the point is the interleaving, and every call
    // here is far below the limiter's budget.
    let server = app().await;
    let responses = futures::future::join_all((0..8).map(|_| {
        server
            .post(&format!("/links/{}/tags", link.id))
            .authorization_bearer(&user.token)
            .json(&json!({ "tag_ids": [tag.id] }))
            .into_future()
    }))
    .await;

    let mut added = 0;
    for res in &responses {
        assert_eq!(res.status_code(), 200, "racing add: {}", res.text());
        added += res.json::<Value>()["added"].as_i64().unwrap();
    }
    assert_eq!(added, 1, "exactly one request attached the tag");
    assert_eq!(tag_ids_of(&db, link.id).await, vec![tag.id]);
}

// ------------------------------------------------------------------------ CSV

/// Split a CSV document into rows of cells (quoted cells may hold commas,
/// quotes, CR and LF).
fn csv_rows(csv: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = csv.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut cell)),
            ('\n', false) => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => cell.push(c),
        }
    }
    rows
}

#[tokio::test]
async fn the_export_is_safe_to_open_in_a_spreadsheet() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let expires = chrono::NaiveDate::from_ymd_opt(2031, 1, 2)
        .unwrap()
        .and_hms_opt(3, 4, 5)
        .unwrap();
    let starts = chrono::NaiveDate::from_ymd_opt(2030, 6, 7)
        .unwrap()
        .and_hms_opt(8, 9, 10)
        .unwrap();

    let notes = [
        "=1+1",
        "+1",
        "-1",
        "@SUM(A1)",
        "\t=cmd",
        "\r=cmd",
        "plain note",
        "Привіт, світе",
    ];
    let mut codes = Vec::new();
    for (i, note) in notes.iter().enumerate() {
        let mut link = new_link(user.id);
        link.notes = Set(Some(note.to_string()));
        if i == 0 {
            link.expires_at = Set(Some(expires));
            link.starts_at = Set(Some(starts));
        }
        codes.push(insert(&db, link).await.code);
    }

    let res = send(&user.token, "GET", "/links/export", None).await;
    assert_eq!(res.status_code(), 200, "export: {}", res.text());
    assert_eq!(
        res.maybe_content_type().as_deref(),
        Some("text/csv; charset=utf-8")
    );
    assert!(
        res.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]),
        "a byte order mark makes Excel read UTF-8"
    );
    let text = res.text();
    let rows = csv_rows(text.strip_prefix('\u{feff}').expect("BOM"));
    assert_eq!(rows[0][0], "ID", "the header row follows the BOM");

    let cell_of = |code: &str, column: usize| -> String {
        rows.iter()
            .find(|r| r.get(1).map(String::as_str) == Some(code))
            .unwrap_or_else(|| panic!("no row for {code}"))[column]
            .clone()
    };
    const NOTES: usize = 8;
    for (code, note) in codes.iter().zip(notes) {
        let expected = if note.starts_with(['=', '+', '-', '@', '\t', '\r']) {
            format!("'{note}")
        } else {
            note.to_string()
        };
        assert_eq!(cell_of(code, NOTES), expected, "notes of {code}");
    }

    // Timestamps carry their zone.
    let created = cell_of(&codes[0], 5);
    assert!(created.ends_with('Z'), "created at {created:?}");
    chrono::DateTime::parse_from_rfc3339(&created).expect("created at is RFC 3339");
    assert_eq!(cell_of(&codes[0], 6), "2031-01-02T03:04:05Z");
    assert_eq!(cell_of(&codes[0], 11), "2030-06-07T08:09:10Z");
    assert_eq!(cell_of(&codes[1], 6), "", "no expiry stays empty");
}

// ------------------------------------------------------------- click counting

/// The app plus a handle on its click buffer, for tests that put clicks in it.
async fn app_with_buffer(
    db: &DatabaseConnection,
) -> (
    TestServer,
    std::sync::Arc<opn_onl_backend::utils::ClickBuffer>,
) {
    let state = AppState::for_tests(db.clone()).await;
    let buffer = state.click_buffer.clone();
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("start test server");
    (server, buffer)
}

#[tokio::test]
async fn adding_a_cap_folds_the_buffered_clicks_into_the_count() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut busy = new_link(user.id);
    busy.click_count = Set(100);
    let busy = insert(&db, busy).await;
    let (server, buffer) = app_with_buffer(&db).await;
    buffer.add_pending_count(busy.id, 3).await;

    let res = server
        .put(&format!("/links/{}", busy.id))
        .authorization_bearer(&user.token)
        .json(&json!({ "max_clicks": 500 }))
        .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(res.json::<Value>()["click_count"], 103);
    assert_eq!(row(&db, busy.id).await.click_count, 103);
    assert_eq!(buffer.pending_count(busy.id), 0, "they are counted once");
}

#[tokio::test]
async fn adding_a_cap_does_not_undo_a_flush_that_lands_mid_request() {
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let mut busy = new_link(user.id);
    busy.click_count = Set(100);
    let busy = insert(&db, busy).await;
    let (server, buffer) = app_with_buffer(&db).await;
    buffer.add_pending_count(busy.id, 3).await;

    // Hashing the password is slow, and happens after the link was read and
    // before it is written. The "flush" (the same statement the click buffer
    // runs) lands in that gap.
    let update = async {
        server
            .put(&format!("/links/{}", busy.id))
            .authorization_bearer(&user.token)
            .json(&json!({ "max_clicks": 500, "password": "a-long-enough-secret" }))
            .await
    };
    let flush = async {
        tokio::time::sleep(StdDuration::from_millis(100)).await;
        db.execute_unprepared(&format!(
            "UPDATE links SET click_count = click_count + 7 WHERE id = {}",
            busy.id
        ))
        .await
        .expect("flush");
    };
    let (res, ()) = tokio::join!(update, flush);

    assert_eq!(res.status_code(), 200, "update: {}", res.text());
    assert_eq!(
        row(&db, busy.id).await.click_count,
        110,
        "100 stored + 7 flushed meanwhile + 3 that were buffered"
    );
}
