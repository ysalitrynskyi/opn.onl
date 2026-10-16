//! Link-in-bio handler tests. Real router + real Postgres via
//! `common::spawn_real_app`.

mod common;

use common::{mark_email_verified, spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::links;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, Statement,
};
use serde_json::{Value, json};

async fn register_verified(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
) -> (String, i32) {
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": unique_email(), "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let token = body["token"].as_str().unwrap().to_string();
    let user_id = body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(db, user_id).await;
    (token, user_id)
}

fn unique_bio_username() -> String {
    format!("bio{}", unique_code().to_lowercase())
}

/// Anonymous GET /api/bio/{username} must not include per-link click counts.
/// Traffic volume for a short link is owner analytics, not public profile data.
#[tokio::test]
async fn public_bio_omits_per_link_click_counts() {
    let (server, db) = spawn_real_app().await;
    let (token, _user_id) = register_verified(&server, &db).await;
    let username = unique_bio_username();

    let settings = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({
            "bio_username": username,
            "bio_enabled": true,
        }))
        .await;
    assert_eq!(
        settings.status_code(),
        200,
        "enable bio: {}",
        settings.text()
    );

    let created = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/bio-click-leak",
            "title": "Public link",
            "bio_visible": true,
        }))
        .await;
    assert_eq!(
        created.status_code(),
        201,
        "create link: {}",
        created.text()
    );
    let link_id = created.json::<Value>()["id"].as_i64().unwrap() as i32;
    let code = created.json::<Value>()["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Some create paths ignore bio_visible; force it on the stored row either way.
    let make_visible = server
        .put(&format!("/links/{link_id}"))
        .authorization_bearer(&token)
        .json(&json!({ "bio_visible": true }))
        .await;
    assert!(
        make_visible.status_code().is_success(),
        "mark bio-visible: {}",
        make_visible.text()
    );

    let model = links::Entity::find_by_id(link_id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    let mut active: links::ActiveModel = model.into();
    active.click_count = Set(42);
    active.update(&db).await.expect("set click_count");

    let res = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(res.status_code(), 200, "public bio: {}", res.text());
    let body: Value = res.json();
    let links = body["links"].as_array().expect("links array");
    assert_eq!(links.len(), 1, "one bio-visible link: {body}");
    assert_eq!(links[0]["code"].as_str(), Some(code.as_str()));
    assert_eq!(links[0]["label"].as_str(), Some("Public link"));
    assert!(
        links[0].get("click_count").is_none(),
        "public bio must not leak click_count, got {}",
        links[0]
    );
}

/// Two callers can both pass the read-then-write uniqueness check. The loser
/// then hits `idx-users-bio_username`. That unique violation must be 409, not
/// the generic 500 from `update`.
///
/// A BEFORE UPDATE trigger assigns the chosen username to another user after
/// the pre-check, so this is a deterministic unique-index collision rather
/// than a flake-prone race.
#[tokio::test]
async fn bio_username_unique_violation_returns_409() {
    let (server, db) = spawn_real_app().await;
    let (_token_a, user_a) = register_verified(&server, &db).await;
    let (token_b, user_b) = register_verified(&server, &db).await;
    let username = unique_bio_username();

    let suffix = unique_code().to_lowercase();
    let fn_name = format!("steal_bio_un_{suffix}");
    db.execute_unprepared(&format!(
        r#"
        CREATE FUNCTION {fn_name}() RETURNS trigger AS $$
        BEGIN
            UPDATE users SET bio_username = NEW.bio_username WHERE id = {user_a};
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql
        "#
    ))
    .await
    .expect("create steal-username function");
    db.execute_unprepared(&format!(
        r#"
        CREATE TRIGGER {fn_name}
        BEFORE UPDATE ON users
        FOR EACH ROW
        WHEN (OLD.id = {user_b} AND NEW.bio_username IS NOT NULL)
        EXECUTE PROCEDURE {fn_name}()
        "#
    ))
    .await
    .expect("install steal-username trigger");

    let res = server
        .put("/auth/bio")
        .authorization_bearer(&token_b)
        .json(&json!({ "bio_username": username }))
        .await;

    let _ = db
        .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {fn_name} ON users"))
        .await;
    let _ = db
        .execute_unprepared(&format!("DROP FUNCTION IF EXISTS {fn_name}()"))
        .await;

    assert_eq!(
        res.status_code(),
        409,
        "unique-index collision on bio_username must be 409, got {}: {}",
        res.status_code(),
        res.text()
    );
    assert!(
        res.text().contains("That username is taken"),
        "documented 409 body, got {}",
        res.text()
    );
}

async fn enable_bio(server: &axum_test::TestServer, token: &str, theme: &str) -> String {
    let username = unique_bio_username();
    let res = server
        .put("/auth/bio")
        .authorization_bearer(token)
        .json(&json!({
            "bio_username": username,
            "bio_enabled": true,
            "bio_theme": theme,
        }))
        .await;
    assert_eq!(res.status_code(), 200, "enable bio: {}", res.text());
    username
}

async fn create_link(server: &axum_test::TestServer, token: &str, title: &str) -> i32 {
    let res = server
        .post("/links")
        .authorization_bearer(token)
        .json(&json!({
            "original_url": format!("https://iana.org/bio-{}", unique_code()),
            "title": title,
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create {title}: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn put_link(
    server: &axum_test::TestServer,
    token: &str,
    id: i32,
    body: Value,
) -> axum_test::TestResponse {
    server
        .put(&format!("/links/{id}"))
        .authorization_bearer(token)
        .json(&body)
        .await
}

fn public_labels(body: &Value) -> Vec<String> {
    body["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|link| link["label"].as_str().unwrap().to_string())
        .collect()
}

/// The bio page already showed `bio_label` and `bio_position`, but a link
/// update dropped both, so the page could only ever show the title.
#[tokio::test]
async fn bio_label_and_position_can_be_set_and_cleared() {
    let (server, db) = spawn_real_app().await;
    let (token, _user_id) = register_verified(&server, &db).await;
    let username = enable_bio(&server, &token, "ink").await;
    let id = create_link(&server, &token, "Title").await;

    let eighty = "你".repeat(80);
    // Spaces count toward the cap, so padding around an already-full label is refused.
    let padded = put_link(
        &server,
        &token,
        id,
        json!({
            "bio_visible": true,
            "bio_label": format!("  {eighty}"),
            "bio_position": 4,
        }),
    )
    .await;
    assert_eq!(padded.status_code(), 400, "padded label: {}", padded.text());
    assert!(
        padded.text().contains("Bio label must be at most 80"),
        "got {}",
        padded.text()
    );

    let set = put_link(
        &server,
        &token,
        id,
        json!({
            "bio_visible": true,
            "bio_label": eighty,
            "bio_position": 4,
        }),
    )
    .await;
    assert_eq!(set.status_code(), 200, "80 chars: {}", set.text());
    let body: Value = set.json();
    assert_eq!(body["bio_label"].as_str(), Some("你".repeat(80).as_str()));
    assert_eq!(body["bio_position"], 4);
    assert_eq!(body["bio_visible"], true);

    let too_long = put_link(&server, &token, id, json!({ "bio_label": "你".repeat(81) })).await;
    assert_eq!(too_long.status_code(), 400, "81 chars: {}", too_long.text());

    // A title edit must not wipe the label or the position.
    let renamed = put_link(&server, &token, id, json!({ "title": "Renamed" })).await;
    assert_eq!(renamed.status_code(), 200, "rename: {}", renamed.text());
    let body: Value = renamed.json();
    assert_eq!(body["title"].as_str(), Some("Renamed"));
    assert_eq!(body["bio_label"].as_str(), Some("你".repeat(80).as_str()));
    assert_eq!(body["bio_position"], 4);

    let stored = links::Entity::find_by_id(id)
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    assert_eq!(stored.bio_label.as_deref(), Some("你".repeat(80).as_str()));
    assert_eq!(stored.bio_position, Some(4));

    // Registration plus the writes above fill the 10-per-second bucket on a
    // fast machine, and a 429 here would hide whether the clear worked.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let cleared = put_link(
        &server,
        &token,
        id,
        json!({ "bio_label": "   ", "remove_bio_position": true, "bio_position": 9 }),
    )
    .await;
    assert_eq!(cleared.status_code(), 200, "clear: {}", cleared.text());
    let body: Value = cleared.json();
    assert_eq!(
        body["bio_label"],
        Value::Null,
        "whitespace clears the label"
    );
    assert_eq!(
        body["bio_position"],
        Value::Null,
        "remove_bio_position wins over a position in the same request"
    );

    let page = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(page.status_code(), 200, "public: {}", page.text());
    let page: Value = page.json();
    assert_eq!(page["theme"].as_str(), Some("ink"));
    assert_eq!(
        public_labels(&page),
        vec!["Renamed".to_string()],
        "a cleared label falls back to the title"
    );

    let shop = put_link(&server, &token, id, json!({ "bio_label": "  Shop  " })).await;
    assert_eq!(shop.status_code(), 200, "trim: {}", shop.text());
    assert_eq!(shop.json::<Value>()["bio_label"].as_str(), Some("Shop"));
    let page = server
        .get(&format!("/api/bio/{username}"))
        .await
        .json::<Value>();
    assert_eq!(public_labels(&page), vec!["Shop".to_string()]);

    let unknown = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({ "bio_theme": "nope" }))
        .await;
    assert_eq!(
        unknown.status_code(),
        400,
        "unknown theme: {}",
        unknown.text()
    );
    assert!(
        unknown.text().contains("Theme must be"),
        "{}",
        unknown.text()
    );

    let still = server
        .get("/auth/me")
        .authorization_bearer(&token)
        .await
        .json::<Value>();
    assert_eq!(
        still["bio_theme"].as_str(),
        Some("ink"),
        "refused theme sticks"
    );

    let cleared_theme = server
        .put("/auth/bio")
        .authorization_bearer(&token)
        .json(&json!({ "bio_theme": "" }))
        .await;
    assert_eq!(
        cleared_theme.status_code(),
        200,
        "clear theme: {}",
        cleared_theme.text()
    );
    assert_eq!(cleared_theme.json::<Value>()["bio_theme"], Value::Null);
}

/// Ordering is one request: a foreign id, a hidden link, an organization
/// link or a partial list must not move the links that were already ordered.
#[tokio::test]
async fn bio_order_rejects_foreign_and_hidden_links() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    let (other_token, _other) = register_verified(&server, &db).await;
    let username = enable_bio(&server, &token, "cobalt").await;

    let mut ids = Vec::new();
    for (title, label, position) in [
        ("First", "Alpha", 0),
        ("Second", "Beta", 1),
        ("Third", "Gamma", 2),
    ] {
        let id = create_link(&server, &token, title).await;
        let res = put_link(
            &server,
            &token,
            id,
            json!({ "bio_visible": true, "bio_label": label, "bio_position": position }),
        )
        .await;
        assert_eq!(res.status_code(), 200, "show {title}: {}", res.text());
        ids.push(id);
    }
    // Two registrations plus those writes already fill the 10-per-second
    // bucket, so the refusals below would otherwise come back as 429.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let hidden = create_link(&server, &token, "Hidden").await;
    let hidden_put = put_link(&server, &token, hidden, json!({ "bio_visible": false })).await;
    assert_eq!(hidden_put.status_code(), 200, "hide: {}", hidden_put.text());

    let foreign = create_link(&server, &other_token, "Foreign").await;
    let foreign_put = put_link(
        &server,
        &other_token,
        foreign,
        json!({ "bio_visible": true, "bio_label": "Foreign" }),
    )
    .await;
    assert_eq!(
        foreign_put.status_code(),
        200,
        "foreign visible: {}",
        foreign_put.text()
    );

    let org = server
        .post("/orgs")
        .authorization_bearer(&token)
        .json(&json!({
            "name": "Bio Org",
            "slug": format!("bio-{}", unique_code().to_lowercase()),
        }))
        .await;
    assert_eq!(org.status_code(), 201, "org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap();
    let org_link = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/org-bio",
            "title": "Org link",
            "org_id": org_id,
        }))
        .await;
    assert_eq!(org_link.status_code(), 201, "org link: {}", org_link.text());
    let org_link_id = org_link.json::<Value>()["id"].as_i64().unwrap() as i32;
    let org_visible = put_link(
        &server,
        &token,
        org_link_id,
        json!({ "bio_visible": true, "bio_label": "Org" }),
    )
    .await;
    assert_eq!(
        org_visible.status_code(),
        200,
        "org bio: {}",
        org_visible.text()
    );

    let page = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(page.status_code(), 200, "before: {}", page.text());
    let before = page.json::<Value>();
    assert_eq!(
        public_labels(&before),
        vec![
            "Alpha".to_string(),
            "Beta".to_string(),
            "Gamma".to_string(),
            "Org".to_string()
        ],
        "personal links follow bio_position; the org link has none so it is last"
    );
    assert_eq!(before["theme"].as_str(), Some("cobalt"));

    async fn order(
        server: &axum_test::TestServer,
        token: &str,
        ids: Vec<i32>,
    ) -> axum_test::TestResponse {
        server
            .put("/auth/bio/order")
            .authorization_bearer(token)
            .json(&json!({ "link_ids": ids }))
            .await
    }

    // Creating the hidden, foreign and organization links used the bucket again.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let foreign_order = order(&server, &other_token, ids.clone()).await;
    assert_eq!(
        foreign_order.status_code(),
        403,
        "foreign: {}",
        foreign_order.text()
    );
    assert!(
        foreign_order.text().contains("don't have permission"),
        "{}",
        foreign_order.text()
    );

    // A foreign id mixed into the caller's own complete list is still a 403,
    // not "list every": existence of the other id stays hidden.
    let foreign_first = order(&server, &token, {
        let mut bad = ids.clone();
        bad.insert(0, foreign);
        bad
    })
    .await;
    assert_eq!(
        foreign_first.status_code(),
        403,
        "foreign first: {}",
        foreign_first.text()
    );
    assert!(
        foreign_first.text().contains("don't have permission"),
        "{}",
        foreign_first.text()
    );

    let hidden_order = order(&server, &token, {
        let mut bad = ids.clone();
        bad.insert(0, hidden);
        bad
    })
    .await;
    assert_eq!(
        hidden_order.status_code(),
        400,
        "hidden: {}",
        hidden_order.text()
    );
    assert!(
        hidden_order
            .text()
            .contains("Only links shown on your bio page"),
        "{}",
        hidden_order.text()
    );

    let org_order = order(&server, &token, {
        let mut bad = ids.clone();
        bad.push(org_link_id);
        bad
    })
    .await;
    assert_eq!(org_order.status_code(), 400, "org: {}", org_order.text());
    assert!(
        org_order.text().contains("Only personal links"),
        "{}",
        org_order.text()
    );

    let partial = order(&server, &token, vec![ids[0], ids[1]]).await;
    assert_eq!(partial.status_code(), 400, "partial: {}", partial.text());
    assert!(
        partial.text().contains("List every personal link"),
        "{}",
        partial.text()
    );

    let dup = order(&server, &token, vec![ids[0], ids[0], ids[1]]).await;
    assert_eq!(dup.status_code(), 400, "duplicate: {}", dup.text());
    assert!(dup.text().contains("only once"), "{}", dup.text());

    // None of the refusals moved the page. A normal edit still works.
    let still = server
        .get(&format!("/api/bio/{username}"))
        .await
        .json::<Value>();
    assert_eq!(public_labels(&still), public_labels(&before));
    let touch = put_link(&server, &token, ids[0], json!({ "title": "First" })).await;
    assert_eq!(touch.status_code(), 200, "control edit: {}", touch.text());
    assert_eq!(touch.json::<Value>()["bio_label"].as_str(), Some("Alpha"));

    let reversed = vec![ids[2], ids[1], ids[0]];
    // The refusals above are a burst, and the API allows 10 requests a second.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let saved = order(&server, &token, reversed.clone()).await;
    assert_eq!(saved.status_code(), 200, "order: {}", saved.text());
    assert_eq!(
        saved.json::<Value>()["link_ids"].as_array().unwrap().len(),
        3
    );

    let page = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(page.status_code(), 200, "after: {}", page.text());
    assert_eq!(
        public_labels(&page.json()),
        vec![
            "Gamma".to_string(),
            "Beta".to_string(),
            "Alpha".to_string(),
            "Org".to_string()
        ]
    );

    let row = links::Entity::find_by_id(ids[2])
        .one(&db)
        .await
        .expect("db")
        .expect("link");
    assert_eq!(row.user_id, Some(user_id));
    assert_eq!(row.bio_position, Some(0));
    assert_eq!(row.bio_label.as_deref(), Some("Gamma"));

    let too_many: Vec<i32> = (1..=501).collect();
    let capped = order(&server, &token, too_many).await;
    assert_eq!(capped.status_code(), 400, "cap: {}", capped.text());
    assert!(
        capped.text().contains("Too many items"),
        "{}",
        capped.text()
    );
}

async fn exec(db: &DatabaseConnection, sql: &str, values: Vec<sea_orm::Value>) {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .await
    .unwrap_or_else(|err| panic!("{sql}: {err}"));
}

async fn stored_link(db: &DatabaseConnection, id: i32) -> links::Model {
    links::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("db")
        .expect("link")
}

fn total_count(res: &axum_test::TestResponse) -> u64 {
    res.headers()
        .get("x-total-count")
        .expect("X-Total-Count")
        .to_str()
        .unwrap()
        .parse()
        .expect("X-Total-Count is a number")
}

/// `bio_visible` is the filter the settings page uses so a personal bio link
/// is not crowded off the first page of every link the account owns.
#[tokio::test]
async fn bio_visible_filter_lists_only_matching_links() {
    let (server, db) = spawn_real_app().await;
    let (token, _user_id) = register_verified(&server, &db).await;
    let shown = create_link(&server, &token, "Shown").await;
    let hidden = create_link(&server, &token, "Hidden").await;
    let mark = put_link(&server, &token, shown, json!({ "bio_visible": true })).await;
    assert_eq!(mark.status_code(), 200, "show: {}", mark.text());

    let yes = server
        .get("/links?bio_visible=true")
        .authorization_bearer(&token)
        .await;
    assert_eq!(yes.status_code(), 200, "visible: {}", yes.text());
    assert_eq!(total_count(&yes), 1);
    let yes_ids: Vec<i64> = yes
        .json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect();
    assert_eq!(yes_ids, vec![shown as i64]);

    let no = server
        .get("/links?bio_visible=false")
        .authorization_bearer(&token)
        .await;
    assert_eq!(no.status_code(), 200, "hidden: {}", no.text());
    assert_eq!(total_count(&no), 1);
    assert_eq!(no.json::<Value>()[0]["id"].as_i64(), Some(hidden as i64));

    // The unfiltered list still returns both. The new query must not become
    // the default.
    let all = server.get("/links").authorization_bearer(&token).await;
    assert_eq!(all.status_code(), 200, "all: {}", all.text());
    assert_eq!(total_count(&all), 2);
}

/// Equal `created_at` values follow id descending, and a later fraction sorts
/// ahead of an earlier one. A string compare of the RFC 3339 form swaps `.1Z`
/// and `.12Z`.
#[tokio::test]
async fn public_bio_breaks_equal_instants_by_newest_id() {
    let (server, db) = spawn_real_app().await;
    let (token, _user_id) = register_verified(&server, &db).await;
    let username = enable_bio(&server, &token, "ink").await;

    let early = create_link(&server, &token, "Early").await;
    let later = create_link(&server, &token, "Later").await;
    let low = create_link(&server, &token, "Low").await;
    let high = create_link(&server, &token, "High").await;
    assert!(high > low, "the later row has the higher id");

    for (id, label, at) in [
        (early, "Early", "2026-01-01 00:00:00.100000"),
        (later, "Later", "2026-01-01 00:00:00.120000"),
        (low, "Low", "2026-01-02 00:00:00"),
        (high, "High", "2026-01-02 00:00:00"),
    ] {
        exec(
            &db,
            "UPDATE links SET bio_visible = true, bio_position = NULL, bio_label = $2, created_at = $3::timestamp WHERE id = $1",
            vec![id.into(), label.into(), at.into()],
        )
        .await;
    }

    let page = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(page.status_code(), 200, "page: {}", page.text());
    assert_eq!(
        public_labels(&page.json()),
        vec![
            "High".to_string(),
            "Low".to_string(),
            "Later".to_string(),
            "Early".to_string()
        ]
    );
}

/// Hiding drops the saved place. Showing the link again, even with a place
/// that was written while it was hidden, leaves it after every positioned link.
#[tokio::test]
async fn hiding_a_bio_link_clears_its_position() {
    let (server, db) = spawn_real_app().await;
    let (token, _user_id) = register_verified(&server, &db).await;
    let username = enable_bio(&server, &token, "").await;

    let mut ids = Vec::new();
    for (title, label, position) in [("A", "Alpha", 0), ("B", "Beta", 1), ("C", "Gamma", 2)] {
        let id = create_link(&server, &token, title).await;
        let res = put_link(
            &server,
            &token,
            id,
            json!({ "bio_visible": true, "bio_label": label, "bio_position": position }),
        )
        .await;
        assert_eq!(res.status_code(), 200, "show {title}: {}", res.text());
        ids.push(id);
    }
    let (a, b, c) = (ids[0], ids[1], ids[2]);

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let hidden = put_link(&server, &token, b, json!({ "bio_visible": false })).await;
    assert_eq!(hidden.status_code(), 200, "hide: {}", hidden.text());
    assert_eq!(hidden.json::<Value>()["bio_position"], Value::Null);
    assert!(stored_link(&db, b).await.bio_position.is_none());

    // Echoing visibility on a link that stays shown does not drop its place.
    let echo = put_link(
        &server,
        &token,
        a,
        json!({ "title": "A2", "bio_visible": true }),
    )
    .await;
    assert_eq!(echo.status_code(), 200, "echo: {}", echo.text());
    assert_eq!(echo.json::<Value>()["bio_position"], 0);
    assert_eq!(stored_link(&db, a).await.bio_position, Some(0));

    let ordered = server
        .put("/auth/bio/order")
        .authorization_bearer(&token)
        .json(&json!({ "link_ids": [c, a] }))
        .await;
    assert_eq!(ordered.status_code(), 200, "order: {}", ordered.text());

    // A place stored while the link is hidden is cleared on the way back.
    exec(
        &db,
        "UPDATE links SET bio_position = 1 WHERE id = $1",
        vec![b.into()],
    )
    .await;
    let shown = put_link(&server, &token, b, json!({ "bio_visible": true })).await;
    assert_eq!(shown.status_code(), 200, "show again: {}", shown.text());
    assert_eq!(shown.json::<Value>()["bio_position"], Value::Null);
    assert!(stored_link(&db, b).await.bio_position.is_none());

    let page = server.get(&format!("/api/bio/{username}")).await;
    assert_eq!(page.status_code(), 200, "page: {}", page.text());
    assert_eq!(
        public_labels(&page.json()),
        vec!["Gamma".to_string(), "Alpha".to_string(), "Beta".to_string()]
    );
}

/// The 501st personal bio link is refused. Hiding one frees a place, and an
/// organization link does not spend the personal cap.
#[tokio::test]
async fn showing_a_501st_personal_bio_link_is_refused() {
    let (server, db) = spawn_real_app().await;
    let (token, user_id) = register_verified(&server, &db).await;
    exec(
        &db,
        "INSERT INTO links (code, original_url, user_id, bio_visible, title) \
         SELECT $1 || g::text, 'https://example.com/bio-cap/' || g::text, $2, true, 'seed' \
         FROM generate_series(1, 500) AS g",
        vec![unique_code().into(), user_id.into()],
    )
    .await;

    let extra = create_link(&server, &token, "Extra").await;
    let refused = put_link(&server, &token, extra, json!({ "bio_visible": true })).await;
    assert_eq!(refused.status_code(), 400, "501st: {}", refused.text());
    assert!(refused.text().contains("at most 500"), "{}", refused.text());
    assert!(!stored_link(&db, extra).await.bio_visible);

    let seeded = links::Entity::find()
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::BioVisible.eq(true))
        .one(&db)
        .await
        .expect("db")
        .expect("seeded")
        .id;
    // Already shown, so saving it again is not a 501st link.
    let again = put_link(
        &server,
        &token,
        seeded,
        json!({ "bio_visible": true, "bio_label": "Still", "bio_position": 4 }),
    )
    .await;
    assert_eq!(again.status_code(), 200, "echo visible: {}", again.text());
    assert_eq!(again.json::<Value>()["bio_position"], 4);

    let org = server
        .post("/orgs")
        .authorization_bearer(&token)
        .json(&json!({
            "name": "Cap Org",
            "slug": format!("cap-{}", unique_code().to_lowercase()),
        }))
        .await;
    assert_eq!(org.status_code(), 201, "org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap();
    let org_link = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({
            "original_url": "https://iana.org/bio-cap-org",
            "title": "Org",
            "org_id": org_id,
        }))
        .await;
    assert_eq!(org_link.status_code(), 201, "org link: {}", org_link.text());
    let org_link_id = org_link.json::<Value>()["id"].as_i64().unwrap() as i32;
    let org_shown = put_link(&server, &token, org_link_id, json!({ "bio_visible": true })).await;
    assert_eq!(
        org_shown.status_code(),
        200,
        "org link does not spend the personal cap: {}",
        org_shown.text()
    );

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let hid = put_link(&server, &token, seeded, json!({ "bio_visible": false })).await;
    assert_eq!(hid.status_code(), 200, "hide seeded: {}", hid.text());
    assert_eq!(hid.json::<Value>()["bio_position"], Value::Null);

    let allowed = put_link(&server, &token, extra, json!({ "bio_visible": true })).await;
    assert_eq!(
        allowed.status_code(),
        200,
        "after a hide: {}",
        allowed.text()
    );
    assert_eq!(allowed.json::<Value>()["bio_position"], Value::Null);
    assert!(stored_link(&db, extra).await.bio_visible);
}

/// An organization editor may edit the link and may echo its bio fields, and
/// may not change the label or the place. Those belong to the creator's page.
#[tokio::test]
async fn an_org_editor_cannot_change_bio_label_or_position() {
    let (server, db) = spawn_real_app().await;
    let (owner_token, _owner_id) = register_verified(&server, &db).await;
    let editor_email = unique_email();
    let registered = server
        .post("/auth/register")
        .json(&json!({ "email": &editor_email, "password": "password123" }))
        .await;
    assert_eq!(
        registered.status_code(),
        201,
        "editor: {}",
        registered.text()
    );
    let editor_body: Value = registered.json();
    let editor_token = editor_body["token"].as_str().unwrap().to_string();
    let editor_id = editor_body["user_id"].as_i64().unwrap() as i32;
    mark_email_verified(&db, editor_id).await;

    let org = server
        .post("/orgs")
        .authorization_bearer(&owner_token)
        .json(&json!({
            "name": "Editor Org",
            "slug": format!("ed-{}", unique_code().to_lowercase()),
        }))
        .await;
    assert_eq!(org.status_code(), 201, "org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap();

    let created = server
        .post("/links")
        .authorization_bearer(&owner_token)
        .json(&json!({
            "original_url": "https://iana.org/bio-editor",
            "title": "Org link",
            "org_id": org_id,
        }))
        .await;
    assert_eq!(created.status_code(), 201, "link: {}", created.text());
    let link_id = created.json::<Value>()["id"].as_i64().unwrap() as i32;
    let placed = put_link(
        &server,
        &owner_token,
        link_id,
        json!({ "bio_visible": true, "bio_label": "Org", "bio_position": 1 }),
    )
    .await;
    assert_eq!(placed.status_code(), 200, "place: {}", placed.text());

    let invited = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner_token)
        .json(&json!({ "email": editor_email, "role": "editor" }))
        .await;
    assert_eq!(invited.status_code(), 201, "invite: {}", invited.text());

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    // The editor can still edit the link. That must not move the bio fields.
    let renamed = put_link(
        &server,
        &editor_token,
        link_id,
        json!({ "title": "Renamed by editor" }),
    )
    .await;
    assert_eq!(renamed.status_code(), 200, "title: {}", renamed.text());
    let renamed_body = renamed.json::<Value>();
    assert_eq!(renamed_body["title"].as_str(), Some("Renamed by editor"));
    assert_eq!(renamed_body["bio_label"].as_str(), Some("Org"));
    assert_eq!(renamed_body["bio_position"], 1);

    let new_label = put_link(
        &server,
        &editor_token,
        link_id,
        json!({ "bio_label": "Shop" }),
    )
    .await;
    assert_eq!(
        new_label.status_code(),
        403,
        "new label: {}",
        new_label.text()
    );
    assert!(
        new_label.text().contains("Only the link's creator"),
        "{}",
        new_label.text()
    );

    let same_label = put_link(
        &server,
        &editor_token,
        link_id,
        json!({ "bio_label": "Org" }),
    )
    .await;
    assert_eq!(
        same_label.status_code(),
        200,
        "same label: {}",
        same_label.text()
    );
    assert_eq!(
        same_label.json::<Value>()["bio_label"].as_str(),
        Some("Org")
    );

    let new_position = put_link(
        &server,
        &editor_token,
        link_id,
        json!({ "bio_position": 7 }),
    )
    .await;
    assert_eq!(
        new_position.status_code(),
        403,
        "new position: {}",
        new_position.text()
    );
    assert!(
        new_position.text().contains("Only the link's creator"),
        "{}",
        new_position.text()
    );

    let same_position = put_link(
        &server,
        &editor_token,
        link_id,
        json!({ "bio_position": 1 }),
    )
    .await;
    assert_eq!(
        same_position.status_code(),
        200,
        "same position: {}",
        same_position.text()
    );
    assert_eq!(same_position.json::<Value>()["bio_position"], 1);

    let row = stored_link(&db, link_id).await;
    assert_eq!(row.bio_label.as_deref(), Some("Org"));
    assert_eq!(row.bio_position, Some(1));
    assert_eq!(row.title.as_deref(), Some("Renamed by editor"));

    let owner_label = put_link(
        &server,
        &owner_token,
        link_id,
        json!({ "bio_label": "Kept" }),
    )
    .await;
    assert_eq!(
        owner_label.status_code(),
        200,
        "creator still can: {}",
        owner_label.text()
    );
    assert_eq!(
        owner_label.json::<Value>()["bio_label"].as_str(),
        Some("Kept")
    );
}
