//! A link deleted on its own keeps its destination, title and notes for as
//! long as the account lives, unless the sweep removes it. Once a link has been
//! deleted for longer than the purge window (the same one deleted accounts get)
//! the daily sweep removes the row, retires its code so the code stays
//! reserved, and lets the click history, tags and routing rules go with it
//! (`ON DELETE CASCADE`).
//!
//! The sweep removes every old deleted link in the database, so these tests
//! use a window no other test's links reach (`WINDOW_DAYS`) and take turns.

use crate::common;

use chrono::{Duration, NaiveDateTime, Utc};
use common::{spawn_real_app, unique_email};
use opn_onl_backend::entity::{
    click_events, link_tags, links, retired_codes, routing_rules, tags, users,
};
use opn_onl_backend::utils::account_purge::purge_expired_deleted_links;
use opn_onl_backend::utils::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};

/// Older than anything another test deletes and then ages.
const WINDOW_DAYS: i32 = 1234;

/// One test at a time: a sweep also takes the rows another test just aged.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn days_ago(days: i64) -> NaiveDateTime {
    Utc::now().naive_utc() - Duration::days(days)
}

async fn account(db: &DatabaseConnection) -> (i32, String) {
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
    (user.id, token)
}

fn code() -> String {
    format!("dl{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

async fn link(
    db: &DatabaseConnection,
    owner: i32,
    deleted_at: Option<NaiveDateTime>,
) -> links::Model {
    links::ActiveModel {
        code: Set(code()),
        original_url: Set("https://iana.org/private-destination".to_string()),
        title: Set(Some("Private title".to_string())),
        notes: Set(Some("Private notes".to_string())),
        user_id: Set(Some(owner)),
        deleted_at: Set(deleted_at),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert link")
}

async fn click(db: &DatabaseConnection, link_id: i32) {
    click_events::ActiveModel {
        link_id: Set(link_id),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert click");
}

async fn tag(db: &DatabaseConnection, owner: i32, link_id: i32) {
    let tag = tags::ActiveModel {
        name: Set(format!("tag-{}", code())),
        user_id: Set(Some(owner)),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert tag");
    link_tags::ActiveModel {
        link_id: Set(link_id),
        tag_id: Set(tag.id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("tag the link");
}

async fn rule(db: &DatabaseConnection, link_id: i32) {
    routing_rules::ActiveModel {
        link_id: Set(link_id),
        priority: Set(1),
        destination_url: Set("https://iana.org/rule".to_string()),
        weight: Set(100),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert rule");
}

async fn link_rows(db: &DatabaseConnection, ids: &[i32]) -> u64 {
    links::Entity::find()
        .filter(links::Column::Id.is_in(ids.to_vec()))
        .count(db)
        .await
        .unwrap()
}

async fn retired(db: &DatabaseConnection, codes: &[String]) -> u64 {
    retired_codes::Entity::find()
        .filter(retired_codes::Column::Code.is_in(codes.to_vec()))
        .count(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn an_old_deleted_link_is_removed_with_its_history_and_its_code_stays_reserved() {
    let _serial = crate::common::serial_guard().await;
    let _turn = TURN.lock().await;
    let (server, db) = spawn_real_app().await;
    let (owner, token) = account(&db).await;
    let old = link(&db, owner, Some(days_ago(i64::from(WINDOW_DAYS) + 5))).await;
    let recent = link(&db, owner, Some(days_ago(i64::from(WINDOW_DAYS) - 5))).await;
    let live = link(&db, owner, None).await;
    for link in [&old, &recent, &live] {
        click(&db, link.id).await;
        tag(&db, owner, link.id).await;
        rule(&db, link.id).await;
    }

    let removed = purge_expired_deleted_links(&db, WINDOW_DAYS)
        .await
        .expect("sweep");
    assert!(removed >= 1, "the old link counts: {removed}");

    assert_eq!(link_rows(&db, &[old.id]).await, 0, "the old link is gone");
    for (table, left) in [
        (
            "click_events",
            click_events::Entity::find()
                .filter(click_events::Column::LinkId.eq(old.id))
                .count(&db)
                .await
                .unwrap(),
        ),
        (
            "link_tags",
            link_tags::Entity::find()
                .filter(link_tags::Column::LinkId.eq(old.id))
                .count(&db)
                .await
                .unwrap(),
        ),
        (
            "routing_rules",
            routing_rules::Entity::find()
                .filter(routing_rules::Column::LinkId.eq(old.id))
                .count(&db)
                .await
                .unwrap(),
        ),
    ] {
        assert_eq!(left, 0, "{table} went with the link");
    }

    // Its code is reserved for good: nobody can claim it and inherit the
    // traffic of a printed QR code.
    assert_eq!(retired(&db, std::slice::from_ref(&old.code)).await, 1);
    let res = server
        .get("/links/check-code")
        .add_query_param("code", &old.code)
        .await;
    assert_eq!(res.json::<Value>()["available"], false, "{}", res.text());
    let res = server
        .post("/links")
        .authorization_bearer(&token)
        .json(&json!({ "original_url": "https://iana.org/claim", "custom_alias": old.code }))
        .await;
    assert_eq!(res.status_code(), 409, "claim: {}", res.text());

    // Controls: inside the window, and not deleted at all, nothing changes.
    assert_eq!(link_rows(&db, &[recent.id, live.id]).await, 2);
    for kept in [&recent, &live] {
        assert_eq!(
            click_events::Entity::find()
                .filter(click_events::Column::LinkId.eq(kept.id))
                .count(&db)
                .await
                .unwrap(),
            1,
            "history of a link that stays"
        );
    }
    assert_eq!(
        retired(&db, &[recent.code.clone(), live.code.clone()]).await,
        0
    );
}

/// More links than one batch: every one goes, with its code retired.
#[tokio::test]
async fn a_backlog_larger_than_one_batch_is_cleared() {
    let _serial = crate::common::serial_guard().await;
    let _turn = TURN.lock().await;
    let (_, db) = spawn_real_app().await;
    let (owner, _) = account(&db).await;
    let stamp = days_ago(i64::from(WINDOW_DAYS) + 30);

    let mut ids = vec![];
    let mut codes = vec![];
    for _ in 0..230 {
        let old = link(&db, owner, Some(stamp)).await;
        ids.push(old.id);
        codes.push(old.code);
    }

    let removed = purge_expired_deleted_links(&db, WINDOW_DAYS)
        .await
        .expect("sweep");

    assert!(removed >= 230, "{removed}");
    assert_eq!(link_rows(&db, &ids).await, 0);
    assert_eq!(retired(&db, &codes).await, 230);
}

/// A link that is still live is never removed, however old it is.
#[tokio::test]
async fn an_old_link_that_was_never_deleted_stays() {
    let _serial = crate::common::serial_guard().await;
    let _turn = TURN.lock().await;
    let (_, db) = spawn_real_app().await;
    let (owner, _) = account(&db).await;
    let mut old: links::ActiveModel = link(&db, owner, None).await.into();
    old.created_at = Set(days_ago(5 * 365));
    let old = old.update(&db).await.expect("age link");

    purge_expired_deleted_links(&db, WINDOW_DAYS)
        .await
        .expect("sweep");

    assert_eq!(link_rows(&db, &[old.id]).await, 1);
}
