use sea_orm::{ConnectionTrait, DbErr, EntityTrait};

use crate::entity::blocked_email_domains;
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
