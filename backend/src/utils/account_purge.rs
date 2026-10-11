//! Final removal of soft-deleted accounts.
//!
//! Deleting an account, by its owner or by an admin, only marks the row. The
//! email address, password hash, profile and the destinations, titles and notes
//! of its links stay in the database, which is what lets an admin restore it.
//! Left alone they stay forever, so a daily sweep removes every account that has
//! been soft-deleted for longer than the grace window (`ACCOUNT_PURGE_DAYS`,
//! default 30). It runs the same transaction as the admin hard delete
//! (`hard_delete_user_in`): org links move to the org owner, the codes of the
//! removed links are retired, click history, passkeys and API keys go with the
//! user. After that the account cannot be restored.

use chrono::{Duration, NaiveDateTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
};

use crate::entity::users;
use crate::handlers::admin::{HardDeleteError, hard_delete_user_in};

/// Default grace window between deleting an account and removing it for good.
pub const DEFAULT_ACCOUNT_PURGE_DAYS: i32 = 30;

/// Larger values are treated as this (100 years): effectively "never", and a
/// cutoff that is still a valid Postgres timestamp.
const MAX_ACCOUNT_PURGE_DAYS: i32 = 36_500;

/// Accounts fetched per query. Each account is removed in its own transaction,
/// so a backlog never holds one long transaction or one huge id list.
const BATCH_SIZE: u64 = 100;

/// Grace window in days, from `ACCOUNT_PURGE_DAYS`. `0` disables the purge
/// (deleted accounts then stay until an admin removes them); unset or invalid
/// falls back to the default.
pub fn account_purge_days() -> Option<i32> {
    parse_purge_days(std::env::var("ACCOUNT_PURGE_DAYS").ok().as_deref())
}

fn parse_purge_days(raw: Option<&str>) -> Option<i32> {
    match raw.map(|v| v.trim().parse::<i32>()) {
        Some(Ok(0)) => None,
        Some(Ok(n)) if n > 0 => Some(n.min(MAX_ACCOUNT_PURGE_DAYS)),
        _ => Some(DEFAULT_ACCOUNT_PURGE_DAYS),
    }
}

/// When an account deleted at `deleted_at` is removed for good, given the
/// grace window in `days`.
pub fn purge_due_at(deleted_at: NaiveDateTime, days: i32) -> Option<NaiveDateTime> {
    deleted_at.checked_add_signed(Duration::days(days.into()))
}

/// What one sweep did. Counts only: accounts are never named in logs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PurgeReport {
    /// Accounts removed.
    pub purged: u64,
    /// Accounts past the window that were kept because they still own an
    /// organization with other live members. Hard delete refuses those too;
    /// ownership has to move first.
    pub blocked: u64,
    /// Accounts whose removal failed with a database error.
    pub failed: u64,
}

enum Outcome {
    Purged,
    Blocked,
    /// Restored, or already removed, since it was selected.
    Gone,
}

/// Remove every account soft-deleted more than `days` days ago.
pub async fn purge_expired_accounts(
    db: &DatabaseConnection,
    days: i32,
) -> Result<PurgeReport, DbErr> {
    let Some(cutoff) = Utc::now()
        .naive_utc()
        .checked_sub_signed(Duration::days(days.into()))
    else {
        return Ok(PurgeReport::default());
    };

    let mut report = PurgeReport::default();
    // Keyset paging: an account that is kept (blocked, or failed) is selected
    // again by the next sweep, never by the next page of this one.
    let mut after_id = 0;
    loop {
        let batch: Vec<i32> = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .filter(users::Column::DeletedAt.lt(cutoff))
            .filter(users::Column::Id.gt(after_id))
            .order_by_asc(users::Column::Id)
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;
        let Some(&last_id) = batch.last() else {
            break;
        };
        after_id = last_id;

        for user_id in &batch {
            match purge_account(db, *user_id, cutoff).await {
                Ok(Outcome::Purged) => report.purged += 1,
                Ok(Outcome::Blocked) => report.blocked += 1,
                Ok(Outcome::Gone) => {}
                Err(e) => {
                    tracing::error!("Account purge failed for one account: {}", e);
                    report.failed += 1;
                }
            }
        }

        if (batch.len() as u64) < BATCH_SIZE {
            break;
        }
    }
    Ok(report)
}

async fn purge_account(
    db: &DatabaseConnection,
    user_id: i32,
    cutoff: NaiveDateTime,
) -> Result<Outcome, DbErr> {
    let txn = db.begin().await?;

    // Re-check under the row lock: an admin restore that ran since the account
    // was selected has either finished (the account is live again) or waits for
    // this transaction and then finds the row gone.
    let still_due = users::Entity::find_by_id(user_id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .and_then(|user| user.deleted_at)
        .is_some_and(|deleted_at| deleted_at < cutoff);
    if !still_due {
        txn.rollback().await?;
        return Ok(Outcome::Gone);
    }

    match hard_delete_user_in(&txn, user_id).await {
        Ok(()) => {
            txn.commit().await?;
            Ok(Outcome::Purged)
        }
        Err(HardDeleteError::NotFound) => {
            txn.rollback().await?;
            Ok(Outcome::Gone)
        }
        Err(HardDeleteError::OwnsOrganizations(_)) => {
            txn.rollback().await?;
            Ok(Outcome::Blocked)
        }
        Err(HardDeleteError::Db(e) | HardDeleteError::Conflict(e)) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_env::{remove_env, set_env};
    use chrono::NaiveDate;

    #[test]
    fn window_defaults_to_thirty_days() {
        assert_eq!(parse_purge_days(None), Some(30));
        assert_eq!(DEFAULT_ACCOUNT_PURGE_DAYS, 30);
    }

    #[test]
    fn window_reads_a_positive_number_of_days() {
        assert_eq!(parse_purge_days(Some("7")), Some(7));
        assert_eq!(parse_purge_days(Some(" 90 ")), Some(90));
    }

    #[test]
    fn zero_disables_the_purge() {
        assert_eq!(parse_purge_days(Some("0")), None);
    }

    #[test]
    fn invalid_values_fall_back_to_the_default() {
        for raw in ["", "  ", "soon", "-5", "1.5", "30 days"] {
            assert_eq!(parse_purge_days(Some(raw)), Some(30), "{raw:?}");
        }
    }

    #[test]
    fn absurdly_long_windows_are_capped() {
        assert_eq!(parse_purge_days(Some("2147483647")), Some(36_500));
    }

    #[test]
    fn window_follows_the_environment() {
        set_env("ACCOUNT_PURGE_DAYS", "14");
        assert_eq!(account_purge_days(), Some(14));
        set_env("ACCOUNT_PURGE_DAYS", "0");
        assert_eq!(account_purge_days(), None);
        remove_env("ACCOUNT_PURGE_DAYS");
        assert_eq!(account_purge_days(), Some(30));
    }

    #[test]
    fn due_date_is_the_deletion_plus_the_window() {
        let deleted = NaiveDate::from_ymd_opt(2026, 10, 11)
            .unwrap()
            .and_hms_opt(8, 30, 0)
            .unwrap();
        assert_eq!(
            purge_due_at(deleted, 30),
            NaiveDate::from_ymd_opt(2026, 11, 10)
                .unwrap()
                .and_hms_opt(8, 30, 0)
        );
    }
}
