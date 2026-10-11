use std::collections::HashMap;

use sea_orm::sea_query::{Expr, Func};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QueryResult, Select, Statement,
};

use crate::entity::{blocked_email_domains, users};
use crate::utils::url_policy::{
    canonical_host, domain_matches, is_reserved_hostname, normalize_hostname,
};

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

/// Lowercase ASCII form of an email domain: IDNA (punycode) for Unicode
/// labels, trailing dot removed. Anything that is not a plain domain name keeps
/// the old plain-lowercase treatment.
fn canonical_domain(domain: &str) -> String {
    let domain = domain.trim().trim_end_matches('.');
    // `Host::parse` percent-decodes, which an email domain never does.
    if !domain.contains('%')
        && let Ok(url::Host::Domain(ascii)) = url::Host::parse(domain)
    {
        return ascii;
    }
    normalize_hostname(domain).unwrap_or_default()
}

/// Trimmed address with the domain canonicalised and the local part left as typed.
fn trim_email(email: &str) -> String {
    let trimmed = email.trim();
    if let Some((local, domain)) = trimmed.rsplit_once('@') {
        format!("{}@{}", local.trim(), canonical_domain(domain))
    } else {
        trimmed.to_string()
    }
}

/// The one form in which an address is stored and compared: trimmed, the
/// domain as lowercase punycode, everything lowercased. Mailboxes are
/// case-insensitive in practice, and `Bob@x.com` / `bob@x.com` (or
/// `x@bücher.de` / `x@xn--bcher-kva.de`) used to be separate accounts, which
/// locked the owner out of login, reset and invites whenever they typed the
/// other spelling.
pub fn normalize_email(email: &str) -> String {
    trim_email(email).to_ascii_lowercase()
}

/// Live accounts registered under `email`, compared case-insensitively
/// (`lower(email) = <normalised>`, served by the partial index on
/// `lower(email)`). Registration stores [`normalize_email`], but rows from
/// before that, or restored from a deletion, may still be mixed-case, so
/// lookups never rely on plain equality.
///
/// If several live accounts share the address (a collision the migration did
/// not merge), the one registered under exactly the spelling typed comes first,
/// then the oldest. `.one()` therefore picks a stable account, and callers add
/// their own filters (disabled, verified) on top. The spelling exactly as typed
/// also matches: a Unicode-domain address that stayed unfolded because it
/// collides is otherwise unreachable by its owner.
pub fn live_users_by_email(email: &str) -> Select<users::Entity> {
    let as_typed = email.trim().to_string();
    let typed = trim_email(email);
    users::Entity::find()
        .filter(
            Condition::any()
                .add(
                    Expr::expr(Func::lower(Expr::col(users::Column::Email)))
                        .eq(typed.to_ascii_lowercase()),
                )
                .add(users::Column::Email.eq(as_typed.clone())),
        )
        .filter(users::Column::DeletedAt.is_null())
        .order_by_desc(Expr::col(users::Column::Email).is_in([as_typed, typed]))
        .order_by_asc(users::Column::CreatedAt)
        .order_by_asc(users::Column::Id)
}

/// Number of addresses held by more than one live account, the spellings
/// differing only by letter case or, for a Unicode domain, by its encoding. A
/// count, not the addresses: it feeds the startup warning and the admin
/// overview, and neither should expose emails.
pub async fn count_email_case_collisions<C: ConnectionTrait>(db: &C) -> Result<u64, DbErr> {
    const ASCII: &str = "deleted_at IS NULL AND octet_length(email) = length(email)";
    let count = |row: Option<QueryResult>| -> Result<u64, DbErr> {
        Ok(match row {
            Some(row) => row.try_get_by_index::<i64>(0)?.max(0) as u64,
            None => 0,
        })
    };

    // Plain ASCII addresses: SQL can group them by lower().
    let mut groups = count(
        db.query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT COUNT(*)::bigint FROM ( \
                     SELECT 1 FROM users WHERE {ASCII} \
                     GROUP BY lower(email) HAVING COUNT(*) > 1 \
                 ) AS collisions"
            ),
        ))
        .await?,
    )?;

    // Unicode-domain addresses fold to punycode in Rust, and may collide with
    // each other or with an ASCII address. A key that already has two ASCII
    // accounts is counted above.
    let unicode = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            "SELECT email FROM users \
             WHERE deleted_at IS NULL AND octet_length(email) <> length(email)",
        ))
        .await?;
    let mut by_key: HashMap<String, u64> = HashMap::new();
    for row in unicode {
        *by_key
            .entry(normalize_email(&row.try_get_by_index::<String>(0)?))
            .or_default() += 1;
    }
    for (key, unicode_accounts) in by_key {
        let ascii_accounts = count(
            db.query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!("SELECT COUNT(*)::bigint FROM users WHERE {ASCII} AND lower(email) = $1"),
                [key.into()],
            ))
            .await?,
        )?;
        if ascii_accounts < 2 && ascii_accounts + unicode_accounts > 1 {
            groups += 1;
        }
    }
    Ok(groups)
}

/// The domain of an address in the spelling blocks are stored and compared in
/// (lower-case punycode), whichever way the address spells an internationalised
/// domain.
pub fn email_domain(email: &str) -> Option<String> {
    let (_, domain) = email.trim().rsplit_once('@')?;
    canonical_host(domain)
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
    let Some(domain) = canonical_host(domain) else {
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
    fn unicode_and_punycode_domains_normalise_to_one_address() {
        for spelling in [
            "x@bücher.de",
            "X@BÜCHER.de",
            "x@xn--bcher-kva.de",
            " X@Xn--Bcher-Kva.DE. ",
        ] {
            assert_eq!(
                normalize_email(spelling),
                "x@xn--bcher-kva.de",
                "{spelling:?}"
            );
        }
        // Not a domain name: kept as typed, lowercased, never percent-decoded.
        assert_eq!(normalize_email("a@Exa%6Dple.com"), "a@exa%6dple.com");
        assert_eq!(normalize_email("a@[::1]"), "a@[::1]");
    }

    #[test]
    fn email_domain_is_lower_case_punycode_whatever_the_spelling() {
        let punycode = Some("xn--bcher-kva.de".to_string());
        assert_eq!(email_domain("x@bücher.de"), punycode);
        assert_eq!(email_domain("x@BÜCHER.DE."), punycode);
        assert_eq!(email_domain("x@xn--bcher-kva.de"), punycode);
        assert_eq!(email_domain("x@XN--BCHER-KVA.DE"), punycode);
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
