//! Organization audit entries must not keep an invitee's address after the
//! invitee's account is gone.
//!
//! An invite entry used to store the invited address in full in its `details`.
//! The entry outlives the account (it belongs to the inviting organization's
//! log), so purging an account left its address behind. New entries now carry
//! a masked form (`j***@example.com`), and removing an account for good masks
//! any entry that still holds its address, whether the sweep or an admin hard
//! delete removes it. Real router, real Postgres.

mod common;

use chrono::{Duration, Utc};
use common::{spawn_real_app, unique_email};
use opn_onl_backend::entity::{audit_log, users};
use opn_onl_backend::utils::account_purge::purge_expired_accounts;
use opn_onl_backend::utils::create_jwt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde_json::{Value, json};

struct Person {
    id: i32,
    email: String,
    token: String,
}

async fn person(db: &DatabaseConnection, deleted_days_ago: Option<i64>, admin: bool) -> Person {
    let email = unique_email();
    let user = users::ActiveModel {
        email: Set(email.clone()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        is_admin: Set(admin),
        deleted_at: Set(deleted_days_ago.map(|days| Utc::now().naive_utc() - Duration::days(days))),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");
    let token = create_jwt(user.id, &email, user.token_version).expect("mint token");
    Person {
        id: user.id,
        email,
        token,
    }
}

/// `t***@users.opn.onl` for `test_…@users.opn.onl`.
fn masked(email: &str) -> String {
    let (local, domain) = email.rsplit_once('@').unwrap();
    format!("{}***@{domain}", &local[..1])
}

async fn audit_entry(db: &DatabaseConnection, details: Option<Value>) -> i32 {
    audit_log::ActiveModel {
        action: Set("invite".to_string()),
        resource_type: Set("member".to_string()),
        details: Set(details),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert audit entry")
    .id
}

async fn details(db: &DatabaseConnection, id: i32) -> Option<Value> {
    audit_log::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .expect("audit entry")
        .details
}

#[tokio::test]
async fn an_invite_is_logged_with_a_masked_address() {
    let (server, db) = spawn_real_app().await;
    let owner = person(&db, None, false).await;
    let invitee = person(&db, None, false).await;

    let org = server
        .post("/orgs")
        .authorization_bearer(&owner.token)
        .json(&json!({
            "name": "Audit Mask Org",
            "slug": format!("mask-{}", uuid::Uuid::new_v4().simple()),
        }))
        .await;
    assert_eq!(org.status_code(), 201, "create org: {}", org.text());
    let org_id = org.json::<Value>()["id"].as_i64().unwrap() as i32;

    // Typed in capitals: the entry still names the account's own spelling.
    let invite = server
        .post(&format!("/orgs/{org_id}/members"))
        .authorization_bearer(&owner.token)
        .json(&json!({ "email": invitee.email.to_uppercase(), "role": "viewer" }))
        .await;
    assert_eq!(invite.status_code(), 201, "invite: {}", invite.text());

    let log = server
        .get(&format!("/orgs/{org_id}/audit"))
        .authorization_bearer(&owner.token)
        .await;
    assert_eq!(log.status_code(), 200, "audit: {}", log.text());
    let entries = log.json::<Value>();
    let entry = entries
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["action"] == "invite")
        .unwrap_or_else(|| panic!("an invite entry in {entries}"));
    assert_eq!(entry["details"]["email"], masked(&invitee.email), "{entry}");
    assert_eq!(entry["details"]["role"], "viewer", "{entry}");

    let stored = audit_log::Entity::find()
        .filter(audit_log::Column::OrgId.eq(org_id))
        .all(&db)
        .await
        .unwrap();
    assert!(
        stored.iter().all(|row| {
            let text = serde_json::to_string(&row.details).unwrap().to_lowercase();
            !text.contains(&invitee.email.to_lowercase())
        }),
        "the address must not be stored in full: {stored:?}"
    );
}

/// Entries written before invites were masked hold the whole address.
async fn seed_entries(db: &DatabaseConnection, email: &str, other: &str) -> Entries {
    Entries {
        exact: audit_entry(db, Some(json!({ "email": email, "role": "editor" }))).await,
        retyped: audit_entry(
            db,
            Some(json!({ "email": format!("  {}  ", email.to_uppercase()), "role": "viewer" })),
        )
        .await,
        other_person: audit_entry(db, Some(json!({ "email": other, "role": "viewer" }))).await,
        no_address: audit_entry(
            db,
            Some(json!({ "old_role": "viewer", "new_role": "editor" })),
        )
        .await,
        no_details: audit_entry(db, None).await,
    }
}

struct Entries {
    exact: i32,
    retyped: i32,
    other_person: i32,
    no_address: i32,
    no_details: i32,
}

async fn assert_scrubbed(db: &DatabaseConnection, entries: &Entries, email: &str, other: &str) {
    let masked = masked(email);
    for (name, id, role) in [
        ("exact", entries.exact, "editor"),
        ("retyped", entries.retyped, "viewer"),
    ] {
        let details = details(db, id).await.expect("details stay");
        assert_eq!(details["email"], masked, "{name}: {details}");
        assert_eq!(details["role"], role, "{name}: the rest of the entry stays");
    }
    // Controls: nobody else's entry, and entries without an address, are
    // left alone.
    assert_eq!(
        details(db, entries.other_person).await.unwrap()["email"],
        other
    );
    assert_eq!(
        details(db, entries.no_address).await.unwrap(),
        json!({ "old_role": "viewer", "new_role": "editor" })
    );
    assert_eq!(details(db, entries.no_details).await, None);
}

#[tokio::test]
async fn the_purge_sweep_masks_the_address_of_an_account_it_removes() {
    let (_, db) = spawn_real_app().await;
    let gone = person(&db, None, false).await;
    let other = person(&db, None, false).await;
    let entries = seed_entries(&db, &gone.email, &other.email).await;
    // Only now does the account fall outside the window. Other tests in this
    // file run the sweep concurrently over the same database; had the account
    // been purgeable while its entries were still being written, one of those
    // sweeps could remove it between two inserts and leave the later entry
    // holding the full address with no account left to scrub it.
    let mut expired: users::ActiveModel = users::Entity::find_by_id(gone.id)
        .one(&db)
        .await
        .unwrap()
        .expect("account")
        .into();
    expired.deleted_at = Set(Some(Utc::now().naive_utc() - Duration::days(40)));
    expired.update(&db).await.expect("expire the account");

    purge_expired_accounts(&db, 30).await.expect("sweep");

    assert!(
        users::Entity::find_by_id(gone.id)
            .one(&db)
            .await
            .unwrap()
            .is_none(),
        "the account is gone"
    );
    assert_scrubbed(&db, &entries, &gone.email, &other.email).await;
}

#[tokio::test]
async fn an_admin_hard_delete_masks_the_address_too() {
    let (server, db) = spawn_real_app().await;
    let admin = person(&db, None, true).await;
    let gone = person(&db, None, false).await;
    let other = person(&db, None, false).await;
    let entries = seed_entries(&db, &gone.email, &other.email).await;

    let res = server
        .delete(&format!("/admin/users/{}/hard", gone.id))
        .authorization_bearer(&admin.token)
        .await;
    assert_eq!(res.status_code(), 200, "hard delete: {}", res.text());

    assert_scrubbed(&db, &entries, &gone.email, &other.email).await;
}

/// Control: an account that is only soft-deleted keeps its entries as they
/// were; the address goes when the account does.
#[tokio::test]
async fn an_account_inside_the_window_keeps_its_entries() {
    let (_, db) = spawn_real_app().await;
    let recent = person(&db, Some(10), false).await;
    let entry = audit_entry(
        &db,
        Some(json!({ "email": recent.email, "role": "viewer" })),
    )
    .await;

    purge_expired_accounts(&db, 30).await.expect("sweep");

    assert_eq!(
        details(&db, entry).await.unwrap()["email"],
        recent.email,
        "restorable for now"
    );
}
