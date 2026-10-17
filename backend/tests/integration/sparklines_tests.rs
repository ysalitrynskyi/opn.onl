//! `GET /links/sparklines`: clicks per UTC calendar day for the last seven days
//! (today included), counted by the database. Real router, real Postgres.
//! The memory side of this lives in `sparklines_memory_tests.rs`, which needs
//! its own test binary.

use crate::common;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use common::{spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{links, org_members, organizations, users};
use opn_onl_backend::utils::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseBackend, DatabaseConnection,
    Statement,
};
use serde_json::Value;

// ---------------------------------------------------------------- fixtures

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

async fn insert_link(db: &DatabaseConnection, creator: i32, org_id: Option<i32>) -> i32 {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set("https://iana.org/original".to_string()),
        user_id: Set(Some(creator)),
        org_id: Set(org_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
    .id
}

async fn insert_org(db: &DatabaseConnection, owner: i32, members: &[(i32, &str)]) -> i32 {
    let org = organizations::ActiveModel {
        name: Set("Sparkline Org".to_string()),
        slug: Set(format!("spark-{}", uuid::Uuid::new_v4().simple())),
        owner_id: Set(owner),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert org");
    for (user_id, role) in members {
        org_members::ActiveModel {
            org_id: Set(org.id),
            user_id: Set(*user_id),
            role: Set(role.to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("add member");
    }
    org.id
}

async fn insert_click(db: &DatabaseConnection, link_id: i32, created_at: NaiveDateTime) {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO click_events (link_id, created_at) VALUES ($1, $2)",
        [link_id.into(), created_at.into()],
    ))
    .await
    .expect("insert click");
}

/// `days_ago` UTC calendar days before `today`, at `h:m:s`.
fn at(today: NaiveDate, days_ago: i64, h: u32, m: u32, s: u32) -> NaiveDateTime {
    (today - Duration::days(days_ago)).and_time(NaiveTime::from_hms_opt(h, m, s).unwrap())
}

async fn sparklines(server: &axum_test::TestServer, token: &str, ids: &str) -> Value {
    let res = server
        .get("/links/sparklines")
        .add_query_param("ids", ids)
        .authorization_bearer(token)
        .await;
    assert_eq!(res.status_code(), 200, "sparklines: {}", res.text());
    res.json()
}

/// The entry for `link_id`, if the response has one.
fn entry(body: &Value, link_id: i32) -> Option<&Value> {
    body["sparklines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["link_id"].as_i64() == Some(link_id as i64))
}

fn series(entry: &Value) -> Vec<i64> {
    entry["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect()
}

fn link_ids(body: &Value) -> Vec<i64> {
    body["sparklines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["link_id"].as_i64().unwrap())
        .collect()
}

// ------------------------------------------------------------------- tests

#[tokio::test]
async fn clicks_are_bucketed_by_utc_calendar_day() {
    let (server, db) = spawn_real_app().await;
    let owner = person(&db).await;

    // The scenario is anchored on today's UTC date, so a run that straddles
    // midnight UTC is thrown away and repeated rather than reported as a bug.
    for _ in 0..3 {
        let today = Utc::now().date_naive();
        let link = insert_link(&db, owner.id, None).await;
        let other = insert_link(&db, owner.id, None).await;

        // Oldest bucket is six days back; its first instant is the window start.
        insert_click(&db, link, at(today, 6, 0, 0, 0)).await;
        // One second before the window: seven days back, last second of the day.
        insert_click(&db, link, at(today, 7, 23, 59, 59)).await;
        insert_click(&db, link, at(today, 8, 12, 0, 0)).await;
        // Two on one afternoon.
        insert_click(&db, link, at(today, 3, 12, 0, 0)).await;
        insert_click(&db, link, at(today, 3, 12, 0, 1)).await;
        // One second before midnight UTC belongs to the day that is ending,
        // however recent it still is.
        insert_click(&db, link, at(today, 1, 23, 59, 59)).await;
        // The first instant of today.
        insert_click(&db, link, at(today, 0, 0, 0, 0)).await;
        // A different link must not bleed into this one.
        insert_click(&db, other, at(today, 2, 9, 0, 0)).await;

        let body = sparklines(&server, &owner.token, &format!("{link},{other}")).await;
        if Utc::now().date_naive() != today {
            continue;
        }

        let mine = entry(&body, link).expect("owned link is returned");
        assert_eq!(series(mine), vec![1, 0, 0, 2, 0, 1, 1]);
        assert_eq!(mine["total"], 5, "total counts the window only");
        let labels: Vec<String> = (0..7)
            .rev()
            .map(|days_ago| {
                (today - Duration::days(days_ago))
                    .format("%m/%d")
                    .to_string()
            })
            .collect();
        assert_eq!(
            mine["labels"],
            serde_json::json!(labels),
            "labels are the UTC dates of the buckets, oldest first"
        );

        let theirs = entry(&body, other).expect("second link is returned");
        assert_eq!(series(theirs), vec![0, 0, 0, 0, 1, 0, 0]);
        assert_eq!(theirs["total"], 1);
        return;
    }
    panic!("the UTC day rolled over on every attempt");
}

#[tokio::test]
async fn link_without_clicks_gets_a_zero_filled_series() {
    let (server, db) = spawn_real_app().await;
    let owner = person(&db).await;
    let quiet = insert_link(&db, owner.id, None).await;

    let body = sparklines(&server, &owner.token, &quiet.to_string()).await;

    let quiet = entry(&body, quiet).expect("a link with no clicks is still returned");
    assert_eq!(series(quiet), vec![0; 7]);
    assert_eq!(quiet["total"], 0);
    assert_eq!(quiet["labels"].as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn links_the_caller_cannot_read_are_omitted() {
    let (server, db) = spawn_real_app().await;
    let me = person(&db).await;
    let stranger = person(&db).await;
    let colleague = person(&db).await;
    let today = Utc::now().date_naive();
    let click_at = at(today, 0, 0, 0, 0);

    let mine = insert_link(&db, me.id, None).await;
    let theirs = insert_link(&db, stranger.id, None).await;
    let deleted = insert_link(&db, me.id, None).await;
    let closed_org_link = {
        let org = insert_org(&db, stranger.id, &[(stranger.id, "owner")]).await;
        insert_link(&db, stranger.id, Some(org)).await
    };
    // `me` belongs to this organization, so its links are readable even though
    // a colleague created them.
    let team_link = {
        let org = insert_org(
            &db,
            colleague.id,
            &[(colleague.id, "owner"), (me.id, "viewer")],
        )
        .await;
        insert_link(&db, colleague.id, Some(org)).await
    };
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE links SET deleted_at = now() WHERE id = $1",
        [deleted.into()],
    ))
    .await
    .expect("soft delete");
    let missing = 2_000_000_000;
    for link in [mine, theirs, deleted, closed_org_link, team_link] {
        insert_click(&db, link, click_at).await;
    }

    let ids = [mine, theirs, deleted, closed_org_link, team_link, missing]
        .map(|id| id.to_string())
        .join(",");
    let body = sparklines(&server, &me.token, &ids).await;

    assert_eq!(
        link_ids(&body),
        vec![mine as i64, team_link as i64],
        "own link and member organization link only"
    );
    assert_eq!(entry(&body, mine).unwrap()["total"], 1);
    assert_eq!(
        entry(&body, team_link).unwrap()["total"],
        1,
        "a member sees an organization link's clicks whoever created it"
    );
}

#[tokio::test]
async fn answers_follow_the_request_order_without_repeats() {
    let (server, db) = spawn_real_app().await;
    let owner = person(&db).await;
    let first = insert_link(&db, owner.id, None).await;
    let second = insert_link(&db, owner.id, None).await;

    let body = sparklines(
        &server,
        &owner.token,
        &format!("{second}, {first},{second},nonsense,"),
    )
    .await;

    assert_eq!(link_ids(&body), vec![second as i64, first as i64]);
}

#[tokio::test]
async fn id_list_is_capped_and_authentication_is_required() {
    let (server, db) = spawn_real_app().await;
    let owner = person(&db).await;
    let link = insert_link(&db, owner.id, None).await;

    let ids = |n: i32| (1..=n).map(|i| i.to_string()).collect::<Vec<_>>().join(",");

    let res = server
        .get("/links/sparklines")
        .add_query_param("ids", ids(501))
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(res.status_code(), 400, "501 ids: {}", res.text());
    assert!(res.json::<Value>()["error"].is_string());

    // At the cap the request is served (and the ids that do not exist are
    // simply absent).
    let body = sparklines(&server, &owner.token, &format!("{link},{}", ids(499))).await;
    assert!(entry(&body, link).is_some());

    let res = server
        .get("/links/sparklines")
        .add_query_param("ids", link.to_string())
        .await;
    assert_eq!(res.status_code(), 401);

    let body = sparklines(&server, &owner.token, "").await;
    assert!(link_ids(&body).is_empty());
}
