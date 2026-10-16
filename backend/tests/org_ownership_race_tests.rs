//! Concurrent ownership changes must leave one owner, and a delete that already
//! passed its check must not purge an organization once ownership has moved.
//!
//! Both handlers used to authorize the caller and then write in a later
//! transaction without locking the organization row. Two transfers could each
//! promote their target, and a delete could commit after a transfer had made
//! someone else the owner. These tests hold `FOR UPDATE` on that row so both
//! requests pass the early check and queue on the row, then release it.
//! Transfer is queued first, and Postgres wakes lock waiters in that order, so
//! the overlap is the one that used to corrupt ownership rather than a flake.
//!
//! A `FOR UPDATE` holder does not show a tuple lock of its own. The first
//! writer often waits on that transaction (`wait_event = transactionid`) and
//! only a later writer shows an ungranted tuple lock, so the barrier counts
//! every session blocked on a statement that names this table. The two tests
//! must not overlap, or one test's waiter would satisfy the other's barrier.

mod common;

use std::future::IntoFuture;
use std::pin::pin;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Serializes the two tests. Their lock-waiter counts are database-wide, so
/// one test's blocked statement would satisfy the other's barrier. The guard
/// is `!Send`, so each test stays on the current-thread runtime.
static ONE_AT_A_TIME: Mutex<()> = Mutex::const_new(());

use axum_test::{TestResponse, TestServer};
use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::{org_members, organizations, users};
use opn_onl_backend::utils::jwt::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, Statement, TransactionTrait,
};
use serde_json::{Value, json};

struct Person {
    id: i32,
    email: String,
    token: String,
}

async fn person(db: &DatabaseConnection) -> Person {
    let user = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &user.email, user.token_version).expect("mint token");
    Person {
        id: user.id,
        email: user.email,
        token,
    }
}

async fn create_org(server: &TestServer, owner: &Person) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Race Org",
            "slug": format!("race-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

async fn invite(server: &TestServer, owner: &Person, org_id: i32, who: &Person) {
    let res = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "email": &who.email, "role": "editor" }))
        .await;
    assert_eq!(res.status_code(), 201, "invite: {}", res.text());
}

/// Sessions blocked on a statement that names `organizations`. The holder is
/// idle in its transaction (`ClientRead`), so it is not counted. `org_id` is
/// unused: the tests run one at a time, and every such waiter belongs to the
/// organization row that test holds.
async fn org_row_waiters(db: &DatabaseConnection, _org_id: i32) -> i64 {
    let row = db
        .query_one(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*)::bigint FROM pg_stat_activity \
             WHERE datname = current_database() \
               AND pid <> pg_backend_pid() \
               AND wait_event_type = 'Lock' \
               AND query LIKE '%organizations%'",
        ))
        .await
        .expect("count lock waiters")
        .expect("count row");
    row.try_get_by_index(0).expect("waiter count")
}

async fn wait_for_waiters(db: &DatabaseConnection, org_id: i32, want: i64) {
    let started = Instant::now();
    loop {
        let n = org_row_waiters(db, org_id).await;
        if n >= want {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "org {org_id}: wanted {want} row-lock waiters, last saw {n}\n{}",
            lock_debug(db).await
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

async fn owner_ids(db: &DatabaseConnection, org_id: i32) -> Vec<i32> {
    let mut ids: Vec<i32> = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::Role.eq("owner"))
        .all(db)
        .await
        .expect("owner rows")
        .into_iter()
        .map(|member| member.user_id)
        .collect();
    ids.sort_unstable();
    ids
}

/// `purge_organization` retires codes with a `SELECT` from `links` before it
/// touches memberships. Holding `links` there pauses a delete that has already
/// passed its owner check, so the transfer can commit before any member row is
/// locked. Without that pause the two transactions deadlock (transfer holds the
/// org row and wants the member row; delete holds the member row and wants the
/// org row) and the purge comes back 500 instead of deleting the new owner's org.
async fn hold_links_table(db: &DatabaseConnection) -> sea_orm::DatabaseTransaction {
    let hold = db.begin().await.expect("begin links hold");
    hold.execute_unprepared("LOCK TABLE links IN ACCESS EXCLUSIVE MODE")
        .await
        .expect("lock links");
    hold
}

async fn links_lock_waiters(db: &DatabaseConnection) -> i64 {
    let row = db
        .query_one(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*)::bigint FROM pg_locks \
             WHERE relation = 'links'::regclass AND NOT granted",
        ))
        .await
        .expect("count links waiters")
        .expect("count row");
    row.try_get_by_index(0).expect("waiter count")
}

/// Delete is parked once it is queued on the organization row (the fixed
/// handler locks that row before purging) or on `links` (today's purge retires
/// codes before it locks the org).
async fn wait_until_delete_parked(db: &DatabaseConnection, org_id: i32) {
    let started = Instant::now();
    loop {
        let on_org = org_row_waiters(db, org_id).await;
        let on_links = links_lock_waiters(db).await;
        if on_org >= 2 || on_links >= 1 {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "delete never parked (org waiters {on_org}, links waiters {on_links})\n{}",
            lock_debug(db).await
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

async fn hold_org_row(db: &DatabaseConnection, org_id: i32) -> sea_orm::DatabaseTransaction {
    let hold = db.begin().await.expect("begin hold");
    hold.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT id FROM organizations WHERE id = $1 FOR UPDATE",
        [org_id.into()],
    ))
    .await
    .expect("lock organization");
    hold
}

async fn lock_debug(db: &DatabaseConnection) -> String {
    let row = db
        .query_one(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COALESCE(string_agg( \
                pid::text || ' ' || COALESCE(wait_event_type, '') || '/' || \
                COALESCE(wait_event, '') || ' ' || left(query, 140), \
                E'\\n'), '') \
             FROM pg_stat_activity \
             WHERE datname = current_database() AND pid <> pg_backend_pid() \
               AND state <> 'idle'",
        ))
        .await
        .ok()
        .flatten();
    match row {
        Some(row) => row.try_get_by_index::<String>(0).unwrap_or_default(),
        None => String::new(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_ownership_transfers_leave_a_single_owner() {
    let _one = ONE_AT_A_TIME.lock().await;
    let (server, db) = setup_app().await;
    let owner = person(&db).await;
    let bee = person(&db).await;
    let cee = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &bee).await;
    invite(&server, &owner, org_id, &cee).await;

    let hold = hold_org_row(&db, org_id).await;
    let mut to_bee = pin!(
        server
            .post(&format!("/orgs/{org_id}/transfer-ownership"))
            .authorization_bearer(&owner.token)
            .json(&json!({ "new_owner_user_id": bee.id }))
            .into_future()
    );
    let mut to_cee = pin!(
        server
            .post(&format!("/orgs/{org_id}/transfer-ownership"))
            .authorization_bearer(&owner.token)
            .json(&json!({ "new_owner_user_id": cee.id }))
            .into_future()
    );
    tokio::select! {
        res = &mut to_bee => panic!("transfer to B finished before it blocked: {} {}", res.status_code(), res.text()),
        res = &mut to_cee => panic!("transfer to C finished before it blocked: {} {}", res.status_code(), res.text()),
        _ = wait_for_waiters(&db, org_id, 2) => {}
    }
    hold.rollback().await.expect("release organization lock");
    let (to_bee, to_cee) = tokio::join!(to_bee, to_cee);

    let winners: Vec<&TestResponse> = [&to_bee, &to_cee]
        .into_iter()
        .filter(|res| res.status_code().as_u16() == 200)
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "one transfer commits and the other is refused\nB: {} {}\nC: {} {}",
        to_bee.status_code(),
        to_bee.text(),
        to_cee.status_code(),
        to_cee.text()
    );
    let loser = [&to_bee, &to_cee]
        .into_iter()
        .find(|res| res.status_code().as_u16() != 200)
        .expect("loser");
    assert_eq!(
        loser.status_code(),
        403,
        "the transfer that lost the lock is refused: {}",
        loser.text()
    );

    let owners = owner_ids(&db, org_id).await;
    assert_eq!(owners.len(), 1, "owner rows: {owners:?}");
    let org = organizations::Entity::find_by_id(org_id)
        .one(&db)
        .await
        .expect("db")
        .expect("org remains");
    assert_eq!(org.owner_id, owners[0], "owner_id matches the owner row");
    let winner_id = winners[0].json::<Value>()["owner_id"].as_i64().unwrap() as i32;
    assert_eq!(winner_id, owners[0]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_transfer_overlapping_delete_does_not_purge_the_new_owners_org() {
    let _one = ONE_AT_A_TIME.lock().await;
    let (server, db) = setup_app().await;
    let owner = person(&db).await;
    let bee = person(&db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &bee).await;

    let hold_org = hold_org_row(&db, org_id).await;
    let hold_links = hold_links_table(&db).await;
    let mut transfer = pin!(
        server
            .post(&format!("/orgs/{org_id}/transfer-ownership"))
            .authorization_bearer(&owner.token)
            .json(&json!({ "new_owner_user_id": bee.id }))
            .into_future()
    );
    tokio::select! {
        res = &mut transfer => panic!("transfer finished before it blocked: {} {}", res.status_code(), res.text()),
        _ = wait_for_waiters(&db, org_id, 1) => {}
    }
    let mut delete = pin!(
        server
            .delete(&format!("/orgs/{org_id}"))
            .authorization_bearer(&owner.token)
            .into_future()
    );
    tokio::select! {
        res = &mut transfer => panic!("transfer finished while delete was starting: {} {}", res.status_code(), res.text()),
        res = &mut delete => panic!("delete finished before it blocked: {} {}", res.status_code(), res.text()),
        _ = wait_until_delete_parked(&db, org_id) => {}
    }
    // The transfer's response still counts links, so it stays blocked on
    // `hold_links` after its commit. The request future has to be polled or
    // that commit never happens. Release the org row, wait until the commit
    // is visible, then release `links`.
    hold_org
        .rollback()
        .await
        .expect("release organization lock");
    let started = Instant::now();
    let mut delete_done: Option<TestResponse> = None;
    loop {
        if delete_done.is_none() {
            tokio::select! {
                res = &mut transfer => panic!(
                    "transfer finished while links were still locked: {} {}",
                    res.status_code(),
                    res.text()
                ),
                res = &mut delete => delete_done = Some(res),
                _ = tokio::time::sleep(Duration::from_millis(15)) => {}
            }
        } else {
            tokio::select! {
                res = &mut transfer => panic!(
                    "transfer finished while links were still locked: {} {}",
                    res.status_code(),
                    res.text()
                ),
                _ = tokio::time::sleep(Duration::from_millis(15)) => {}
            }
        }
        let owner_now = organizations::Entity::find_by_id(org_id)
            .one(&db)
            .await
            .expect("db")
            .map(|org| org.owner_id);
        if owner_now == Some(bee.id) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "transfer never committed (owner_id={owner_now:?})\n{}",
            lock_debug(&db).await
        );
    }
    hold_links.rollback().await.expect("release links lock");
    let transfer = transfer.await;
    let delete = match delete_done {
        Some(done) => done,
        None => delete.await,
    };

    assert_eq!(
        transfer.status_code(),
        200,
        "the queued transfer commits: {}",
        transfer.text()
    );
    assert_eq!(
        delete.status_code(),
        403,
        "the old owner must not purge the org after losing it: {}",
        delete.text()
    );
    let org = organizations::Entity::find_by_id(org_id)
        .one(&db)
        .await
        .expect("db")
        .expect("org remains with the new owner");
    assert_eq!(org.owner_id, bee.id);
    assert_eq!(owner_ids(&db, org_id).await, vec![bee.id]);
}

async fn setup_app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}
