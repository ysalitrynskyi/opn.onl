//! Reminders before a link expires.
//!
//! Once a day, the owner of each personal link that expires within the next
//! [`NOTICE_WINDOW_DAYS`] days gets one email listing up to
//! [`MAX_LINKS_PER_NOTICE`] of them, soonest first. Every link the email
//! lists is then stamped (`links.expiry_notice_sent_at`) and is not listed
//! again; changing its expiry clears the stamp, so the new date gets a
//! reminder of its own. A link the email did not list, or a mail that was not
//! sent, leaves the stamp empty, and the next run picks the link up while it
//! is still due.
//!
//! What is selected:
//! - personal links only (`org_id IS NULL`): an organization link belongs to
//!   the team, and there is no single owner to remind;
//! - links that are live: not deleted, not used up, already started, and with
//!   a destination that is not on the blocklist;
//! - owners that are live (not deleted, not disabled), have reminders on
//!   (`users.notify_link_expiry`), and are verified when the instance requires
//!   verification.
//!
//! The mail draws on a pool of the hourly budget of its own
//! ([`MailKind::LinkExpiry`](super::email_limits::MailKind::LinkExpiry)). When
//! that pool is spent the run stops; the owners it did not reach are reminded
//! by the next run.

use std::sync::Arc;

use chrono::{Duration, NaiveDateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use super::email::{EmailService, ExpiringLink, NoticeError};
use super::email_limits::loggable_domain;
use super::env::env_flag;
use crate::entity::{links, users};
use crate::handlers::links::destination_is_blocklisted;

/// How far ahead a reminder looks.
pub const NOTICE_WINDOW_DAYS: i64 = 3;

/// Most links one reminder lists. The rest wait for the next run.
pub const MAX_LINKS_PER_NOTICE: usize = 20;

/// Owners fetched per query, and due links read per query for one owner.
const BATCH_SIZE: u64 = 100;

/// What one run did. Counts only: owners and links are never named in logs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReminderReport {
    /// Owners who were sent a reminder.
    pub sent: u64,
    /// Links those reminders listed and that were stamped.
    pub links: u64,
    /// Owners whose reminder could not be sent. Their links stay unstamped.
    pub failed: u64,
    /// The pool of the hourly budget the reminders draw on ran out, so the
    /// run stopped early.
    pub pool_spent: bool,
}

/// Whether this instance sends expiry reminders: `LINK_EXPIRY_REMINDERS` is
/// on (the default) and SMTP is configured. `/auth/settings` reports the same
/// answer, so Settings offers the switch only when it does something.
pub fn link_expiry_reminders_enabled(email: Option<&EmailService>) -> bool {
    env_flag("LINK_EXPIRY_REMINDERS", true) && email.is_some_and(EmailService::is_configured)
}

/// Links that are due a reminder at `now`, whoever owns them.
fn due_links(now: NaiveDateTime) -> Condition {
    Condition::all()
        .add(links::Column::OrgId.is_null())
        .add(links::Column::DeletedAt.is_null())
        .add(links::Column::BurnedAt.is_null())
        .add(links::Column::ExpiryNoticeSentAt.is_null())
        .add(links::Column::ExpiresAt.gt(now))
        .add(links::Column::ExpiresAt.lte(now + Duration::days(NOTICE_WINDOW_DAYS)))
}

/// Owners who may be sent a reminder.
fn remindable_owners(require_email_verification: bool) -> Condition {
    let owners = Condition::all()
        .add(users::Column::DeletedAt.is_null())
        .add(users::Column::DisabledAt.is_null())
        .add(users::Column::NotifyLinkExpiry.eq(true));
    if require_email_verification {
        owners.add(users::Column::EmailVerified.eq(true))
    } else {
        owners
    }
}

/// Send every reminder that is due and stamp the links they list.
///
/// Does nothing when `email` cannot send. `require_email_verification` is the
/// instance setting (`AppState::require_email_verification`): while it is on,
/// an owner whose address is not verified is not mailed.
pub async fn send_link_expiry_reminders(
    db: &DatabaseConnection,
    email: &EmailService,
    require_email_verification: bool,
) -> Result<ReminderReport, DbErr> {
    let mut report = ReminderReport::default();
    if !email.is_configured() {
        return Ok(report);
    }
    let now = Utc::now().naive_utc();

    // Keyset paging over owners: an owner whose mail failed is selected again
    // by the next run, never by the next page of this one.
    let mut after_id = 0;
    loop {
        let owners: Vec<(i32, String)> = links::Entity::find()
            .select_only()
            .column(links::Column::UserId)
            .column(users::Column::Email)
            .distinct()
            .inner_join(users::Entity)
            .filter(due_links(now))
            .filter(remindable_owners(require_email_verification))
            .filter(links::Column::UserId.gt(after_id))
            .order_by_asc(links::Column::UserId)
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;
        let Some(&(last_id, _)) = owners.last() else {
            break;
        };
        after_id = last_id;

        for (user_id, address) in &owners {
            let (listed, more) = links_to_list(db, *user_id, now).await?;
            let lines: Vec<ExpiringLink> = listed.iter().filter_map(expiring_link).collect();
            if lines.is_empty() {
                continue;
            }
            match email.send_link_expiry_email(address, &lines, more).await {
                Ok(()) => {
                    report.sent += 1;
                    report.links += mark_reminded(db, &listed).await?;
                }
                Err(NoticeError::PoolSpent) => {
                    report.pool_spent = true;
                    return Ok(report);
                }
                Err(NoticeError::Failed(e)) => {
                    tracing::error!(
                        "Failed to send a link expiry reminder to a {} address: {}",
                        loggable_domain(address),
                        e
                    );
                    report.failed += 1;
                }
            }
        }

        if (owners.len() as u64) < BATCH_SIZE {
            break;
        }
    }
    Ok(report)
}

/// The due links of `user_id` that one reminder lists (soonest first, at
/// most [`MAX_LINKS_PER_NOTICE`]), and whether more are due than it lists.
/// Used-up, not-yet-started and blocklisted links are left out.
async fn links_to_list(
    db: &DatabaseConnection,
    user_id: i32,
    now: NaiveDateTime,
) -> Result<(Vec<links::Model>, bool), DbErr> {
    let mut listed = Vec::new();
    let mut offset = 0;
    loop {
        let page = links::Entity::find()
            .filter(due_links(now))
            .filter(links::Column::UserId.eq(user_id))
            .order_by_asc(links::Column::ExpiresAt)
            .order_by_asc(links::Column::Id)
            .offset(offset)
            .limit(BATCH_SIZE)
            .all(db)
            .await?;
        for link in &page {
            if !link.is_active() || destination_is_blocklisted(db, &link.original_url).await? {
                continue;
            }
            if listed.len() == MAX_LINKS_PER_NOTICE {
                return Ok((listed, true));
            }
            listed.push(link.clone());
        }
        if (page.len() as u64) < BATCH_SIZE {
            return Ok((listed, false));
        }
        offset += BATCH_SIZE;
    }
}

/// How a link appears in the reminder: its title, or its destination when it
/// has none.
fn expiring_link(link: &links::Model) -> Option<ExpiringLink> {
    let label = link
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&link.original_url);
    Some(ExpiringLink {
        code: link.code.clone(),
        label: label.to_string(),
        expires_at: link.expires_at?,
    })
}

/// Stamp the links a sent reminder listed. A link whose expiry changed since
/// it was read is left alone: the reminder named the old date, and the new one
/// is due a reminder of its own.
async fn mark_reminded(db: &DatabaseConnection, listed: &[links::Model]) -> Result<u64, DbErr> {
    if listed.is_empty() {
        return Ok(0);
    }
    let as_listed = listed.iter().fold(Condition::any(), |any, link| {
        any.add(
            Condition::all()
                .add(links::Column::Id.eq(link.id))
                .add(links::Column::ExpiresAt.eq(link.expires_at)),
        )
    });
    let res = links::Entity::update_many()
        .col_expr(
            links::Column::ExpiryNoticeSentAt,
            Expr::value(Utc::now().naive_utc()),
        )
        .filter(links::Column::ExpiryNoticeSentAt.is_null())
        .filter(as_listed)
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

/// Spawn the daily reminder run. The first run happens at startup; the stamps
/// keep a restart from mailing anyone twice about the same date. Not started
/// when `LINK_EXPIRY_REMINDERS` is off or SMTP is not configured.
pub fn spawn_link_expiry_task(
    db: DatabaseConnection,
    email: Option<Arc<EmailService>>,
    require_email_verification: bool,
) {
    if !env_flag("LINK_EXPIRY_REMINDERS", true) {
        tracing::info!("Link expiry reminders disabled (LINK_EXPIRY_REMINDERS=false)");
        return;
    }
    let Some(email) = email.filter(|service| service.is_configured()) else {
        tracing::info!("Link expiry reminders disabled (SMTP not configured)");
        return;
    };
    tracing::info!(
        "Link expiry reminders enabled: owners are mailed once a day about links expiring within {} days",
        NOTICE_WINDOW_DAYS
    );

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            match send_link_expiry_reminders(&db, &email, require_email_verification).await {
                Ok(report) => {
                    if report.sent > 0 {
                        tracing::info!(
                            "Link expiry reminders sent to {} owners about {} links",
                            report.sent,
                            report.links
                        );
                    }
                    if report.failed > 0 {
                        tracing::warn!("Link expiry reminders failed for {} owners", report.failed);
                    }
                    if report.pool_spent {
                        tracing::warn!(
                            "Link expiry reminders stopped: their share of EMAIL_RATE_LIMIT_PER_HOUR is spent; the rest go out with the next run"
                        );
                    }
                }
                Err(e) => tracing::error!("Link expiry reminders failed: {}", e),
            }
        }
    });
}
