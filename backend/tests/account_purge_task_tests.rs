//! The daily retention task removes expired accounts on its first run. Alone in
//! its own test binary: any other test sweeping the same database could remove
//! the account first and hide a task that never calls the purge.

mod common;

use chrono::{Duration, Utc};
use common::{setup_test_db, unique_email};
use opn_onl_backend::entity::users;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

#[tokio::test]
async fn the_daily_retention_task_purges_expired_accounts_when_it_starts() {
    let db = setup_test_db().await;
    let insert = |deleted_days_ago: Option<i64>| {
        users::ActiveModel {
            email: Set(unique_email()),
            password_hash: Set("not-a-real-hash".to_string()),
            created_at: Set(Utc::now().naive_utc()),
            email_verified: Set(true),
            is_admin: Set(false),
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
            users::Entity::find_by_id(id)
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
        "the retention task never purged the expired account"
    );
    assert!(exists(recent).await, "an account inside the window stays");
    assert!(exists(live).await, "a live account stays");
}
