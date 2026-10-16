//! Data-lifecycle bugs: a one-time link saved with no click cap, admin list
//! pages that tie on a non-unique column, and a short-code lookup that treats
//! a database error as "free".

mod common;

use axum::http::header;
use common::{mark_email_verified, setup_test_db, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use opn_onl_backend::handlers::links::unused_code;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, Database, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, Statement, TransactionTrait,
};
use serde_json::{Value, json};

const DESKTOP_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

const ONE_TIME_OPENED: &str = "This one-time link has already been opened";

async fn register(server: &axum_test::TestServer, email: &str) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register {email}: {}", res.text());
    let body: Value = res.json();
    (
        body["token"].as_str().expect("token").to_string(),
        body["user_id"].as_i64().expect("user_id") as i32,
    )
}

async fn make_admin(db: &DatabaseConnection, user_id: i32) {
    use opn_onl_backend::entity::users;
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("db")
        .expect("user");
    let mut active: users::ActiveModel = user.into();
    active.is_admin = Set(true);
    active.update(db).await.expect("promote");
}

fn ids_of(body: &Value) -> Vec<i64> {
    body["links"]
        .as_array()
        .expect("links array")
        .iter()
        .map(|link| link["id"].as_i64().expect("id"))
        .collect()
}

/// A row v1.4.0 could save: `burn_after_reading` with no cap. The first visit
/// redirects; the second is the same dead answer a burned one-time link gets,
/// and `burned_at` is set.
#[tokio::test]
async fn a_burn_link_saved_without_a_cap_burns_on_first_visit() {
    let (server, db) = spawn_real_app().await;
    let code = unique_code();
    let dest = format!("https://iana.org/legacy-burn-{code}");
    links::ActiveModel {
        code: Set(code.clone()),
        original_url: Set(dest.clone()),
        burn_after_reading: Set(true),
        max_clicks: Set(None),
        click_count: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert the legacy row");

    let stored = links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .one(&db)
        .await
        .expect("db")
        .expect("row");
    assert!(stored.burn_after_reading);
    assert!(
        stored.max_clicks.is_none(),
        "the row must still have no cap"
    );
    assert!(stored.burned_at.is_none());

    let first = server
        .get(&format!("/{code}"))
        .add_header(header::USER_AGENT, DESKTOP_UA)
        .await;
    assert_eq!(first.status_code(), 307, "first visit: {}", first.text());
    let location = first
        .maybe_header(header::LOCATION)
        .map(|v| v.to_str().expect("location").to_string());
    assert_eq!(location.as_deref(), Some(dest.as_str()));

    let burned = links::Entity::find()
        .filter(links::Column::Code.eq(&code))
        .one(&db)
        .await
        .expect("db")
        .expect("row");
    assert!(
        burned.burned_at.is_some(),
        "the first visit must stamp burned_at, got click_count {}",
        burned.click_count
    );
    assert_eq!(burned.click_count, 1);

    let second = server
        .get(&format!("/{code}"))
        .add_header(header::USER_AGENT, DESKTOP_UA)
        .await;
    assert_eq!(second.status_code(), 410, "second visit: {}", second.text());
    assert_eq!(second.text(), ONE_TIME_OPENED);

    // Control: a burn link that already has its cap of 1 dies the same way.
    let (token, user_id) = register(&server, &unique_email()).await;
    mark_email_verified(&db, user_id).await;
    let normal_code = unique_code();
    let normal_dest = format!("https://iana.org/capped-burn-{normal_code}");
    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": normal_dest,
            "custom_alias": normal_code,
            "burn_after_reading": true,
        }))
        .await;
    assert_eq!(
        created.status_code(),
        201,
        "control create: {}",
        created.text()
    );
    let normal_second = server
        .get(&format!("/{normal_code}"))
        .add_header(header::USER_AGENT, DESKTOP_UA)
        .await;
    // Spend the one click, then the next visit must match the legacy answer.
    assert_eq!(
        normal_second.status_code(),
        307,
        "control first visit: {}",
        normal_second.text()
    );
    let normal_dead = server
        .get(&format!("/{normal_code}"))
        .add_header(header::USER_AGENT, DESKTOP_UA)
        .await;
    assert_eq!(
        normal_dead.status_code(),
        410,
        "control: {}",
        normal_dead.text()
    );
    assert_eq!(normal_dead.text(), second.text());
}

/// Thirty links with the same click count, sorted by clicks. Page 1 and page 2
/// are disjoint and together they are the whole set, in id order within the tie.
#[tokio::test]
async fn admin_link_pages_do_not_repeat_rows_that_tie_on_clicks() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &unique_email()).await;
    make_admin(&db, admin_id).await;
    let (_token, user_id) = register(&server, &unique_email()).await;
    mark_email_verified(&db, user_id).await;

    let prefix = unique_code();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO links (code, original_url, user_id, click_count) \
         SELECT $1 || g::text, 'https://iana.org/tie/' || g::text, $2, 0 \
         FROM generate_series(1, 30) AS g",
        vec![prefix.into(), user_id.into()],
    ))
    .await
    .expect("seed");

    let mut ids: Vec<i64> = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .all(&db)
        .await
        .expect("db")
        .into_iter()
        .map(|link| i64::from(link.id))
        .collect();
    ids.sort_unstable();
    assert_eq!(ids.len(), 30, "seed");
    // A middle id, so "first when sorted by clicks" cannot be explained by
    // the insertion order. The lowest id would lead an id-ascending tie too.
    let bumped = ids[ids.len() / 2];
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE links SET click_count = 3 WHERE id = $1",
        vec![bumped.into()],
    ))
    .await
    .expect("bump");

    // clicks DESC, and id DESC inside the tie. The bumped id leads because
    // its click count is the only one above zero.
    let mut tied: Vec<i64> = ids.iter().copied().filter(|id| *id != bumped).collect();
    tied.sort_unstable_by(|a, b| b.cmp(a));
    let expected: Vec<i64> = std::iter::once(bumped).chain(tied).collect();

    let page = async |page: u64, order: String| {
        let res = server
            .get("/admin/links")
            .add_query_param("user_id", user_id.to_string())
            .add_query_param("sort", "clicks")
            .add_query_param("order", order)
            .add_query_param("per_page", "15")
            .add_query_param("page", page.to_string())
            .authorization_bearer(&admin_token)
            .await;
        assert_eq!(res.status_code(), 200, "page {page}: {}", res.text());
        let body: Value = res.json();
        assert_eq!(body["total"].as_u64(), Some(30), "{body}");
        ids_of(&body)
    };

    let desc_1 = page(1, "desc".to_string()).await;
    let desc_2 = page(2, "desc".to_string()).await;
    let mut union = desc_1.clone();
    union.extend(desc_2.iter().copied());
    union.sort_unstable();
    let mut all = expected.clone();
    all.sort_unstable();
    assert_eq!(union, all, "the two pages must cover every link once");
    assert!(
        desc_1.iter().all(|id| !desc_2.contains(id)),
        "page 1 and page 2 overlap: {desc_1:?} vs {desc_2:?}"
    );
    assert_eq!(desc_1, expected[..15], "clicks desc, id desc on a tie");
    assert_eq!(desc_2, expected[15..]);

    // The other direction: the high click count is last, and the tie is id asc.
    let mut tied_asc: Vec<i64> = ids.iter().copied().filter(|id| *id != bumped).collect();
    tied_asc.sort_unstable();
    let expected_asc: Vec<i64> = tied_asc
        .into_iter()
        .chain(std::iter::once(bumped))
        .collect();
    let asc_1 = page(1, "asc".to_string()).await;
    let asc_2 = page(2, "asc".to_string()).await;
    assert_eq!(asc_1, expected_asc[..15], "clicks asc, id asc on a tie");
    assert_eq!(asc_2, expected_asc[15..]);
}

/// A lookup that fails must not be treated as "this code is free".
#[tokio::test]
async fn a_failed_code_lookup_does_not_hand_out_the_code() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let db = Database::connect(&url).await.expect("connect");
    // Migrations may already have run in this process. Closing this pool
    // must not touch the suite's other connections.
    db.close_by_ref().await.expect("close");

    let probe = links::Entity::find().one(&db).await;
    assert!(
        probe.is_err(),
        "the closed pool must fail a lookup, got {probe:?}"
    );

    let candidate = unique_code();
    let issued = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        unused_code(&db, || candidate.clone()),
    )
    .await
    .expect("a failed lookup must not hang");
    assert!(
        issued.is_err(),
        "a failed lookup handed out {candidate}: {issued:?}"
    );

    // Control: a live database still issues the first code nobody holds.
    let live = setup_test_db().await;
    let free = unique_code();
    let got = unused_code(&live, || free.clone())
        .await
        .expect("live pool");
    assert_eq!(got, free);
}

/// The repair sets a cap of 1 only where a one-time link has none, and a
/// second run changes nothing.
#[tokio::test]
async fn migration_caps_only_burn_links_missing_a_cap() {
    let db = setup_test_db().await;
    // The repair updates every uncapped one-time link. Keep it in a transaction
    // and roll it back, or it would cap a row another test inserted on purpose.
    let db = db.begin().await.expect("transaction");

    async fn insert(
        db: &impl ConnectionTrait,
        burn: bool,
        max_clicks: Option<i32>,
        click_count: i64,
        deleted: bool,
    ) -> links::Model {
        links::ActiveModel {
            code: Set(unique_code()),
            original_url: Set("https://iana.org/repair".to_string()),
            burn_after_reading: Set(burn),
            max_clicks: Set(max_clicks),
            click_count: Set(click_count),
            deleted_at: Set(deleted.then(|| chrono::Utc::now().naive_utc())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert")
    }

    let fresh = insert(&db, true, None, 0, false).await;
    let used_and_deleted = insert(&db, true, None, 4, true).await;
    let already_capped = insert(&db, true, Some(5), 0, false).await;
    let plain = insert(&db, false, None, 0, false).await;

    migration::cap_uncapped_burn_links(&db)
        .await
        .expect("repair");
    migration::cap_uncapped_burn_links(&db)
        .await
        .expect("second run is a no-op");

    async fn reload(db: &impl ConnectionTrait, id: i32) -> links::Model {
        links::Entity::find_by_id(id)
            .one(db)
            .await
            .expect("db")
            .expect("row")
    }

    let fresh = reload(&db, fresh.id).await;
    assert_eq!(fresh.max_clicks, Some(1));
    assert_eq!(fresh.click_count, 0);
    assert!(
        fresh.burned_at.is_none(),
        "the repair must not stamp burned_at"
    );

    let used_and_deleted = reload(&db, used_and_deleted.id).await;
    assert_eq!(used_and_deleted.max_clicks, Some(1));
    assert_eq!(used_and_deleted.click_count, 4);
    assert!(used_and_deleted.burned_at.is_none());
    assert!(used_and_deleted.deleted_at.is_some());

    assert_eq!(reload(&db, already_capped.id).await.max_clicks, Some(5));
    assert_eq!(reload(&db, plain.id).await.max_clicks, None);
    db.rollback().await.expect("rollback");
}

fn ids_named(body: &Value, key: &str) -> Vec<i64> {
    body[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} array in {body}"))
        .iter()
        .map(|row| row["id"].as_i64().expect("id"))
        .collect()
}

/// Users created in one statement share `created_at`. Page 1 and page 2 follow
/// id inside that tie, and a later `created_at` still leads or trails.
#[tokio::test]
async fn admin_user_pages_break_ties_on_id() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &unique_email()).await;
    make_admin(&db, admin_id).await;

    let token = uuid::Uuid::new_v4().simple().to_string();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO users (email, password_hash, created_at) \
         SELECT $1 || g::text || '@example.com', 'x', TIMESTAMP '2020-01-01 00:00:00' \
         FROM generate_series(1, 30) AS g",
        vec![token.clone().into()],
    ))
    .await
    .expect("seed users");

    let mut ids: Vec<i64> = {
        use opn_onl_backend::entity::users;
        users::Entity::find()
            .filter(users::Column::Email.contains(&token))
            .all(&db)
            .await
            .expect("db")
            .into_iter()
            .map(|user| i64::from(user.id))
            .collect()
    };
    ids.sort_unstable();
    assert_eq!(ids.len(), 30, "seed");
    let bumped = ids[ids.len() / 2];
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE users SET created_at = TIMESTAMP '2024-06-01 00:00:00' WHERE id = $1",
        vec![bumped.into()],
    ))
    .await
    .expect("bump");

    let mut tied: Vec<i64> = ids.iter().copied().filter(|id| *id != bumped).collect();
    tied.sort_unstable_by(|a, b| b.cmp(a));
    let expected: Vec<i64> = std::iter::once(bumped).chain(tied).collect();

    let page = async |page: u64, order: String| {
        let res = server
            .get("/admin/users")
            .add_query_param("search", &token)
            .add_query_param("order", order)
            .add_query_param("per_page", "15")
            .add_query_param("page", page.to_string())
            .authorization_bearer(&admin_token)
            .await;
        assert_eq!(res.status_code(), 200, "users page {page}: {}", res.text());
        let body: Value = res.json();
        assert_eq!(body["total"].as_u64(), Some(30), "{body}");
        ids_named(&body, "users")
    };

    let desc_1 = page(1, "desc".to_string()).await;
    let desc_2 = page(2, "desc".to_string()).await;
    assert_eq!(desc_1, expected[..15], "created_at desc, id desc on a tie");
    assert_eq!(desc_2, expected[15..]);

    let mut tied_asc: Vec<i64> = ids.iter().copied().filter(|id| *id != bumped).collect();
    tied_asc.sort_unstable();
    let expected_asc: Vec<i64> = tied_asc
        .into_iter()
        .chain(std::iter::once(bumped))
        .collect();
    let asc_1 = page(1, "asc".to_string()).await;
    let asc_2 = page(2, "asc".to_string()).await;
    assert_eq!(asc_1, expected_asc[..15], "created_at asc, id asc on a tie");
    assert_eq!(asc_2, expected_asc[15..]);
}

/// Organizations are always newest first. Equal `created_at` breaks on id desc.
#[tokio::test]
async fn admin_org_pages_break_ties_on_id() {
    let (server, db) = spawn_real_app().await;
    let (admin_token, admin_id) = register(&server, &unique_email()).await;
    make_admin(&db, admin_id).await;
    let (_token, owner_id) = register(&server, &unique_email()).await;

    let token = uuid::Uuid::new_v4().simple().to_string();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO organizations (name, slug, owner_id, created_at) \
         SELECT $1 || g::text, $1 || g::text, $2, TIMESTAMP '2020-01-01 00:00:00' \
         FROM generate_series(1, 30) AS g",
        vec![token.clone().into(), owner_id.into()],
    ))
    .await
    .expect("seed orgs");

    let mut ids: Vec<i64> = {
        use opn_onl_backend::entity::organizations;
        organizations::Entity::find()
            .filter(organizations::Column::Slug.starts_with(&token))
            .all(&db)
            .await
            .expect("db")
            .into_iter()
            .map(|org| i64::from(org.id))
            .collect()
    };
    ids.sort_unstable();
    assert_eq!(ids.len(), 30, "seed");
    let bumped = ids[ids.len() / 2];
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE organizations SET created_at = TIMESTAMP '2024-06-01 00:00:00' WHERE id = $1",
        vec![bumped.into()],
    ))
    .await
    .expect("bump");

    let mut tied: Vec<i64> = ids.iter().copied().filter(|id| *id != bumped).collect();
    tied.sort_unstable_by(|a, b| b.cmp(a));
    let expected: Vec<i64> = std::iter::once(bumped).chain(tied).collect();

    let page = async |page: u64| {
        let res = server
            .get("/admin/orgs")
            .add_query_param("search", &token)
            .add_query_param("per_page", "15")
            .add_query_param("page", page.to_string())
            .authorization_bearer(&admin_token)
            .await;
        assert_eq!(res.status_code(), 200, "orgs page {page}: {}", res.text());
        let body: Value = res.json();
        assert_eq!(body["total"].as_u64(), Some(30), "{body}");
        ids_named(&body, "orgs")
    };

    let page_1 = page(1).await;
    let page_2 = page(2).await;
    assert_eq!(page_1, expected[..15], "created_at desc, id desc on a tie");
    assert_eq!(page_2, expected[15..]);
}
