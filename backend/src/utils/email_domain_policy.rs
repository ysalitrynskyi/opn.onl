use sea_orm::sea_query::{Expr, Func};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, DbErr, EntityTrait, QueryFilter, QueryOrder, Select,
    Statement,
};

use crate::entity::{blocked_email_domains, users};
use crate::utils::url_policy::{domain_matches, is_reserved_hostname, normalize_hostname};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EmailDomainRejection {
    Invalid,
    Reserved,
    Blocked(String),
}

impl EmailDomainRejection {
    pub fn public_message(&self) -> &'static str {
        match self {
            Self::Invalid => "Invalid email domain",
            Self::Reserved => "This email domain cannot be used",
            Self::Blocked(_) => "This email domain is blocked",
        }
    }
}

/// Trimmed address with the domain normalised and the local part left as typed.
fn trim_email(email: &str) -> String {
    let trimmed = email.trim();
    if let Some((local, domain)) = trimmed.rsplit_once('@') {
        let local = local.trim();
        let domain = normalize_hostname(domain).unwrap_or_default();
        format!("{local}@{domain}")
    } else {
        trimmed.to_string()
    }
}

/// The one form in which an address is stored and compared: trimmed and
/// lowercased throughout. Mailboxes are case-insensitive in practice, and
/// `Bob@x.com` / `bob@x.com` used to be two accounts, which locked the owner out
/// of login, reset and invites whenever they typed the other spelling.
pub fn normalize_email(email: &str) -> String {
    trim_email(email).to_ascii_lowercase()
}

/// Live accounts registered under `email`, compared case-insensitively
/// (`lower(email) = <normalised>`, served by the partial index on
/// `lower(email)`). Registration stores [`normalize_email`], but rows from
/// before that, or restored from a deletion, may still be mixed-case, so
/// lookups never rely on plain equality.
///
/// If several live accounts share the address (a case collision the migration
/// did not merge), the one registered under exactly the spelling typed comes
/// first, then the oldest. `.one()` therefore picks a stable account, and
/// callers add their own filters (disabled, verified) on top.
pub fn live_users_by_email(email: &str) -> Select<users::Entity> {
    let typed = trim_email(email);
    users::Entity::find()
        .filter(
            Expr::expr(Func::lower(Expr::col(users::Column::Email))).eq(typed.to_ascii_lowercase()),
        )
        .filter(users::Column::DeletedAt.is_null())
        .order_by_desc(Expr::col(users::Column::Email).eq(typed))
        .order_by_asc(users::Column::CreatedAt)
        .order_by_asc(users::Column::Id)
}

/// Number of addresses held by more than one live account, the spellings
/// differing only by letter case. A count, not the addresses: it feeds the
/// startup warning and the admin overview, and neither should expose emails.
pub async fn count_email_case_collisions<C: ConnectionTrait>(db: &C) -> Result<u64, DbErr> {
    let row = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT COUNT(*)::bigint FROM ( \
                 SELECT 1 FROM users WHERE deleted_at IS NULL \
                 GROUP BY lower(email) HAVING COUNT(*) > 1 \
             ) AS collisions",
        ))
        .await?;
    Ok(match row {
        Some(row) => row.try_get_by_index::<i64>(0)?.max(0) as u64,
        None => 0,
    })
}

pub fn email_domain(email: &str) -> Option<String> {
    let (_, domain) = email.trim().rsplit_once('@')?;
    normalize_hostname(domain)
}

pub fn is_reserved_email_domain(domain: &str) -> bool {
    is_reserved_hostname(domain)
        || normalize_hostname(domain)
            .map(|domain| {
                domain == "localhost"
                    || domain.ends_with(".localhost")
                    || domain.ends_with(".local")
                    || domain.ends_with(".internal")
                    || domain.ends_with(".lan")
            })
            .unwrap_or(true)
}

pub async fn blocked_email_domain<C: ConnectionTrait>(
    db: &C,
    domain: &str,
) -> Result<Option<blocked_email_domains::Model>, DbErr> {
    let Some(domain) = normalize_hostname(domain) else {
        return Ok(None);
    };

    let blocked = blocked_email_domains::Entity::find().all(db).await?;
    Ok(blocked
        .into_iter()
        .find(|blocked| domain_matches(&blocked.domain, &domain)))
}

pub async fn ensure_email_domain_allowed<C: ConnectionTrait>(
    db: &C,
    email: &str,
) -> Result<(), EmailDomainRejection> {
    let domain = email_domain(email).ok_or(EmailDomainRejection::Invalid)?;

    if is_reserved_email_domain(&domain) {
        return Err(EmailDomainRejection::Reserved);
    }

    match blocked_email_domain(db, &domain).await {
        Ok(Some(blocked)) => Err(EmailDomainRejection::Blocked(blocked.domain)),
        Ok(None) => Ok(()),
        Err(_) => Err(EmailDomainRejection::Blocked(domain)),
    }
}

pub async fn user_email_domain_blocked<C: ConnectionTrait>(
    db: &C,
    email: &str,
) -> Result<bool, DbErr> {
    let Some(domain) = email_domain(email) else {
        return Ok(true);
    };
    Ok(is_reserved_email_domain(&domain) || blocked_email_domain(db, &domain).await?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_email_case_and_domain() {
        assert_eq!(
            normalize_email(" User@Example.COM. "),
            "user@example.com".to_string()
        );
        assert_eq!(normalize_email(" Bob.X@x.com "), "bob.x@x.com".to_string());
        assert_eq!(normalize_email(" NoAtSign "), "noatsign".to_string());
        assert_eq!(
            email_domain("User@Example.COM."),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn flags_reserved_email_domains() {
        assert!(is_reserved_email_domain("example.com"));
        assert!(is_reserved_email_domain("foo.test"));
        assert!(is_reserved_email_domain("foo.invalid"));
        assert!(is_reserved_email_domain("localhost"));
        assert!(!is_reserved_email_domain("opn.onl"));
    }
}
