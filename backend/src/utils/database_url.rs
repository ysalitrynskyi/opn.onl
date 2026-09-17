//! Assemble `DATABASE_URL` from discrete Postgres pieces.
//!
//! Compose interpolates `${POSTGRES_PASSWORD}` into a URL verbatim. A password
//! containing `@`, `:`, `/`, or `#` then splits the URL (userinfo vs host), so
//! the backend never reaches Postgres even though the server accepted the
//! password. Percent-encode userinfo here instead.

/// Wrap an IPv6 literal so `url::Url` / libpq see a host rather than a port.
/// Hostnames and already-bracketed values are returned unchanged.
fn bracket_ipv6_host(host: &str) -> String {
    if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

/// Build a postgres URL with RFC 3986 percent-encoded user and password.
pub fn assemble_database_url(
    user: &str,
    password: &str,
    host: &str,
    port: &str,
    db: &str,
) -> String {
    format!(
        "postgres://{}:{}@{}:{}/{}",
        urlencoding::encode(user),
        urlencoding::encode(password),
        bracket_ipv6_host(host),
        port,
        db
    )
}

/// `DATABASE_URL` if set and non-empty; otherwise assemble from `POSTGRES_*`.
///
/// `POSTGRES_HOST` defaults to `localhost` (cargo run); compose must pass
/// `POSTGRES_HOST=db`. Panics if neither `DATABASE_URL` nor `POSTGRES_PASSWORD`
/// is set — same fail-fast as the old `DATABASE_URL must be set` check.
pub fn resolve_database_url() -> String {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => {
            let password = std::env::var("POSTGRES_PASSWORD").unwrap_or_default();
            if password.is_empty() {
                panic!(
                    "DATABASE_URL must be set, or POSTGRES_PASSWORD (with optional POSTGRES_USER/HOST/PORT/DB)"
                );
            }
            assemble_database_url(
                &std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
                &password,
                &std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
                &std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string()),
                &std::env::var("POSTGRES_DB").unwrap_or_else(|_| "opn_onl".to_string()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::assemble_database_url;

    #[test]
    fn percent_encodes_password_so_host_stays_intact() {
        let url = assemble_database_url("postgres", "p@ss/w:rd", "db", "5432", "opn_onl");
        let parsed = url::Url::parse(&url).expect("assembled DATABASE_URL must parse");
        assert_eq!(parsed.username(), "postgres");
        assert_eq!(
            parsed.host_str(),
            Some("db"),
            "unencoded @ would steal the host"
        );
        assert_eq!(parsed.port(), Some(5432));
        assert_eq!(parsed.path(), "/opn_onl");
        assert!(
            url.contains("%40"),
            "@ in the password must be percent-encoded"
        );
        assert!(
            !url.contains("p@ss"),
            "raw password must not appear in the URL"
        );
    }

    #[test]
    fn ipv6_host_is_wrapped_in_brackets() {
        for host in ["::1", "2001:db8::1"] {
            let url = assemble_database_url("postgres", "p@ss", host, "5432", "opn_onl");
            let parsed = url::Url::parse(&url).unwrap_or_else(|e| {
                panic!("IPv6 DATABASE_URL must parse (host={host}, url={url}): {e}")
            });
            let bracketed = format!("[{host}]");
            assert_eq!(
                parsed.host_str(),
                Some(bracketed.as_str()),
                "host={host} url={url}"
            );
            assert_eq!(parsed.port(), Some(5432), "host={host} url={url}");
        }

        let already = assemble_database_url("postgres", "p@ss", "[::1]", "5432", "opn_onl");
        let parsed = url::Url::parse(&already).expect("pre-bracketed IPv6 must still parse");
        assert_eq!(parsed.host_str(), Some("[::1]"));
        assert_eq!(parsed.port(), Some(5432));
    }
}
