//! Restoring a deleted account must not undo an email-domain block.
//!
//! A block disables the live, non-admin accounts on a domain. An account that
//! was deleted at the time is not live, so the block skipped it, and the admin
//! restore then brought it back enabled: the one account the block was meant to
//! cover was signing in again. A restored account on a blocked domain now comes
//! back disabled, exactly as the block would have left it, and the admin
//! enables it once the block is lifted. Real router, real Postgres.

mod common;

use common::{spawn_real_app, unique_email};
use opn_onl_backend::entity::users;
use opn_onl_backend::utils::create_jwt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::{Value, json};

fn fresh_domain() -> String {
    format!("restore-{}.iana.org", uuid::Uuid::new_v4().simple())
}

/// An admin inserted directly, with a token for the admin API.
async fn admin(db: &DatabaseConnection) -> (String, i32) {
    let email = unique_email();
    let admin = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert admin");
    let token = create_jwt(admin.id, &email, admin.token_version).expect("mint token");
    (token, admin.id)
}

/// A deleted account with an address on `domain`.
async fn deleted_user_on(db: &DatabaseConnection, domain: &str, is_admin: bool) -> users::Model {
    let now = chrono::Utc::now().naive_utc();
    users::ActiveModel {
        email: Set(format!("user-{}@{domain}", uuid::Uuid::new_v4().simple())),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(now),
        email_verified: Set(true),
        is_admin: Set(is_admin),
        deleted_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert deleted user")
}

async fn row(db: &DatabaseConnection, id: i32) -> users::Model {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .expect("user row")
}

async fn block(server: &axum_test::TestServer, admin_token: &str, domain: &str) -> Value {
    let res = server
        .post("/admin/blocked/email-domains")
        .authorization_bearer(admin_token)
        .json(&json!({ "domain": domain, "reason": "restore test" }))
        .await;
    assert_eq!(res.status_code(), 201, "block domain: {}", res.text());
    res.json()
}

async fn restore(server: &axum_test::TestServer, admin_token: &str, user_id: i32) -> Value {
    let res = server
        .post(&format!("/admin/users/{user_id}/restore"))
        .authorization_bearer(admin_token)
        .await;
    assert_eq!(res.status_code(), 200, "restore: {}", res.text());
    res.json()
}

#[tokio::test]
async fn a_restored_account_on_a_blocked_domain_comes_back_disabled() {
    let (server, db) = spawn_real_app().await;
    let (token, admin_id) = admin(&db).await;
    let domain = fresh_domain();
    let direct = deleted_user_on(&db, &domain, false).await;
    let subdomain = deleted_user_on(&db, &format!("mail.{domain}"), false).await;

    let block = block(&server, &token, &domain).await;
    // The block skipped them: they were not live.
    assert!(row(&db, direct.id).await.disabled_at.is_none());

    let body = restore(&server, &token, direct.id).await;
    let message = body["message"].as_str().unwrap();
    assert!(
        message.contains("disabled") && message.contains(&domain),
        "the response says why the account is not usable: {message}"
    );
    let restored = row(&db, direct.id).await;
    assert!(restored.deleted_at.is_none(), "it is back among the living");
    assert!(restored.disabled_at.is_some(), "but disabled");
    assert_eq!(restored.disabled_by, Some(admin_id));
    assert!(
        restored
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains(&domain)),
        "{:?}",
        restored.disabled_reason
    );

    // A block covers subdomains, and so does the check.
    restore(&server, &token, subdomain.id).await;
    assert!(row(&db, subdomain.id).await.disabled_at.is_some());

    // The existing block semantics apply from here: it cannot be enabled while
    // the domain is blocked, and can be once the block is lifted.
    let res = server
        .post(&format!("/admin/users/{}/enable", direct.id))
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 409, "enable: {}", res.text());
    let res = server
        .delete(&format!(
            "/admin/blocked/email-domains/{}",
            block["id"].as_i64().unwrap()
        ))
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "unblock: {}", res.text());
    let res = server
        .post(&format!("/admin/users/{}/enable", direct.id))
        .authorization_bearer(&token)
        .await;
    assert_eq!(res.status_code(), 200, "enable: {}", res.text());
    assert!(row(&db, direct.id).await.disabled_at.is_none());
}

/// Controls: an account on a domain nobody blocked, and an admin on a blocked
/// one (blocks leave admins alone so one can always lift them), come back
/// enabled, with the plain message.
#[tokio::test]
async fn other_restores_are_unchanged() {
    let (server, db) = spawn_real_app().await;
    let (token, _) = admin(&db).await;
    let blocked = fresh_domain();
    block(&server, &token, &blocked).await;

    let unblocked = deleted_user_on(&db, &fresh_domain(), false).await;
    let admin_on_blocked = deleted_user_on(&db, &blocked, true).await;

    for user in [&unblocked, &admin_on_blocked] {
        let body = restore(&server, &token, user.id).await;
        assert_eq!(
            body["message"],
            format!("User {} restored", user.id),
            "{body}"
        );
        let restored = row(&db, user.id).await;
        assert!(restored.deleted_at.is_none());
        assert!(restored.disabled_at.is_none(), "{}", restored.email);
    }
}
