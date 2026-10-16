//! Abuse limits for outgoing mail.
//!
//! `register`, `forgot-password` and `resend-verification` send mail to an
//! address the caller names, so an anonymous caller can spend the instance's
//! mail on someone else's behalf. Three independent layers bound that:
//!
//! - **Per recipient** ([`TriggerLimiter`]): at most one mail of a kind per
//!   [`RECIPIENT_COOLDOWN`] and [`RECIPIENT_DAILY_CAP`] per
//!   [`RECIPIENT_DAILY_WINDOW`] to one address. Nobody can mail-bomb an inbox,
//!   or keep replacing the link that is already in it.
//! - **Per client** ([`TriggerLimiter`]): at most [`CLIENT_HOURLY_CAP`] mails
//!   triggered per client IP per hour, so one address cannot use up the
//!   instance-wide budget.
//! - **Per pool** ([`Pools`]): the instance-wide hourly budget
//!   (`EMAIL_RATE_LIMIT_PER_HOUR`) is split into an account pool, a security
//!   pool, a contact pool and a pool for scheduled notices, so a flood of one
//!   kind cannot starve the others. In particular the "your password was
//!   changed" notice, which may be the owner's only warning of a takeover, has
//!   a pool nobody else draws on, and the daily expiry reminders cannot use up
//!   the verification and reset mail of the hour they go out in.

use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Least time between two mails of one kind to one address.
pub const RECIPIENT_COOLDOWN: Duration = Duration::from_secs(5 * 60);
/// Most mails of one kind one address receives within [`RECIPIENT_DAILY_WINDOW`].
pub const RECIPIENT_DAILY_CAP: usize = 5;
pub const RECIPIENT_DAILY_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// Most mails one client IP can trigger per [`CLIENT_WINDOW`] through
/// registration, password reset and verification resend.
pub const CLIENT_HOURLY_CAP: usize = 20;
pub const CLIENT_WINDOW: Duration = Duration::from_secs(60 * 60);

const POOL_WINDOW: Duration = Duration::from_secs(60 * 60);
/// How often the per-recipient and per-client tables are swept for entries
/// that have aged out of their windows.
const SWEEP_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// What a mail is for. The kind picks the pool it is charged to, and
/// verification and reset mail are limited per recipient separately, so a
/// reset request does not eat into the verification allowance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MailKind {
    Verification,
    PasswordReset,
    Welcome,
    PasswordChanged,
    Contact,
    /// The daily reminder that links are about to expire. Sent by a
    /// background task, not a request, so only its pool limits it.
    LinkExpiry,
}

impl MailKind {
    pub fn label(self) -> &'static str {
        match self {
            MailKind::Verification => "verification",
            MailKind::PasswordReset => "password reset",
            MailKind::Welcome => "welcome",
            MailKind::PasswordChanged => "password changed",
            MailKind::Contact => "contact",
            MailKind::LinkExpiry => "link expiry reminder",
        }
    }

    fn pool(self) -> PoolKind {
        match self {
            MailKind::Verification | MailKind::PasswordReset | MailKind::Welcome => {
                PoolKind::Account
            }
            MailKind::PasswordChanged => PoolKind::Security,
            MailKind::Contact => PoolKind::Contact,
            MailKind::LinkExpiry => PoolKind::Notices,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PoolKind {
    Account,
    Security,
    Contact,
    Notices,
}

/// Why a mail may not be sent right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailDenied {
    /// The pool this kind of mail draws on is spent for the hour.
    PoolSpent { retry_after: Duration },
    /// A mail of this kind went to this address less than
    /// [`RECIPIENT_COOLDOWN`] ago.
    Cooldown { retry_after: Duration },
    /// This address already received [`RECIPIENT_DAILY_CAP`] mails of this kind.
    RecipientDaily { retry_after: Duration },
    /// This client already triggered [`CLIENT_HOURLY_CAP`] mails this hour.
    Client { retry_after: Duration },
}

impl MailDenied {
    pub fn retry_after(self) -> Duration {
        match self {
            MailDenied::PoolSpent { retry_after }
            | MailDenied::Cooldown { retry_after }
            | MailDenied::RecipientDaily { retry_after }
            | MailDenied::Client { retry_after } => retry_after,
        }
    }

    /// `Retry-After` value: whole seconds, rounded up, never zero.
    pub fn retry_after_secs(self) -> u64 {
        let wait = self.retry_after();
        (wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1)
    }

    /// Size of the budget that ran out (`X-RateLimit-Limit`).
    pub fn limit(self) -> u32 {
        match self {
            MailDenied::PoolSpent { .. } => 0,
            MailDenied::Cooldown { .. } => 1,
            MailDenied::RecipientDaily { .. } => RECIPIENT_DAILY_CAP as u32,
            MailDenied::Client { .. } => CLIENT_HOURLY_CAP as u32,
        }
    }

    /// For logs. Never names the recipient.
    pub fn reason(self) -> &'static str {
        match self {
            MailDenied::PoolSpent { .. } => "hourly mail pool is spent",
            MailDenied::Cooldown { .. } => "recipient cooldown",
            MailDenied::RecipientDaily { .. } => "recipient daily limit",
            MailDenied::Client { .. } => "client hourly limit",
        }
    }
}

/// One hourly budget: a fixed window that starts at the first use.
struct HourlyPool {
    limit: u64,
    window: Mutex<PoolWindow>,
}

struct PoolWindow {
    started: Instant,
    used: u64,
}

impl PoolWindow {
    /// Start a new window once the hour is over.
    fn roll(&mut self, now: Instant) {
        if now.saturating_duration_since(self.started) >= POOL_WINDOW {
            self.started = now;
            self.used = 0;
        }
    }
}

impl HourlyPool {
    fn new(limit: u64, now: Instant) -> Self {
        Self {
            limit,
            window: Mutex::new(PoolWindow {
                started: now,
                used: 0,
            }),
        }
    }

    /// Take one permit, or say when the window ends. `spend` false only asks
    /// whether a permit is left. Returns how many are used afterwards.
    fn acquire(&self, now: Instant, spend: bool) -> Result<u64, MailDenied> {
        let mut window = self.window.lock();
        window.roll(now);
        if window.used >= self.limit {
            return Err(MailDenied::PoolSpent {
                retry_after: POOL_WINDOW
                    .saturating_sub(now.saturating_duration_since(window.started)),
            });
        }
        if spend {
            window.used += 1;
        }
        Ok(window.used)
    }
}

/// The instance-wide hourly budget, split by what the mail is for.
pub struct Pools {
    account: HourlyPool,
    security: HourlyPool,
    contact: HourlyPool,
    notices: HourlyPool,
    total: u64,
}

impl Pools {
    /// Split `total` mails per hour: 10% for the contact form, 20% for
    /// security notices, 10% for scheduled notices (expiry reminders), the
    /// rest for account mail (verification, password reset, welcome). A total
    /// of at least one leaves every pool at least one mail, so very small
    /// settings still let each kind through.
    pub fn from_total(total: u64) -> Self {
        Self::from_total_at(total, Instant::now())
    }

    fn from_total_at(total: u64, now: Instant) -> Self {
        let share = |percent: u64| {
            if total == 0 {
                0
            } else {
                (total.saturating_mul(percent) / 100).max(1)
            }
        };
        let contact = share(10);
        let security = share(20);
        let notices = share(10);
        let account = if total == 0 {
            0
        } else {
            total.saturating_sub(contact + security + notices).max(1)
        };
        Self {
            account: HourlyPool::new(account, now),
            security: HourlyPool::new(security, now),
            contact: HourlyPool::new(contact, now),
            notices: HourlyPool::new(notices, now),
            total,
        }
    }

    fn pool(&self, kind: MailKind) -> &HourlyPool {
        match kind.pool() {
            PoolKind::Account => &self.account,
            PoolKind::Security => &self.security,
            PoolKind::Contact => &self.contact,
            PoolKind::Notices => &self.notices,
        }
    }

    /// The configured total (`EMAIL_RATE_LIMIT_PER_HOUR`).
    pub fn total(&self) -> u64 {
        self.total
    }

    /// `(account, security, contact, notices)` sizes, for the startup log.
    pub fn sizes(&self) -> (u64, u64, u64, u64) {
        (
            self.account.limit,
            self.security.limit,
            self.contact.limit,
            self.notices.limit,
        )
    }

    /// Take a permit from the pool `kind` draws on. Returns `(used, limit)`
    /// for that pool.
    pub fn try_acquire(&self, kind: MailKind) -> Result<(u64, u64), MailDenied> {
        self.try_acquire_at(kind, Instant::now())
    }

    fn try_acquire_at(&self, kind: MailKind, now: Instant) -> Result<(u64, u64), MailDenied> {
        let pool = self.pool(kind);
        pool.acquire(now, true).map(|used| (used, pool.limit))
    }

    /// Whether `kind` could be sent now, without taking a permit.
    pub fn check(&self, kind: MailKind) -> Result<(), MailDenied> {
        self.check_at(kind, Instant::now())
    }

    fn check_at(&self, kind: MailKind, now: Instant) -> Result<(), MailDenied> {
        self.pool(kind).acquire(now, false).map(|_| ())
    }
}

/// Send times, oldest first, within the longest window that still matters.
type SendTimes = VecDeque<Instant>;

fn forget_older_than(times: &mut SendTimes, now: Instant, window: Duration) {
    while times
        .front()
        .is_some_and(|sent| now.saturating_duration_since(*sent) >= window)
    {
        times.pop_front();
    }
}

/// Per-recipient and per-client limits for mail that an unauthenticated (or
/// merely logged-in) caller can trigger. State is in memory, like the other
/// limiters: a restart forgets it, and several instances do not share it.
pub struct TriggerLimiter {
    recipients: DashMap<(MailKind, String), SendTimes>,
    clients: DashMap<String, SendTimes>,
    last_sweep: Mutex<Instant>,
}

impl Default for TriggerLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerLimiter {
    pub fn new() -> Self {
        Self {
            recipients: DashMap::new(),
            clients: DashMap::new(),
            last_sweep: Mutex::new(Instant::now()),
        }
    }

    /// Ask to send one mail of `kind` to `recipient` on behalf of `client`. On
    /// `Ok` both allowances are spent. On `Err` nothing is: a refused request
    /// must not use up an address's cooldown, or anyone could lock a victim
    /// out of recovery mail by asking for it from an over-budget client.
    pub fn try_acquire(
        &self,
        kind: MailKind,
        recipient: &str,
        client: &str,
    ) -> Result<(), MailDenied> {
        self.try_acquire_at(Instant::now(), kind, recipient, client)
    }

    fn try_acquire_at(
        &self,
        now: Instant,
        kind: MailKind,
        recipient: &str,
        client: &str,
    ) -> Result<(), MailDenied> {
        self.sweep_if_due(now);

        // Both entries stay locked until the end, so the decision and the
        // bookkeeping are one step. The two tables are never locked in the
        // other order, so this cannot deadlock.
        let mut sent_to = self
            .recipients
            .entry((kind, recipient.trim().to_ascii_lowercase()))
            .or_default();
        forget_older_than(&mut sent_to, now, RECIPIENT_DAILY_WINDOW);
        if let Some(last) = sent_to.back() {
            let elapsed = now.saturating_duration_since(*last);
            if elapsed < RECIPIENT_COOLDOWN {
                return Err(MailDenied::Cooldown {
                    retry_after: RECIPIENT_COOLDOWN - elapsed,
                });
            }
        }
        if sent_to.len() >= RECIPIENT_DAILY_CAP
            && let Some(oldest) = sent_to.front()
        {
            return Err(MailDenied::RecipientDaily {
                retry_after: RECIPIENT_DAILY_WINDOW
                    .saturating_sub(now.saturating_duration_since(*oldest)),
            });
        }

        let mut triggered = self.clients.entry(client.to_string()).or_default();
        forget_older_than(&mut triggered, now, CLIENT_WINDOW);
        if triggered.len() >= CLIENT_HOURLY_CAP
            && let Some(oldest) = triggered.front()
        {
            return Err(MailDenied::Client {
                retry_after: CLIENT_WINDOW.saturating_sub(now.saturating_duration_since(*oldest)),
            });
        }

        sent_to.push_back(now);
        triggered.push_back(now);
        Ok(())
    }

    /// Drop entries whose newest send has left its window. Runs from
    /// `try_acquire`, at most once per [`SWEEP_INTERVAL`], so the tables stay
    /// bounded without a background task.
    fn sweep_if_due(&self, now: Instant) {
        {
            let mut last = self.last_sweep.lock();
            if now.saturating_duration_since(*last) < SWEEP_INTERVAL {
                return;
            }
            *last = now;
        }
        self.recipients.retain(|_, times| {
            times
                .back()
                .is_some_and(|sent| now.saturating_duration_since(*sent) < RECIPIENT_DAILY_WINDOW)
        });
        self.clients.retain(|_, times| {
            times
                .back()
                .is_some_and(|sent| now.saturating_duration_since(*sent) < CLIENT_WINDOW)
        });
    }
}

/// Where a mail address may appear in a log: the domain, never the mailbox.
pub fn loggable_domain(address: &str) -> &str {
    address
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim())
        .filter(|domain| !domain.is_empty())
        .unwrap_or("unknown domain")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: MailKind = MailKind::PasswordReset;

    fn later(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }

    #[test]
    fn a_second_mail_inside_the_cooldown_is_refused_with_the_time_left() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        assert_eq!(limiter.try_acquire_at(t0, KIND, "a@x.test", "c1"), Ok(()));

        let denied = limiter
            .try_acquire_at(later(t0, 60), KIND, "a@x.test", "c1")
            .unwrap_err();
        assert_eq!(
            denied,
            MailDenied::Cooldown {
                retry_after: Duration::from_secs(240)
            }
        );
        assert_eq!(denied.retry_after_secs(), 240);

        assert_eq!(
            limiter.try_acquire_at(later(t0, 300), KIND, "a@x.test", "c1"),
            Ok(()),
            "the cooldown ends after five minutes"
        );
    }

    #[test]
    fn the_cooldown_is_per_address_and_per_kind() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        limiter.try_acquire_at(t0, KIND, "a@x.test", "c1").unwrap();

        assert_eq!(
            limiter.try_acquire_at(t0, KIND, "b@x.test", "c1"),
            Ok(()),
            "another address is unaffected"
        );
        assert_eq!(
            limiter.try_acquire_at(t0, MailKind::Verification, "a@x.test", "c1"),
            Ok(()),
            "a reset mail does not use up the verification allowance"
        );
    }

    #[test]
    fn addresses_differing_only_in_case_share_one_allowance() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        limiter
            .try_acquire_at(t0, KIND, "Victim@X.test", "c1")
            .unwrap();
        assert!(matches!(
            limiter.try_acquire_at(t0, KIND, "victim@x.test ", "c1"),
            Err(MailDenied::Cooldown { .. })
        ));
    }

    #[test]
    fn an_address_gets_five_mails_a_day_then_waits_for_the_oldest_to_age_out() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        // Five mails, a cooldown apart (a different client each, so only the
        // per-recipient limit is in play).
        for n in 0..RECIPIENT_DAILY_CAP as u64 {
            limiter
                .try_acquire_at(later(t0, n * 301), KIND, "a@x.test", &format!("c{n}"))
                .unwrap();
        }
        let sixth = later(t0, RECIPIENT_DAILY_CAP as u64 * 301);
        let denied = limiter
            .try_acquire_at(sixth, KIND, "a@x.test", "c9")
            .unwrap_err();
        assert!(
            matches!(denied, MailDenied::RecipientDaily { .. }),
            "{denied:?}"
        );
        assert_eq!(
            denied.retry_after(),
            RECIPIENT_DAILY_WINDOW - Duration::from_secs(RECIPIENT_DAILY_CAP as u64 * 301)
        );

        assert_eq!(
            limiter.try_acquire_at(later(t0, 24 * 3600 + 1), KIND, "a@x.test", "c9"),
            Ok(()),
            "the first mail left the 24 hour window"
        );
    }

    #[test]
    fn a_client_triggers_twenty_mails_an_hour() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        for n in 0..CLIENT_HOURLY_CAP {
            limiter
                .try_acquire_at(t0, KIND, &format!("user{n}@x.test"), "1.2.3.4")
                .unwrap();
        }
        let denied = limiter
            .try_acquire_at(later(t0, 10), KIND, "one-more@x.test", "1.2.3.4")
            .unwrap_err();
        assert_eq!(
            denied,
            MailDenied::Client {
                retry_after: Duration::from_secs(3590)
            }
        );

        assert_eq!(
            limiter.try_acquire_at(later(t0, 10), KIND, "one-more@x.test", "5.6.7.8"),
            Ok(()),
            "another client has its own allowance"
        );
        assert_eq!(
            limiter.try_acquire_at(later(t0, 3600), KIND, "one-more@x.test", "1.2.3.4"),
            Ok(()),
            "the hour is over"
        );
    }

    /// A request the limiter refuses must not spend anything: otherwise a
    /// client past its own budget could put a victim's address into cooldown
    /// (and, five times a day, into the daily limit) without any mail being
    /// sent, and so keep the victim from ever getting a recovery link.
    #[test]
    fn a_refused_request_leaves_the_recipient_allowance_untouched() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        for n in 0..CLIENT_HOURLY_CAP {
            limiter
                .try_acquire_at(t0, KIND, &format!("user{n}@x.test"), "attacker")
                .unwrap();
        }
        assert!(matches!(
            limiter.try_acquire_at(t0, KIND, "victim@x.test", "attacker"),
            Err(MailDenied::Client { .. })
        ));

        assert_eq!(
            limiter.try_acquire_at(t0, KIND, "victim@x.test", "victim-client"),
            Ok(()),
            "the victim's own request is not in cooldown"
        );
    }

    #[test]
    fn stale_entries_are_swept() {
        let limiter = TriggerLimiter::new();
        let t0 = Instant::now();
        limiter
            .try_acquire_at(t0, KIND, "old@x.test", "old-client")
            .unwrap();
        assert_eq!(limiter.recipients.len(), 1);

        limiter
            .try_acquire_at(later(t0, 25 * 3600), KIND, "new@x.test", "new-client")
            .unwrap();
        assert_eq!(limiter.recipients.len(), 1, "only the new address is left");
        assert_eq!(limiter.clients.len(), 1);
    }

    #[test]
    fn the_total_is_split_between_account_security_contact_and_notice_mail() {
        assert_eq!(Pools::from_total(500).sizes(), (300, 100, 50, 50));
        assert_eq!(Pools::from_total(10).sizes(), (6, 2, 1, 1));
        // Small totals still leave every kind of mail a permit.
        assert_eq!(Pools::from_total(1).sizes(), (1, 1, 1, 1));
        assert_eq!(Pools::from_total(0).sizes(), (0, 0, 0, 0));
        // No overflow on an absurd setting.
        assert!(Pools::from_total(u64::MAX).sizes().0 > 0);
    }

    #[test]
    fn spending_one_pool_leaves_the_others() {
        let t0 = Instant::now();
        // account 6, security 2, contact 1, notices 1
        let pools = Pools::from_total_at(10, t0);
        for _ in 0..6 {
            pools.try_acquire_at(MailKind::Verification, t0).unwrap();
        }
        assert!(matches!(
            pools.try_acquire_at(MailKind::PasswordReset, t0),
            Err(MailDenied::PoolSpent { .. })
        ));
        assert!(matches!(
            pools.check_at(MailKind::Welcome, t0),
            Err(MailDenied::PoolSpent { .. })
        ));
        assert_eq!(
            pools.try_acquire_at(MailKind::PasswordChanged, t0),
            Ok((1, 2))
        );
        assert_eq!(pools.try_acquire_at(MailKind::Contact, t0), Ok((1, 1)));
        assert!(matches!(
            pools.try_acquire_at(MailKind::Contact, t0),
            Err(MailDenied::PoolSpent { .. })
        ));
        assert_eq!(pools.try_acquire_at(MailKind::LinkExpiry, t0), Ok((1, 1)));
        assert!(matches!(
            pools.check_at(MailKind::LinkExpiry, t0),
            Err(MailDenied::PoolSpent { .. })
        ));

        // A fresh hour refills every pool.
        assert!(
            pools
                .try_acquire_at(MailKind::Verification, later(t0, 3600))
                .is_ok()
        );
    }

    /// The reminders go out in one burst a day. However many there are, the
    /// verification and reset mail of that hour must still get through.
    #[test]
    fn expiry_reminders_cannot_spend_the_account_pool() {
        let t0 = Instant::now();
        let pools = Pools::from_total_at(500, t0);
        for _ in 0..50 {
            pools.try_acquire_at(MailKind::LinkExpiry, t0).unwrap();
        }
        assert!(matches!(
            pools.try_acquire_at(MailKind::LinkExpiry, t0),
            Err(MailDenied::PoolSpent { .. })
        ));
        assert_eq!(
            pools.try_acquire_at(MailKind::Verification, t0),
            Ok((1, 300))
        );
        assert_eq!(
            pools.try_acquire_at(MailKind::PasswordChanged, t0),
            Ok((1, 100))
        );
    }

    #[test]
    fn checking_a_pool_does_not_spend_it() {
        let t0 = Instant::now();
        let pools = Pools::from_total_at(10, t0);
        for _ in 0..50 {
            pools.check_at(MailKind::Contact, t0).unwrap();
        }
        assert_eq!(pools.try_acquire_at(MailKind::Contact, t0), Ok((1, 1)));
    }

    #[test]
    fn log_lines_name_the_domain_only() {
        assert_eq!(loggable_domain("alice@example.com"), "example.com");
        assert_eq!(loggable_domain("a@b@example.com"), "example.com");
        assert_eq!(loggable_domain("not-an-address"), "unknown domain");
        assert_eq!(loggable_domain("trailing@"), "unknown domain");
    }
}
