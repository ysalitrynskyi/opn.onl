use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};

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

pub fn normalize_email(email: &str) -> String {
    let trimmed = email.trim();
    if let Some((local, domain)) = trimmed.rsplit_once('@') {
        let local = local.trim();
        let domain = normalize_hostname(domain).unwrap_or_default();
        format!("{local}@{domain}")
    } else {
        trimmed.to_ascii_lowercase()
    }
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

/// `ensure_email_domain_allowed` for the paths that let an existing account
/// back in: login, password reset and passkey login.
///
/// Blocking a domain disables its non-admin accounts and leaves admins alone
/// (`disable_users_for_email_domain` in `handlers/admin.rs`), because an admin
/// has to stay able to unblock the domain. The same exemption applies here.
/// Without it an admin on the domain keeps working only until the session
/// token expires, and then can neither sign in, reset the password nor use a
/// passkey. Only the "blocked" verdict is waived; a reserved or malformed
/// domain still fails. Registration keeps the plain check.
pub async fn ensure_sign_in_domain_allowed<C: ConnectionTrait>(
    db: &C,
    email: &str,
) -> Result<(), EmailDomainRejection> {
    let verdict = ensure_email_domain_allowed(db, email).await;
    if matches!(verdict, Err(EmailDomainRejection::Blocked(_))) && is_live_admin(db, email).await {
        return Ok(());
    }
    verdict
}

/// A failed lookup counts as "not an admin": the block then stands.
async fn is_live_admin<C: ConnectionTrait>(db: &C, email: &str) -> bool {
    users::Entity::find()
        .filter(users::Column::Email.eq(email))
        .filter(users::Column::IsAdmin.eq(true))
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::DisabledAt.is_null())
        .one(db)
        .await
        .ok()
        .flatten()
        .is_some()
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
    fn normalizes_email_domain() {
        assert_eq!(
            normalize_email(" User@Example.COM. "),
            "User@example.com".to_string()
        );
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
