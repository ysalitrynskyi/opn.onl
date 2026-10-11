//! Deleting an account closes it at once but keeps the data for a grace window
//! (30 days) so an admin can restore it; after the window the daily sweep removes
//! the account, its links and everything that hangs off them. Real router and
//! real Postgres; accounts are aged by writing `deleted_at` directly.

mod common;

use axum_test::TestServer;
use chrono::{Duration, Utc};
use common::{mark_email_verified, setup_test_db, unique_code, unique_email};
use opn_onl_backend::entity::{
    api_keys, click_events, links, org_members, organizations, passkeys, retired_codes, users,
};
use opn_onl_backend::utils::account_purge::purge_expired_accounts;
use opn_onl_backend::utils::jwt::create_jwt;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::sync::Arc;

const WINDOW_DAYS: i32 = 30;

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

/// The real router with limits wide enough for a test that signs up several
/// accounts and makes dozens of requests from one address.
async fn spawn_real_app() -> (TestServer, DatabaseConnection) {
    common::set_env("FORCE_HTTPS", "false");
    common::set_env("TRUST_PROXY_HEADERS", "false");
    if std::env::var("JWT_SECRET").is_err() {
        common::set_env("JWT_SECRET", "integration-test-secret-0123456789abcdef");
    }
    let db = setup_test_db().await;
    let mut state = opn_onl_backend::AppState::for_tests(db.clone()).await;
    state.rate_limiters = Arc::new(RateLimiters {
        per_second: roomy(),
        general: roomy(),
        link_creation: roomy(),
        auth: roomy(),
        redirect: roomy(),
        password_verify: roomy(),
        password_verify_ip: roomy(),
        contact: roomy(),
    });
    let server = TestServer::new(opn_onl_backend::build_router(state)).expect("test server");
    (server, db)
}

struct Account {
    id: i32,
    email: String,
    token: String,
}

/// A verified account made through the real sign-up. The very first account on
/// an empty database becomes the instance admin, which the last-admin rule
/// would stop from deleting itself, so demote it.
async fn register(server: &TestServer, db: &DatabaseConnection) -> Account {
    let email = unique_email();
    let res = server
        .post("/auth/register")
        .json(&json!({ "email": email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 201, "register: {}", res.text());
    let body: Value = res.json();
    let id = body["user_id"].as_i64().expect("user id") as i32;
    let user = users::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    if user.is_admin {
        let mut active: users::ActiveModel = user.into();
        active.is_admin = Set(false);
        active.update(db).await.expect("demote bootstrap admin");
    }
    mark_email_verified(db, id).await;
    Account {
        id,
        email,
        token: body["token"].as_str().expect("token").to_string(),
    }
}

/// An instance admin inserted directly, with a token for the admin API.
async fn admin_token(db: &DatabaseConnection) -> String {
    let admin = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert admin");
    create_jwt(admin.id, &admin.email, admin.token_version).expect("mint token")
}

async fn create_link(server: &TestServer, who: &Account, body: Value) -> (i32, String) {
    let res = server
        .post("/links")
        .authorization_bearer(&who.token)
        .json(&body)
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().expect("id") as i32,
        body["code"].as_str().expect("code").to_string(),
    )
}

async fn create_org(server: &TestServer, owner: &Account) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Purge Org",
            "slug": format!("purge-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().expect("org id") as i32
}

async fn invite(server: &TestServer, owner: &Account, org_id: i32, who: &Account) {
    let res = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "email": who.email, "role": "editor" }))
        .await;
    assert_eq!(res.status_code(), 201, "invite: {}", res.text());
}

async fn delete_own_account(server: &TestServer, who: &Account) {
    let res = server
        .post("/auth/delete-account")
        .authorization_bearer(&who.token)
        .json(&json!({ "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "self delete: {}", res.text());
}

/// Pretend the account was deleted `days` ago. The links that went down with it
/// carry its exact deletion stamp (that is how a restore finds them), so they
/// move together.
async fn deleted_days_ago(db: &DatabaseConnection, user_id: i32, days: i64) {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .expect("user");
    let taken_down_at = user.deleted_at.expect("delete the account first");
    let stamp = Utc::now().naive_utc() - Duration::days(days);
    links::Entity::update_many()
        .col_expr(links::Column::DeletedAt, Expr::value(stamp))
        .filter(links::Column::UserId.eq(user_id))
        .filter(links::Column::DeletedAt.eq(taken_down_at))
        .exec(db)
        .await
        .expect("age links");
    let mut active: users::ActiveModel = user.into();
    active.deleted_at = Set(Some(stamp));
    active.update(db).await.expect("age user");
}

async fn sweep(db: &DatabaseConnection) {
    purge_expired_accounts(db, WINDOW_DAYS)
        .await
        .expect("purge sweep");
}

async fn user_exists(db: &DatabaseConnection, user_id: i32) -> bool {
    users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .is_some()
}

async fn is_retired(db: &DatabaseConnection, code: &str) -> bool {
    retired_codes::Entity::find_by_id(code)
        .one(db)
        .await
        .unwrap()
        .is_some()
}

async fn link_rows(db: &DatabaseConnection, ids: &[i32]) -> u64 {
    links::Entity::find()
        .filter(links::Column::Id.is_in(ids.to_vec()))
        .count(db)
        .await
        .unwrap()
}

async fn click_rows(db: &DatabaseConnection, ids: &[i32]) -> u64 {
    click_events::Entity::find()
        .filter(click_events::Column::LinkId.is_in(ids.to_vec()))
        .count(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_deleted_account_is_erased_once_the_window_has_passed() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (server, db) = spawn_real_app().await;
    let admin = admin_token(&db).await;
    let user = register(&server, &db).await;

    // Personal data on the account row: profile fields and a pending token.
    let tag = unique_code();
    let mut profile: users::ActiveModel = users::Entity::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .into();
    profile.display_name = Set(Some(format!("Dana Delete {tag}")));
    profile.bio = Set(Some(format!("bio {tag}")));
    profile.website = Set(Some(format!("https://iana.org/dana-{tag}")));
    profile.location = Set(Some(format!("Kyiv {tag}")));
    profile.avatar_url = Set(Some(format!("https://iana.org/avatar-{tag}.png")));
    profile.bio_theme = Set(Some(format!("theme-{tag}")));
    profile.verification_token = Set(Some(format!("pending-{tag}")));
    profile.update(&db).await.expect("seed profile");

    // Personal links with a title and notes, one of them deleted on its own
    // earlier, plus clicks on the first.
    let mut personal = vec![];
    for i in 0..3 {
        personal.push(
            create_link(
                &server,
                &user,
                json!({
                    "original_url": format!("https://iana.org/private-{tag}-{i}"),
                    "title": format!("Private title {tag} {i}"),
                    "notes": format!("Private notes {tag} {i}"),
                }),
            )
            .await,
        );
    }
    let res = server
        .delete(&format!("/links/{}", personal[2].0))
        .authorization_bearer(&user.token)
        .await;
    assert!(
        res.status_code().is_success(),
        "soft delete: {}",
        res.text()
    );
    for _ in 0..2 {
        click_events::ActiveModel {
            link_id: Set(personal[0].0),
            created_at: Set(Utc::now().naive_utc()),
            country: Set(Some("UA".to_string())),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert click");
    }
    let personal_ids: Vec<i32> = personal.iter().map(|(id, _)| *id).collect();

    // A link made inside somebody else's organization belongs to that team.
    let team_owner = register(&server, &db).await;
    let team_org = create_org(&server, &team_owner).await;
    invite(&server, &team_owner, team_org, &user).await;
    let (team_link_id, team_code) = create_link(
        &server,
        &user,
        json!({ "original_url": "https://iana.org/team", "org_id": team_org }),
    )
    .await;

    // An organization of their own that nobody else belongs to.
    let solo_org = create_org(&server, &user).await;
    let (solo_link_id, solo_code) = create_link(
        &server,
        &user,
        json!({ "original_url": "https://iana.org/solo", "org_id": solo_org }),
    )
    .await;

    delete_own_account(&server, &user).await;

    // Inside the window the account is closed but nothing is gone: the row
    // holds its data and links stay in the table for a restore.
    assert_eq!(
        server
            .get(&format!("/{}", personal[0].1))
            .await
            .status_code(),
        404,
        "the links stop working at once"
    );
    sweep(&db).await;
    let kept = users::Entity::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("still inside the window");
    assert!(kept.deleted_at.is_some());
    assert_eq!(kept.email, user.email);
    assert_eq!(link_rows(&db, &personal_ids).await, 3);

    // Accounts deleted before credentials were cleaned up on delete still carry
    // them; the purge must not leave those behind either.
    passkeys::ActiveModel {
        user_id: Set(user.id),
        cred_id: Set(format!("purge-cred-{tag}")),
        cred_public_key: Set("legacy".to_string()),
        counter: Set(0),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert passkey");
    api_keys::ActiveModel {
        user_id: Set(user.id),
        name: Set("legacy key".to_string()),
        key_hash: Set(format!("purge-hash-{tag}")),
        key_prefix: Set("opn_legacy".to_string()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert api key");

    deleted_days_ago(&db, user.id, 31).await;
    sweep(&db).await;

    assert!(!user_exists(&db, user.id).await, "the account row is gone");
    assert_eq!(
        users::Entity::find()
            .filter(users::Column::Email.eq(&user.email))
            .count(&db)
            .await
            .unwrap(),
        0,
        "the email address is not stored anywhere on the users table"
    );
    assert_eq!(
        link_rows(&db, &personal_ids).await,
        0,
        "soft-deleted links go with their URL, title and notes"
    );
    assert_eq!(click_rows(&db, &personal_ids).await, 0);
    assert_eq!(
        passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(user.id))
            .count(&db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        api_keys::Entity::find()
            .filter(api_keys::Column::UserId.eq(user.id))
            .count(&db)
            .await
            .unwrap(),
        0
    );

    // The removed codes stay reserved, so nobody inherits the old traffic.
    for (_, code) in &personal {
        assert!(is_retired(&db, code).await, "{code} was not retired");
    }
    let res = server
        .get("/links/check-code")
        .add_query_param("code", &personal[0].1)
        .await;
    assert_eq!(res.json::<Value>()["available"], json!(false));

    // The organization nobody else was in goes with its link.
    assert_eq!(link_rows(&db, &[solo_link_id]).await, 0);
    assert!(is_retired(&db, &solo_code).await);
    assert!(
        organizations::Entity::find_by_id(solo_org)
            .one(&db)
            .await
            .unwrap()
            .is_none()
    );

    // The team's link stays, owned by the team owner, with its code free.
    let team_link = links::Entity::find_by_id(team_link_id)
        .one(&db)
        .await
        .unwrap()
        .expect("the team keeps its link");
    assert_eq!(team_link.user_id, Some(team_owner.id));
    assert!(!is_retired(&db, &team_code).await);

    // Nothing is left to restore.
    let res = server
        .post(&format!("/admin/users/{}/restore", user.id))
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 404, "restore: {}", res.text());
}

#[tokio::test]
async fn an_account_inside_the_window_survives_the_sweep_and_can_be_restored() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (server, db) = spawn_real_app().await;
    let admin = admin_token(&db).await;
    let user = register(&server, &db).await;
    let (link_id, code) = create_link(
        &server,
        &user,
        json!({ "original_url": "https://iana.org/keep", "title": "Keep me" }),
    )
    .await;

    delete_own_account(&server, &user).await;
    deleted_days_ago(&db, user.id, 29).await;
    sweep(&db).await;

    let still_there = users::Entity::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("29 days is inside the 30-day window");
    assert_eq!(still_there.email, user.email);
    assert_eq!(link_rows(&db, &[link_id]).await, 1);
    assert!(!is_retired(&db, &code).await);

    let res = server
        .post(&format!("/admin/users/{}/restore", user.id))
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200, "restore: {}", res.text());
    assert!(
        server
            .get(&format!("/{code}"))
            .await
            .status_code()
            .is_redirection(),
        "the restored account's link works again"
    );
    let login = server
        .post("/auth/login")
        .json(&json!({ "email": user.email, "password": "password123" }))
        .await;
    assert_eq!(
        login.status_code(),
        200,
        "login after restore: {}",
        login.text()
    );
}

#[tokio::test]
async fn the_window_is_counted_from_the_deletion_not_from_sign_up() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (server, db) = spawn_real_app().await;
    let user = register(&server, &db).await;

    // Signed up long ago, deleted just now: not due.
    let old_signup = Utc::now().naive_utc() - Duration::days(400);
    let mut active: users::ActiveModel = users::Entity::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .into();
    active.created_at = Set(old_signup);
    active.update(&db).await.unwrap();
    delete_own_account(&server, &user).await;

    sweep(&db).await;
    assert!(user_exists(&db, user.id).await);
}

#[tokio::test]
async fn a_live_account_is_never_swept() {
    let (server, db) = spawn_real_app().await;
    let user = register(&server, &db).await;
    let mut active: users::ActiveModel = users::Entity::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .into();
    active.created_at = Set(Utc::now().naive_utc() - Duration::days(1000));
    active.update(&db).await.unwrap();

    sweep(&db).await;
    assert!(user_exists(&db, user.id).await);
}

#[tokio::test]
async fn a_deleted_owner_of_a_team_organization_is_kept_while_it_has_members() {
    common::set_env("ENABLE_ACCOUNT_DELETION", "true");
    let (server, db) = spawn_real_app().await;
    let admin = admin_token(&db).await;
    let owner = register(&server, &db).await;
    let member = register(&server, &db).await;
    let bystander = register(&server, &db).await;
    let org_id = create_org(&server, &owner).await;
    invite(&server, &owner, org_id, &member).await;

    // How the state arises: the only other member is deleted, so the owner may
    // delete too (the org looks solo), and then the member is restored.
    let res = server
        .delete(&format!("/admin/users/{}", member.id))
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200, "delete member: {}", res.text());
    delete_own_account(&server, &owner).await;
    let res = server
        .post(&format!("/admin/users/{}/restore", member.id))
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 200, "restore member: {}", res.text());
    // The restore revokes the member's old tokens; sign in again.
    let res = server
        .post("/auth/login")
        .json(&json!({ "email": member.email, "password": "password123" }))
        .await;
    assert_eq!(res.status_code(), 200, "member login: {}", res.text());
    let member_token = res.json::<Value>()["token"].as_str().unwrap().to_string();

    // A second account that is due, to show one kept account does not stop the sweep.
    delete_own_account(&server, &bystander).await;
    deleted_days_ago(&db, owner.id, 31).await;
    deleted_days_ago(&db, bystander.id, 31).await;
    sweep(&db).await;

    assert!(
        user_exists(&db, owner.id).await,
        "the owner of an organization with a live member is not removed"
    );
    assert!(
        organizations::Entity::find_by_id(org_id)
            .one(&db)
            .await
            .unwrap()
            .is_some()
    );
    let res = server
        .get(&format!("/orgs/{org_id}"))
        .authorization_bearer(&member_token)
        .await;
    assert_eq!(res.status_code(), 200, "org stays visible: {}", res.text());
    assert!(
        !user_exists(&db, bystander.id).await,
        "other due accounts are still removed"
    );
    // The admin hard delete refuses the same account for the same reason.
    let res = server
        .delete(&format!("/admin/users/{}/hard", owner.id))
        .authorization_bearer(&admin)
        .await;
    assert_eq!(res.status_code(), 409, "hard delete: {}", res.text());

    // Once nobody else is in the organization it goes with its owner.
    org_members::Entity::delete_many()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(member.id))
        .exec(&db)
        .await
        .unwrap();
    sweep(&db).await;
    assert!(!user_exists(&db, owner.id).await);
    assert!(
        organizations::Entity::find_by_id(org_id)
            .one(&db)
            .await
            .unwrap()
            .is_none()
    );
}

/// Insert a bare account row with the given deletion stamp.
async fn bare_user(
    db: &DatabaseConnection,
    deleted_at: Option<chrono::NaiveDateTime>,
) -> users::Model {
    users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(false),
        deleted_at: Set(deleted_at),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

#[tokio::test]
async fn a_backlog_larger_than_one_batch_is_swept_past_a_kept_account() {
    let (_server, db) = spawn_real_app().await;
    let aged = Utc::now().naive_utc() - Duration::days(45);

    // The lowest id is a kept account (it owns an organization with a live
    // member), so every batch starts with something that cannot be removed.
    let kept = bare_user(&db, Some(aged)).await;
    let live_member = bare_user(&db, None).await;
    let org = organizations::ActiveModel {
        name: Set("Kept".to_string()),
        slug: Set(format!("kept-{}", uuid::Uuid::new_v4().simple())),
        owner_id: Set(kept.id),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert org");
    for (user_id, role) in [(kept.id, "owner"), (live_member.id, "editor")] {
        org_members::ActiveModel {
            org_id: Set(org.id),
            user_id: Set(user_id),
            role: Set(role.to_string()),
            joined_at: Set(Utc::now().naive_utc()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert member");
    }

    let mut due = vec![];
    for _ in 0..105 {
        due.push(bare_user(&db, Some(aged)).await.id);
    }

    sweep(&db).await;

    let left = users::Entity::find()
        .filter(users::Column::Id.is_in(due))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(
        left, 0,
        "every due account is removed, not just the first batch"
    );
    assert!(user_exists(&db, kept.id).await);
}
