//! Privacy helpers for click analytics: IP truncation at collection time and
//! a scheduled retention sweep that anonymizes old per-visitor identifiers
//! (and the truncated address kept on organization audit entries).
//!
//! Visitor IPs are personal data (GDPR et al.). Analytics only need network /
//! city granularity, so we truncate before storage (IPv4 to /24, IPv6 to /48)
//! and null the remaining identifier columns after a retention window while
//! keeping the aggregate dimensions (country, city, device, browser, referer).

use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use std::net::{IpAddr, Ipv6Addr};

use super::account_purge::{
    account_purge_days, purge_expired_accounts, purge_expired_deleted_links,
};

/// Default retention for per-visitor identifier columns (`ip_address`,
/// `user_agent`) on click events: ~13 months, long enough for year-over-year
/// analytics comparisons.
pub const DEFAULT_PII_RETENTION_DAYS: i32 = 396;

/// Truncate an IP address for storage: IPv4 keeps the /24 (last octet
/// zeroed), IPv6 keeps the /48. An IPv4-mapped IPv6 address
/// (`::ffff:203.0.113.9`, what a dual-stack proxy reports) is an IPv4 client
/// and is truncated as one; as IPv6 its /48 would be all zeros. GeoIP lookup
/// must happen on the full address *before* truncation. Unparsable input is
/// dropped entirely rather than stored raw.
pub fn anonymize_ip(ip_str: &str) -> Option<String> {
    match ip_str.trim().parse::<IpAddr>().map(|ip| ip.to_canonical()) {
        Ok(IpAddr::V4(v4)) => {
            let o = v4.octets();
            Some(format!("{}.{}.{}.0", o[0], o[1], o[2]))
        }
        Ok(IpAddr::V6(v6)) => {
            let s = v6.segments();
            Some(IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], 0, 0, 0, 0, 0)).to_string())
        }
        Err(_) => None,
    }
}

/// Reduce a `Referer` header to just its host before storage. The full referring
/// URL can carry personal data in its path/query (search terms, session IDs,
/// tokens), which analytics does not need and we should not retain. Returns e.g.
/// `example.com` for `https://example.com/path?q=secret`; hostless or unparseable
/// input is dropped.
pub fn anonymize_referer(referer: &str) -> Option<String> {
    url::Url::parse(referer.trim())
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
}

/// Retention window in days, from `ANALYTICS_PII_RETENTION_DAYS`.
/// `0` disables the sweep; unset or invalid falls back to the default.
pub fn pii_retention_days() -> Option<i32> {
    match std::env::var("ANALYTICS_PII_RETENTION_DAYS") {
        Ok(v) => match v.trim().parse::<i32>() {
            Ok(0) => None,
            Ok(n) if n > 0 => Some(n),
            _ => Some(DEFAULT_PII_RETENTION_DAYS),
        },
        Err(_) => Some(DEFAULT_PII_RETENTION_DAYS),
    }
}

/// Null `ip_address` and `user_agent` on click events older than `days`.
/// Aggregate columns (country, city, region, device, browser, os, referer,
/// coordinates) are kept so historical analytics stay useful.
pub async fn scrub_expired_click_pii(
    db: &DatabaseConnection,
    days: i32,
) -> Result<u64, sea_orm::DbErr> {
    let res = db
        .execute(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            "UPDATE click_events SET ip_address = NULL, user_agent = NULL \
             WHERE created_at < NOW() - make_interval(days => $1) \
               AND (ip_address IS NOT NULL OR user_agent IS NOT NULL)",
            [days.into()],
        ))
        .await?;
    Ok(res.rows_affected())
}

/// Null `ip_address` on organization audit-log entries older than `days`, the
/// same window as click events. The entry itself (who did what, when) stays.
pub async fn scrub_expired_audit_ips(
    db: &DatabaseConnection,
    days: i32,
) -> Result<u64, sea_orm::DbErr> {
    let res = db
        .execute(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            "UPDATE audit_log SET ip_address = NULL \
             WHERE created_at < NOW() - make_interval(days => $1) \
               AND ip_address IS NOT NULL",
            [days.into()],
        ))
        .await?;
    Ok(res.rows_affected())
}

/// Erase visitor PII (IP, user-agent, referer) from every click event on a
/// user's personal links. Called on account deletion so a departing user's link
/// analytics stop retaining per-visitor identifiers immediately, rather than
/// waiting out the retention window. Aggregate dimensions (country, city,
/// device, browser, …) are kept so historical counts still work.
pub async fn purge_click_pii_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i32,
) -> Result<u64, sea_orm::DbErr> {
    let res = db
        .execute(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            "UPDATE click_events SET ip_address = NULL, user_agent = NULL, referer = NULL \
             WHERE link_id IN (SELECT id FROM links WHERE user_id = $1 AND org_id IS NULL)",
            [user_id.into()],
        ))
        .await?;
    Ok(res.rows_affected())
}

/// Spawn the daily retention sweeps: click and audit identifiers, and the
/// final removal of accounts deleted past their grace window. First run
/// happens at startup so a long gap between deploys can't accumulate
/// over-retained data.
pub fn spawn_retention_task(db: DatabaseConnection) {
    let pii_days = pii_retention_days();
    let purge_days = account_purge_days();

    match pii_days {
        Some(days) => tracing::info!(
            "Analytics PII retention sweep enabled: anonymizing click identifiers older than {} days",
            days
        ),
        None => {
            tracing::info!(
                "Analytics PII retention sweep disabled (ANALYTICS_PII_RETENTION_DAYS=0)"
            )
        }
    }
    match purge_days {
        Some(days) => tracing::info!(
            "Account purge enabled: removing accounts and links deleted more than {} days ago",
            days
        ),
        None => tracing::info!(
            "Account purge disabled (ACCOUNT_PURGE_DAYS=0): deleted accounts and links stay until an admin removes them"
        ),
    }
    if pii_days.is_none() && purge_days.is_none() {
        return;
    }

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            if let Some(days) = pii_days {
                match scrub_expired_click_pii(&db, days).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(
                        "Analytics PII retention sweep anonymized {} click events (older than {} days)",
                        n,
                        days
                    ),
                    Err(e) => tracing::error!("Analytics PII retention sweep failed: {}", e),
                }
                match scrub_expired_audit_ips(&db, days).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(
                        "Retention sweep removed the IP address from {} audit log entries (older than {} days)",
                        n,
                        days
                    ),
                    Err(e) => tracing::error!("Audit log IP retention sweep failed: {}", e),
                }
            }
            if let Some(days) = purge_days {
                match purge_expired_accounts(&db, days).await {
                    Ok(report) => {
                        if report.purged > 0 {
                            tracing::info!(
                                "Account purge removed {} accounts deleted more than {} days ago",
                                report.purged,
                                days
                            );
                        }
                        if report.blocked > 0 {
                            tracing::warn!(
                                "Account purge kept {} deleted accounts that still own organizations with other members",
                                report.blocked
                            );
                        }
                        if report.failed > 0 {
                            tracing::warn!("Account purge failed for {} accounts", report.failed);
                        }
                    }
                    Err(e) => tracing::error!("Account purge failed: {}", e),
                }
                match purge_expired_deleted_links(&db, days).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(
                        "Deleted-link purge removed {} links deleted more than {} days ago",
                        n,
                        days
                    ),
                    Err(e) => tracing::error!("Deleted-link purge failed: {}", e),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_truncates_last_octet() {
        assert_eq!(
            anonymize_ip("203.0.113.195").as_deref(),
            Some("203.0.113.0")
        );
        assert_eq!(anonymize_ip("10.1.2.3").as_deref(), Some("10.1.2.0"));
    }

    #[test]
    fn ipv6_truncates_to_48() {
        assert_eq!(
            anonymize_ip("2001:db8:85a3:8d3:1319:8a2e:370:7348").as_deref(),
            Some("2001:db8:85a3::")
        );
    }

    #[test]
    fn ipv4_mapped_ipv6_truncates_like_ipv4() {
        assert_eq!(
            anonymize_ip("::ffff:203.0.113.77").as_deref(),
            Some("203.0.113.0")
        );
        // The hex spelling of the same address.
        assert_eq!(
            anonymize_ip("::ffff:cb00:714d").as_deref(),
            Some("203.0.113.0")
        );
    }

    #[test]
    fn ipv6_loopback_is_not_mistaken_for_ipv4() {
        assert_eq!(anonymize_ip("::1").as_deref(), Some("::"));
    }

    #[test]
    fn garbage_is_dropped_not_stored() {
        assert_eq!(anonymize_ip("not-an-ip"), None);
        assert_eq!(anonymize_ip(""), None);
    }

    #[test]
    fn already_truncated_is_stable() {
        assert_eq!(anonymize_ip("203.0.113.0").as_deref(), Some("203.0.113.0"));
    }

    #[test]
    fn referer_reduced_to_host_only() {
        // Path and query (potential PII) are dropped; only the host is kept.
        assert_eq!(
            anonymize_referer("https://example.com/search?q=secret+terms").as_deref(),
            Some("example.com")
        );
        // Host is normalized to lowercase.
        assert_eq!(
            anonymize_referer("http://Sub.Example.COM/a/b").as_deref(),
            Some("sub.example.com")
        );
        // Non-URL / hostless input is dropped rather than stored.
        assert_eq!(anonymize_referer("not a url"), None);
        assert_eq!(anonymize_referer(""), None);
    }
}
