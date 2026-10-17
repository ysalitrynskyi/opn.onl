//! The daily retention task removes old deleted links on its first run, with
//! the same window as deleted accounts (`ACCOUNT_PURGE_DAYS`, default 30).
//! It runs alone (`integration_serial`), like the account version of this
//! test: another test sweeping the same database could remove the link first
//! and hide a task that never calls the purge.

use crate::common;

use chrono::{Duration, Utc};
use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::{links, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

#[tokio::test]
async fn the_daily_retention_task_purges_old_deleted_links_when_it_starts() {
    let _serial = crate::common::serial_guard().await;
    let db = setup_test_db().await;
    let owner = users::ActiveModel {
        email: Set(unique_email()),
        password_hash: Set("not-a-real-hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        email_verified: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert owner")
    .id;
    let insert = |deleted_days_ago: Option<i64>| {
        links::ActiveModel {
            code: Set(format!(
                "dt{}",
                &uuid::Uuid::new_v4().simple().to_string()[..12]
            )),
            original_url: Set("https://iana.org/task".to_string()),
            user_id: Set(Some(owner)),
            deleted_at: Set(
                deleted_days_ago.map(|days| Utc::now().naive_utc() - Duration::days(days))
            ),
            ..Default::default()
        }
        .insert(&db)
    };
    let expired = insert(Some(40)).await.expect("insert expired").id;
    let recent = insert(Some(10)).await.expect("insert recent").id;
    let live = insert(None).await.expect("insert live").id;

    opn_onl_backend::utils::privacy::spawn_retention_task(db.clone());

    let exists = |id: i32| {
        let db = db.clone();
        async move {
            links::Entity::find_by_id(id)
                .one(&db)
                .await
                .unwrap()
                .is_some()
        }
    };
    let mut purged = false;
    for _ in 0..100 {
        if !exists(expired).await {
            purged = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        purged,
        "the retention task never purged the old deleted link"
    );
    assert!(exists(recent).await, "a link inside the window stays");
    assert!(exists(live).await, "a live link stays");
}
