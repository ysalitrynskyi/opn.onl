//! Short codes of hard-deleted links stay reserved. Soft-deleted links already
//! keep theirs (the row holds the code); an organization purge and an admin
//! hard delete remove the rows, so they retire the codes instead.

mod common;

use axum_test::TestServer;
use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::{links, retired_codes, users};
use opn_onl_backend::handlers::links::unused_code;
use opn_onl_backend::utils::jwt::create_jwt;
use opn_onl_backend::utils::rate_limiter::{RateLimitConfig, RateLimiter, RateLimiters};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn roomy() -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(RateLimitConfig::new(1_000_000, 60)))
}

async fn app() -> (TestServer, DatabaseConnection) {
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

struct Person {
    id: i32,
    email: String,
    token: String,
}

async fn person(db: &DatabaseConnection, admin: bool) -> Person {
    let user = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(admin),
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

fn alias() -> String {
    format!("rc{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

async fn create_org(server: &TestServer, owner: &Person) -> i32 {
    let res = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Retired Codes Org",
            "slug": format!("rc-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create org: {}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap() as i32
}

/// Create a link, with `alias` as its code when given. Returns `(id, code)`.
async fn create_link(
    server: &TestServer,
    who: &Person,
    alias: Option<&str>,
    org_id: Option<i32>,
) -> (i32, String) {
    let res = server
        .post("/links")
        .authorization_bearer(&who.token)
        .json(&json!({
            "original_url": "https://iana.org/retired",
            "custom_alias": alias,
            "org_id": org_id,
        }))
        .await;
    assert_eq!(res.status_code(), 201, "create link: {}", res.text());
    let body: Value = res.json();
    (
        body["id"].as_i64().unwrap() as i32,
        body["code"].as_str().unwrap().to_string(),
    )
}

async fn is_retired(db: &DatabaseConnection, code: &str) -> bool {
    retired_codes::Entity::find_by_id(code)
        .one(db)
        .await
        .unwrap()
        .is_some()
}

async fn check_code_available(server: &TestServer, code: &str) -> bool {
    let res = server
        .get("/links/check-code")
        .add_query_param("code", code)
        .await;
    assert_eq!(res.status_code(), 200, "check-code: {}", res.text());
    res.json::<Value>()["available"].as_bool().unwrap()
}

/// Nobody can claim `code` as an alias, and `check-code` agrees.
async fn assert_code_refused(server: &TestServer, claimant: &Person, code: &str) {
    assert!(
        !check_code_available(server, code).await,
        "check-code offers retired code {code}"
    );
    let res = server
        .post("/links")
        .authorization_bearer(&claimant.token)
        .json(&json!({ "original_url": "https://iana.org/claim", "custom_alias": code }))
        .await;
    assert_eq!(res.status_code(), 409, "claim of {code}: {}", res.text());
    assert!(
        res.json::<Value>()["error"]
            .as_str()
            .unwrap()
            .contains("cannot be reused"),
        "the refusal must read like a soft-deleted alias: {}",
        res.text()
    );
}

#[tokio::test]
async fn deleting_an_org_retires_every_code_it_issued() {
    let (server, db) = app().await;
    let owner = person(&db, false).await;
    let claimant = person(&db, false).await;
    let org_id = create_org(&server, &owner).await;

    let (_, live_alias) = create_link(&server, &owner, Some(&alias()), Some(org_id)).await;
    let (soft_id, soft_alias) = create_link(&server, &owner, Some(&alias()), Some(org_id)).await;
    let (_, random_code) = create_link(&server, &owner, None, Some(org_id)).await;
    let res = server
        .delete(&format!("/links/{soft_id}"))
        .authorization_bearer(&owner.token)
        .await;
    assert!(
        res.status_code().is_success(),
        "soft delete: {}",
        res.text()
    );

    // Before the purge the rows hold the codes; nothing is retired yet.
    assert!(!check_code_available(&server, &live_alias).await);
    assert!(!is_retired(&db, &live_alias).await);

    let res = server
        .delete(&format!("/orgs/{org_id}"))
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(res.status_code(), 204, "delete org: {}", res.text());

    assert_eq!(
        links::Entity::find()
            .filter(links::Column::OrgId.eq(org_id))
            .count(&db)
            .await
            .unwrap(),
        0,
        "the purge removes the link rows"
    );
    for code in [&live_alias, &soft_alias, &random_code] {
        assert!(is_retired(&db, code).await, "{code} was not retired");
    }

    assert_code_refused(&server, &claimant, &live_alias).await;
    assert_code_refused(&server, &claimant, &soft_alias).await;
    assert!(
        !check_code_available(&server, &random_code).await,
        "a random code is reserved too"
    );
    assert_eq!(
        server.get(&format!("/{live_alias}")).await.status_code(),
        404,
        "the old link no longer redirects"
    );

    // Control: an alias nobody ever held is still free.
    let fresh = alias();
    assert!(check_code_available(&server, &fresh).await);
    let (_, code) = create_link(&server, &claimant, Some(&fresh), None).await;
    assert_eq!(code, fresh);
}

#[tokio::test]
async fn an_admin_hard_delete_retires_the_codes_it_removes() {
    let (server, db) = app().await;
    let admin = person(&db, true).await;
    let victim = person(&db, false).await;
    let team_owner = person(&db, false).await;
    let claimant = person(&db, false).await;

    // The victim's personal links (one soft-deleted), a solo org of theirs, and
    // a link in somebody else's org, which survives the account.
    let (_, personal_alias) = create_link(&server, &victim, Some(&alias()), None).await;
    let (soft_id, soft_alias) = create_link(&server, &victim, Some(&alias()), None).await;
    let res = server
        .delete(&format!("/links/{soft_id}"))
        .authorization_bearer(&victim.token)
        .await;
    assert!(
        res.status_code().is_success(),
        "soft delete: {}",
        res.text()
    );
    let solo_org = create_org(&server, &victim).await;
    let (_, solo_org_alias) = create_link(&server, &victim, Some(&alias()), Some(solo_org)).await;
    let team_org = create_org(&server, &team_owner).await;
    let invited = server
        .post(&format!("/orgs/{team_org}/members"))
        .authorization_bearer(&team_owner.token)
        .json(&json!({ "email": &victim.email, "role": "editor" }))
        .await;
    assert_eq!(invited.status_code(), 201, "invite: {}", invited.text());
    let (team_link_id, team_alias) =
        create_link(&server, &victim, Some(&alias()), Some(team_org)).await;

    let res = server
        .delete(&format!("/admin/users/{}/hard", victim.id))
        .authorization_bearer(&admin.token)
        .await;
    assert_eq!(res.status_code(), 200, "hard delete: {}", res.text());

    for code in [&personal_alias, &soft_alias, &solo_org_alias] {
        assert!(is_retired(&db, code).await, "{code} was not retired");
        assert_code_refused(&server, &claimant, code).await;
    }

    // The team's link lives on under the org owner and keeps its code.
    assert!(!is_retired(&db, &team_alias).await);
    let kept = links::Entity::find_by_id(team_link_id)
        .one(&db)
        .await
        .unwrap()
        .expect("team link survives");
    assert_eq!(kept.user_id, Some(team_owner.id));
    assert_eq!(kept.code, team_alias);
}

#[tokio::test]
async fn code_generation_skips_retired_live_and_soft_deleted_codes() {
    let (_server, db) = app().await;
    let owner = person(&db, false).await;
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let retired = format!("rt{tag}");
    let live = format!("lv{tag}");
    let soft = format!("sd{tag}");
    let free = format!("fr{tag}");

    retired_codes::ActiveModel {
        code: Set(retired.clone()),
        retired_at: Set(chrono::Utc::now().naive_utc()),
    }
    .insert(&db)
    .await
    .expect("retire a code");
    for (code, deleted) in [(&live, false), (&soft, true)] {
        links::ActiveModel {
            code: Set(code.clone()),
            original_url: Set("https://iana.org/taken".to_string()),
            user_id: Set(Some(owner.id)),
            deleted_at: Set(deleted.then(|| chrono::Utc::now().naive_utc())),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert link");
    }

    let mut candidates = [retired, live, soft, free.clone()].into_iter();
    let mut tried = 0;
    let got = unused_code(&db, || {
        tried += 1;
        candidates.next().expect("ran out of candidates")
    })
    .await
    .expect("the free candidate is issued");
    assert_eq!(got, free, "the first three candidates are all unavailable");
    assert_eq!(tried, 4);
}
