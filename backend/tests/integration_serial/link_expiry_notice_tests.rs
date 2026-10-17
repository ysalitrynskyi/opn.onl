//! Reminders before a link expires.
//!
//! The daily job (`utils::link_expiry::send_link_expiry_reminders`) is called
//! directly with a real `EmailService` pointed at the local SMTP stub
//! (`tests/common/smtp_stub.rs`), so the tests see the mail that was really
//! handed to SMTP and the stamps the job left in Postgres. The API tests run
//! the real router.
//!
//! A run reminds every due owner in the database, including ones other tests
//! made, so the tests run alone (`integration_serial`) and every assertion
//! is about the test's own owners and links.

use crate::common;
use crate::common::smtp_stub;

use std::sync::Arc;

use axum_test::TestServer;
use chrono::{Duration, NaiveDateTime, SubsecRound, Utc};
use common::{spawn_real_app, unique_code, unique_email};
use opn_onl_backend::entity::{blocked_domains, links, organizations, users};
use opn_onl_backend::utils::create_jwt;
use opn_onl_backend::utils::email::EmailService;
use opn_onl_backend::utils::link_expiry::{MAX_LINKS_PER_NOTICE, send_link_expiry_reminders};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use smtp_stub::smtp_stub;

/// One reminder run at a time: a run in one test would otherwise mail and
/// stamp the links another test is about to check.
static JOB: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Plenty of hourly budget: the database holds due links of other tests too,
/// and a run must reach this test's owners however many there are.
const ROOMY: u64 = 1_000_000;

// ---------------------------------------------------------------- fixtures

async fn db() -> DatabaseConnection {
    // Starts the SMTP server and points SMTP_* (and FRONTEND_URL) at it
    // before any EmailService is built.
    smtp_stub();
    common::setup_test_db().await
}

fn mailer(hourly_limit: u64) -> EmailService {
    smtp_stub();
    let service = EmailService::new().with_hourly_limit(hourly_limit);
    assert!(service.is_configured(), "the SMTP stub is not configured");
    service
}

async fn owner(db: &DatabaseConnection, verified: bool) -> users::Model {
    users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(verified),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

async fn update_owner(
    db: &DatabaseConnection,
    user: &users::Model,
    change: impl FnOnce(&mut users::ActiveModel),
) {
    let mut active: users::ActiveModel = user.clone().into();
    change(&mut active);
    active.update(db).await.expect("update user");
}

/// Whole seconds from now: Postgres keeps microseconds, and tests compare.
fn from_now(offset: Duration) -> NaiveDateTime {
    (Utc::now() + offset).trunc_subsecs(0).naive_utc()
}

/// A live personal link of `user_id` that expires after `expires_in`.
fn due_in(user_id: i32, expires_in: Duration) -> links::ActiveModel {
    links::ActiveModel {
        code: Set(unique_code()),
        original_url: Set(format!(
            "https://iana.org/lexp-{}",
            uuid::Uuid::new_v4().simple()
        )),
        user_id: Set(Some(user_id)),
        title: Set(Some(format!("Title {}", uuid::Uuid::new_v4().simple()))),
        expires_at: Set(Some(from_now(expires_in))),
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
        .expect("db")
        .expect("link row")
}

async fn stamp(db: &DatabaseConnection, id: i32) -> Option<NaiveDateTime> {
    row(db, id).await.expiry_notice_sent_at
}

/// Links stamped at or after `since`, whoever owns them. Only a reminder run
/// stamps a link with the current time, and runs take turns (`JOB`).
async fn stamped_since(db: &DatabaseConnection, since: NaiveDateTime) -> u64 {
    links::Entity::find()
        .filter(links::Column::ExpiryNoticeSentAt.gte(since))
        .count(db)
        .await
        .expect("count stamps")
}

async fn set_stamp(db: &DatabaseConnection, id: i32, value: Option<NaiveDateTime>) {
    let mut active: links::ActiveModel = row(db, id).await.into();
    active.expiry_notice_sent_at = Set(value);
    active.update(db).await.expect("set stamp");
}

/// The reminders delivered to `address`, decoded.
fn reminders_to(address: &str) -> Vec<String> {
    smtp_stub()
        .to(address)
        .iter()
        .filter(|mail| mail.subject().contains("expire"))
        .map(|mail| mail.decoded())
        .collect()
}

fn short_url(link: &links::Model) -> String {
    format!("https://app.test/{}", link.code)
}

// ------------------------------------------------------------ the selection

#[tokio::test]
async fn links_due_within_three_days_are_listed_in_one_mail_and_stamped() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    let user = owner(&db, true).await;
    let tomorrow = insert(&db, due_in(user.id, Duration::days(1))).await;
    let edge = insert(
        &db,
        due_in(user.id, Duration::days(3) - Duration::minutes(5)),
    )
    .await;
    let later = insert(&db, due_in(user.id, Duration::days(4))).await;
    let expired = insert(&db, due_in(user.id, -Duration::hours(1))).await;
    let mut forever = due_in(user.id, Duration::days(1));
    forever.expires_at = Set(None);
    let forever = insert(&db, forever).await;

    let report = send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run");
    assert!(!report.pool_spent);

    let mails = reminders_to(&user.email);
    assert_eq!(mails.len(), 1, "one mail for all the owner's due links");
    let mail = &mails[0];
    for listed in [&tomorrow, &edge] {
        assert!(mail.contains(&short_url(listed)), "{mail}");
        assert!(mail.contains(listed.title.as_deref().unwrap()), "{mail}");
        assert!(stamp(&db, listed.id).await.is_some(), "listed is stamped");
    }
    let expected = tomorrow
        .expires_at
        .unwrap()
        .format("Expires %Y-%m-%d %H:%M UTC");
    assert!(mail.contains(&expected.to_string()), "{mail}");
    for left_out in [&later, &expired, &forever] {
        assert!(!mail.contains(&short_url(left_out)), "{mail}");
        assert_eq!(stamp(&db, left_out.id).await, None, "unlisted is unstamped");
    }

    // The stamps keep the next run from mailing about the same dates.
    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("second run");
    assert_eq!(reminders_to(&user.email).len(), 1, "no second reminder");
}

#[tokio::test]
async fn org_deleted_blocked_used_up_unstarted_and_reminded_links_are_left_out() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    let user = owner(&db, true).await;
    let day = Duration::days(1);
    let control = insert(&db, due_in(user.id, day)).await;

    let org = organizations::ActiveModel {
        name: Set("Expiry Org".to_string()),
        slug: Set(format!("lexp-{}", uuid::Uuid::new_v4().simple())),
        owner_id: Set(user.id),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert org");
    let mut team = due_in(user.id, day);
    team.org_id = Set(Some(org.id));
    let team = insert(&db, team).await;

    let mut deleted = due_in(user.id, day);
    deleted.deleted_at = Set(Some(Utc::now().naive_utc()));
    let deleted = insert(&db, deleted).await;

    // A host of its own: the block must not reach other tests' links.
    let host = format!("lexp-{}.iana.org", uuid::Uuid::new_v4().simple());
    blocked_domains::ActiveModel {
        domain: Set(host.clone()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("block domain");
    let mut blocked = due_in(user.id, day);
    blocked.original_url = Set(format!("https://{host}/page"));
    let blocked = insert(&db, blocked).await;

    let mut used_up = due_in(user.id, day);
    used_up.max_clicks = Set(Some(3));
    used_up.click_count = Set(3);
    let used_up = insert(&db, used_up).await;

    let mut burned = due_in(user.id, day);
    burned.burn_after_reading = Set(true);
    burned.max_clicks = Set(Some(1));
    burned.click_count = Set(1);
    burned.burned_at = Set(Some(Utc::now().naive_utc()));
    let burned = insert(&db, burned).await;

    let mut unstarted = due_in(user.id, Duration::days(2));
    unstarted.starts_at = Set(Some(from_now(Duration::hours(6))));
    let unstarted = insert(&db, unstarted).await;

    let reminded_at = from_now(-Duration::days(1));
    let mut reminded = due_in(user.id, day);
    reminded.expiry_notice_sent_at = Set(Some(reminded_at));
    let reminded = insert(&db, reminded).await;

    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run");

    let mails = reminders_to(&user.email);
    assert_eq!(mails.len(), 1, "the control link is reminded");
    let mail = &mails[0];
    assert!(mail.contains(&short_url(&control)), "{mail}");
    assert!(stamp(&db, control.id).await.is_some());
    for left_out in [&team, &deleted, &blocked, &used_up, &burned, &unstarted] {
        assert!(!mail.contains(&short_url(left_out)), "{mail}");
        assert_eq!(stamp(&db, left_out.id).await, None, "{}", left_out.code);
    }
    assert!(!mail.contains(&short_url(&reminded)), "{mail}");
    assert_eq!(
        stamp(&db, reminded.id).await,
        Some(reminded_at),
        "an earlier stamp is kept"
    );
}

#[tokio::test]
async fn owners_who_opted_out_or_may_not_be_mailed_get_nothing() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    let day = Duration::days(1);

    let opted_out = owner(&db, true).await;
    update_owner(&db, &opted_out, |u| u.notify_link_expiry = Set(false)).await;
    let deleted = owner(&db, true).await;
    update_owner(&db, &deleted, |u| {
        u.deleted_at = Set(Some(Utc::now().naive_utc()))
    })
    .await;
    let disabled = owner(&db, true).await;
    update_owner(&db, &disabled, |u| {
        u.disabled_at = Set(Some(Utc::now().naive_utc()))
    })
    .await;
    let unverified = owner(&db, false).await;
    let control = owner(&db, true).await;

    let mut due = Vec::new();
    for user in [&opted_out, &deleted, &disabled, &unverified, &control] {
        due.push(insert(&db, due_in(user.id, day)).await);
    }

    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run");

    for (user, link) in [&opted_out, &deleted, &disabled, &unverified]
        .into_iter()
        .zip(&due)
    {
        assert!(reminders_to(&user.email).is_empty(), "{}", user.email);
        assert_eq!(stamp(&db, link.id).await, None, "{}", user.email);
    }
    assert_eq!(reminders_to(&control.email).len(), 1, "control is mailed");
    assert!(stamp(&db, due[4].id).await.is_some());

    // On an instance that does not require verification, the unverified
    // owner is mailed; the others still are not.
    send_link_expiry_reminders(&db, &mailer(ROOMY), false)
        .await
        .expect("run without the verification requirement");
    assert_eq!(reminders_to(&unverified.email).len(), 1);
    assert!(stamp(&db, due[3].id).await.is_some());
    for user in [&opted_out, &deleted, &disabled] {
        assert!(reminders_to(&user.email).is_empty(), "{}", user.email);
    }

    // Turning reminders back on picks up a link that is still due.
    update_owner(&db, &opted_out, |u| u.notify_link_expiry = Set(true)).await;
    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run after opting back in");
    assert_eq!(reminders_to(&opted_out.email).len(), 1);
    assert!(stamp(&db, due[0].id).await.is_some());
}

#[tokio::test]
async fn one_mail_lists_the_twenty_soonest_and_the_rest_wait_for_the_next_run() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    let user = owner(&db, true).await;
    let count = MAX_LINKS_PER_NOTICE + 2;
    let mut due = Vec::new();
    // Inserted latest first, so listing by id would get the order wrong.
    for minutes in (0..count as i64).rev() {
        due.push(
            insert(
                &db,
                due_in(user.id, Duration::hours(2) + Duration::minutes(minutes)),
            )
            .await,
        );
    }
    due.reverse();

    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run");
    let mails = reminders_to(&user.email);
    assert_eq!(mails.len(), 1);
    assert!(
        mails[0].contains("More of your links expire"),
        "{}",
        mails[0]
    );
    let (listed, waiting) = due.split_at(MAX_LINKS_PER_NOTICE);
    for link in listed {
        assert!(mails[0].contains(&short_url(link)), "{}", link.code);
        assert!(stamp(&db, link.id).await.is_some(), "{}", link.code);
    }
    for link in waiting {
        assert!(!mails[0].contains(&short_url(link)), "{}", link.code);
        assert_eq!(stamp(&db, link.id).await, None, "{}", link.code);
    }

    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("next run");
    let mails = reminders_to(&user.email);
    assert_eq!(mails.len(), 2, "the next run lists the rest");
    assert!(!mails[1].contains("More of your links"), "{}", mails[1]);
    for link in waiting {
        assert!(mails[1].contains(&short_url(link)), "{}", link.code);
        assert!(stamp(&db, link.id).await.is_some(), "{}", link.code);
    }
}

// ------------------------------------------------------ mail that did not go

#[tokio::test]
async fn a_spent_budget_leaves_the_links_for_the_next_run() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    let user = owner(&db, true).await;
    let link = insert(&db, due_in(user.id, Duration::days(1))).await;

    let started = Utc::now().naive_utc();
    let report = send_link_expiry_reminders(&db, &mailer(0), true)
        .await
        .expect("run with no budget");
    assert!(report.pool_spent);
    assert!(reminders_to(&user.email).is_empty());
    assert_eq!(stamp(&db, link.id).await, None, "no mail, no stamp");
    // The run stops at the first owner it cannot mail, which may be another
    // test's: that owner's links must not be stamped either.
    assert_eq!(
        stamped_since(&db, started).await,
        0,
        "the run stamped nothing"
    );

    // Control: with budget the same link is reminded and stamped.
    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run with budget");
    assert_eq!(reminders_to(&user.email).len(), 1);
    assert!(stamp(&db, link.id).await.is_some());
}

#[tokio::test]
async fn a_failed_send_leaves_the_links_for_the_next_run() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    let user = owner(&db, true).await;
    let link = insert(&db, due_in(user.id, Duration::days(1))).await;

    // Reports itself configured; every send is refused.
    let broken = EmailService::configured_for_tests().with_hourly_limit(ROOMY);
    let report = send_link_expiry_reminders(&db, &broken, true)
        .await
        .expect("run against a dead server");
    assert!(report.failed >= 1, "{report:?}");
    assert_eq!(stamp(&db, link.id).await, None, "no mail, no stamp");

    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("run against the stub");
    assert_eq!(reminders_to(&user.email).len(), 1);
    assert!(stamp(&db, link.id).await.is_some());
}

/// The owner moves the expiry while the reminder about the old date is on its
/// way. The stamp would hide the new date, so it is not set.
#[tokio::test]
async fn a_link_whose_expiry_changed_while_mailing_is_not_stamped() {
    let _serial = crate::common::serial_guard().await;
    let _turn = JOB.lock().await;
    let db = db().await;
    // Reminded first (lower id), so once its link is stamped the run has
    // moved on to `user`.
    let before = owner(&db, true).await;
    let before_link = insert(&db, due_in(before.id, Duration::days(1))).await;
    let user = owner(&db, true).await;
    let link = insert(&db, due_in(user.id, Duration::days(1))).await;
    let old_expiry = link.expires_at.unwrap();

    // Released however the test ends: a held mail would stall every later
    // run for lettre's minute-long timeout.
    struct Release(smtp_stub::MailHold);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release();
        }
    }
    let hold = Release(smtp_stub().hold_mail_for(&user.email));
    let run = {
        let db = db.clone();
        let service = Arc::new(mailer(ROOMY));
        tokio::spawn(async move { send_link_expiry_reminders(&db, &service, true).await })
    };
    for _ in 0..200 {
        if stamp(&db, before_link.id).await.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        stamp(&db, before_link.id).await.is_some(),
        "the run did not start"
    );
    // Time for the run to read `user`'s links and reach the held mail.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut moved: links::ActiveModel = row(&db, link.id).await.into();
    moved.expires_at = Set(Some(from_now(Duration::days(2))));
    moved.update(&db).await.expect("move the expiry");
    drop(hold);
    run.await.expect("join").expect("run");

    let mails = reminders_to(&user.email);
    assert_eq!(mails.len(), 1);
    let old = old_expiry.format("Expires %Y-%m-%d %H:%M UTC").to_string();
    assert!(
        mails[0].contains(&old),
        "the mail names the old date: {}",
        mails[0]
    );
    assert_eq!(
        stamp(&db, link.id).await,
        None,
        "the new date is not stamped"
    );

    // The next run reminds the owner of the new date.
    send_link_expiry_reminders(&db, &mailer(ROOMY), true)
        .await
        .expect("next run");
    assert_eq!(reminders_to(&user.email).len(), 2);
    assert!(stamp(&db, link.id).await.is_some());
}

// ---------------------------------------------- the stamp and expiry edits

struct Person {
    id: i32,
    token: String,
}

async fn person(db: &DatabaseConnection) -> Person {
    let user = owner(db, true).await;
    let token = create_jwt(user.id, &user.email, user.token_version).expect("mint token");
    Person { id: user.id, token }
}

/// A fresh app per request: the mock transport puts every caller in one
/// per-second bucket.
async fn app() -> TestServer {
    spawn_real_app().await.0
}

async fn put_link(token: &str, id: i32, body: Value) {
    let res = app()
        .await
        .put(&format!("/links/{id}"))
        .authorization_bearer(token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 200, "update: {}", res.text());
}

async fn bulk_update(token: &str, body: Value) {
    let res = app()
        .await
        .post("/links/bulk/update")
        .authorization_bearer(token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 200, "bulk update: {}", res.text());
    assert_eq!(res.json::<Value>()["updated"], 1, "{}", res.text());
}

fn rfc3339(at: NaiveDateTime) -> String {
    at.and_utc().to_rfc3339()
}

#[tokio::test]
async fn changing_the_expiry_clears_the_stamp_and_other_edits_keep_it() {
    let _serial = crate::common::serial_guard().await;
    // Expiries beyond the window: no reminder run touches these links.
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;
    let link = insert(&db, due_in(user.id, Duration::days(10))).await;
    let reminded = Some(from_now(-Duration::hours(1)));

    // Edits that leave the expiry alone keep the stamp, sending the stored
    // value back included.
    set_stamp(&db, link.id, reminded).await;
    put_link(&user.token, link.id, json!({ "title": "Renamed" })).await;
    assert_eq!(stamp(&db, link.id).await, reminded);
    let same = rfc3339(link.expires_at.unwrap());
    put_link(&user.token, link.id, json!({ "expires_at": same })).await;
    assert_eq!(stamp(&db, link.id).await, reminded, "the same date");

    let new_date = from_now(Duration::days(20));
    put_link(
        &user.token,
        link.id,
        json!({ "expires_at": rfc3339(new_date) }),
    )
    .await;
    assert_eq!(stamp(&db, link.id).await, None, "a new date");
    assert_eq!(row(&db, link.id).await.expires_at, Some(new_date));

    set_stamp(&db, link.id, reminded).await;
    put_link(&user.token, link.id, json!({ "remove_expiration": true })).await;
    assert_eq!(stamp(&db, link.id).await, None, "expiry removed");

    // Bulk update: the same rules.
    let bulk = insert(&db, due_in(user.id, Duration::days(10))).await;
    set_stamp(&db, bulk.id, reminded).await;
    let same = rfc3339(bulk.expires_at.unwrap());
    bulk_update(&user.token, json!({ "ids": [bulk.id], "expires_at": same })).await;
    assert_eq!(stamp(&db, bulk.id).await, reminded, "the same date");
    bulk_update(
        &user.token,
        json!({ "ids": [bulk.id], "expires_at": rfc3339(from_now(Duration::days(30))) }),
    )
    .await;
    assert_eq!(stamp(&db, bulk.id).await, None, "a new date");

    set_stamp(&db, bulk.id, reminded).await;
    bulk_update(
        &user.token,
        json!({ "ids": [bulk.id], "remove_expiration": true }),
    )
    .await;
    assert_eq!(stamp(&db, bulk.id).await, None, "expiry removed");
}

// ------------------------------------------------------ the account setting

#[tokio::test]
async fn the_profile_reports_and_changes_the_reminder_setting() {
    let _serial = crate::common::serial_guard().await;
    let (_, db) = spawn_real_app().await;
    let user = person(&db).await;

    let me = app()
        .await
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(me.status_code(), 200, "{}", me.text());
    assert_eq!(
        me.json::<Value>()["notify_link_expiry"],
        true,
        "on by default"
    );

    let res = app()
        .await
        .put("/auth/profile")
        .authorization_bearer(&user.token)
        .json(&json!({ "notify_link_expiry": false }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["notify_link_expiry"], false);
    let stored = users::Entity::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.notify_link_expiry);

    // A profile save that does not name the setting leaves it alone.
    let res = app()
        .await
        .put("/auth/profile")
        .authorization_bearer(&user.token)
        .json(&json!({ "display_name": "Ada" }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["notify_link_expiry"], false);

    let res = app()
        .await
        .put("/auth/profile")
        .authorization_bearer(&user.token)
        .json(&json!({ "notify_link_expiry": true }))
        .await;
    assert_eq!(res.status_code(), 200, "{}", res.text());
    let me = app()
        .await
        .get("/auth/me")
        .authorization_bearer(&user.token)
        .await;
    assert_eq!(me.json::<Value>()["notify_link_expiry"], true);
}

#[tokio::test]
async fn the_settings_say_whether_this_instance_sends_reminders() {
    let _serial = crate::common::serial_guard().await;
    async fn reminders_enabled(email_service: Option<Arc<EmailService>>) -> Value {
        let db = db().await;
        let mut state = opn_onl_backend::AppState::for_tests(db).await;
        state.email_service = email_service;
        let server = TestServer::new(opn_onl_backend::build_router(state)).expect("server");
        let res = server.get("/auth/settings").await;
        assert_eq!(res.status_code(), 200, "{}", res.text());
        res.json::<Value>()["link_expiry_reminders_enabled"].clone()
    }

    // The only test that reads the switch.
    common::remove_env("LINK_EXPIRY_REMINDERS");
    assert_eq!(reminders_enabled(Some(Arc::new(mailer(ROOMY)))).await, true);
    assert_eq!(reminders_enabled(None).await, false, "no SMTP");

    common::set_env("LINK_EXPIRY_REMINDERS", "Off");
    assert_eq!(
        reminders_enabled(Some(Arc::new(mailer(ROOMY)))).await,
        false
    );
    common::remove_env("LINK_EXPIRY_REMINDERS");
}
