//! Content-safety predicates for destination URLs.
//!
//! These are pure detectors — they do NOT read configuration. Callers decide
//! whether to *enforce* them:
//!   * link creation enforces them behind `BLOCK_DANGEROUS_FILE_EXTENSIONS` /
//!     `BLOCK_RAW_IP_URLS` (see `handlers::links`);
//!   * the admin panel uses them purely to *flag* existing links as suspicious,
//!     regardless of whether the guard was on when the link was created.
//!
//! Keeping them here (config-free) means "what counts as dangerous" has one
//! definition shared by the guard and the flagger.

/// Executable / script / installer / disk-image extensions that have no
/// legitimate reason to sit behind a public short link — they exist to deliver
/// a payload the moment they are opened. `.hta` in particular runs with full
/// trust via `mshta.exe`.
const DANGEROUS_EXTENSIONS: &[&str] = &[
    // Windows executables & script hosts
    "hta", "exe", "scr", "bat", "cmd", "com", "pif", "cpl", "dll", "msc", "ps1", "psm1", "vbs",
    "vbe", "js", "jse", "wsf", "wsh", "sct", "gadget", // Installers / packages
    "msi", "msp", "msix", "appx", "jar", "jnlp", "reg", "inf", "lnk", "apk", "deb", "rpm", "run",
    // Disk images that commonly wrap a payload
    "dmg", "iso",
];

/// The dangerous-extension list, for callers that need to build a query-side
/// filter (e.g. the admin "suspicious links" SQL predicate) from the same
/// source of truth as [`dangerous_extension`].
pub fn dangerous_extensions() -> &'static [&'static str] {
    DANGEROUS_EXTENSIONS
}

/// If the URL's path points at a file with a dangerous extension, return that
/// extension (lowercased, without the dot). The query string is ignored — a
/// lure like `...goodbrainthings.hta?id=news-headline` is still an `.hta`.
pub fn dangerous_extension(url: &str) -> Option<&'static str> {
    let path = match url::Url::parse(url) {
        Ok(u) => u.path().to_string(),
        // Fall back to the raw string so a not-quite-parseable URL is still
        // screened; strip any query/fragment ourselves.
        Err(_) => url.split(['?', '#']).next().unwrap_or(url).to_string(),
    };

    // Percent-decode so `%2ehta` / `foo%2Ehta` can't smuggle the extension past.
    let decoded = urlencoding::decode(&path)
        .map(|c| c.into_owned())
        .unwrap_or(path);

    let last_segment = decoded.rsplit('/').next().unwrap_or("").trim();
    let ext = last_segment.rsplit_once('.')?.1.to_ascii_lowercase();
    DANGEROUS_EXTENSIONS.iter().copied().find(|&d| d == ext)
}

/// True when the URL's host is a bare IP literal (IPv4 or IPv6) rather than a
/// domain name. Legitimate links almost always use a domain; a raw IP host is a
/// strong abuse signal (reputation-free, TLS-less payload hosting).
pub fn host_is_raw_ip(url: &str) -> bool {
    matches!(
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.host().map(|h| h.to_owned())),
        Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_))
    )
}

pub fn normalize_hostname(host: &str) -> Option<String> {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        None
    } else {
        Some(h)
    }
}

pub fn normalize_domain_input(input: &str) -> Option<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }

    let candidate = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };

    url::Url::parse(&candidate)
        .ok()
        .and_then(|url| url.host_str().map(normalize_hostname))
        .flatten()
}

pub fn domain_matches(blocked_domain: &str, host: &str) -> bool {
    let Some(blocked) = normalize_hostname(blocked_domain) else {
        return false;
    };
    let Some(host) = normalize_hostname(host) else {
        return false;
    };

    host == blocked || host.ends_with(&format!(".{blocked}"))
}

pub fn is_reserved_hostname(host: &str) -> bool {
    let Some(h) = normalize_hostname(host) else {
        return true;
    };

    domain_matches("example.com", &h)
        || domain_matches("example.net", &h)
        || domain_matches("example.org", &h)
        || h.ends_with(".example")
        || h.ends_with(".test")
        || h.ends_with(".invalid")
}

/// Hostnames that must never be accepted as user-supplied http(s) destinations
/// (profile website / avatar, and optionally short-link targets). Blocking by
/// name covers the cases raw-IP checks miss (`localhost`, `.local`, cloud
/// metadata DNS aliases).
pub fn is_disallowed_hostname(host: &str) -> bool {
    let Some(h) = normalize_hostname(host) else {
        return true;
    };

    is_reserved_hostname(&h)
        || h == "localhost"
        || h == "metadata.google.internal"
        || h == "metadata"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h.ends_with(".internal")
        || h.ends_with(".lan")
}

/// Require a parseable `http`/`https` URL with a real host that is not an
/// obvious local/internal name. Used for profile website / avatar fields where
/// `Url::parse` alone wrongly accepts `javascript:` and `data:`.
pub fn validate_http_https_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "Invalid URL format".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("URL must use http or https protocol".to_string());
    }
    let Some(host) = parsed.host_str() else {
        return Err("URL must have a valid host".to_string());
    };
    if is_disallowed_hostname(host) {
        return Err("Links to local/internal hosts are not allowed".to_string());
    }
    if host_is_raw_ip(url) {
        return Err("Links to raw IP addresses are not allowed".to_string());
    }
    Ok(())
}

/// One-line, human-readable reason a URL looks like abuse, or `None` if it does
/// not trip any detector. Used by the admin panel to label links.
pub fn suspicion_reason(url: &str) -> Option<String> {
    let mut reasons = Vec::new();
    if let Some(ext) = dangerous_extension(url) {
        reasons.push(format!("dangerous file type (.{ext})"));
    }
    if host_is_raw_ip(url) {
        reasons.push("raw IP host".to_string());
    }
    if url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(is_reserved_hostname))
        .unwrap_or(false)
    {
        reasons.push("reserved/test host".to_string());
    }
    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_hta_with_news_lure_query() {
        let u = "http://107.173.227.83/31/goodbrainthings.hta?id=foxbusiness.com/media/x";
        assert_eq!(dangerous_extension(u), Some("hta"));
        assert!(host_is_raw_ip(u));
        let reason = suspicion_reason(u).unwrap();
        assert!(reason.contains("hta"));
        assert!(reason.contains("raw IP"));
    }

    #[test]
    fn flags_percent_encoded_extension() {
        assert_eq!(dangerous_extension("http://evil.test/a%2Ehta"), Some("hta"));
    }

    #[test]
    fn ignores_benign_domain_link() {
        let u = "https://www.rona.ca/en/product/lattice-trp3672br";
        assert_eq!(dangerous_extension(u), None);
        assert!(!host_is_raw_ip(u));
        assert!(suspicion_reason(u).is_none());
    }

    #[test]
    fn benign_extension_is_allowed() {
        assert_eq!(dangerous_extension("https://iana.org/report.pdf"), None);
        assert_eq!(dangerous_extension("https://iana.org/photo.jpg"), None);
    }

    #[test]
    fn raw_ipv6_host_is_flagged() {
        assert!(host_is_raw_ip("http://[2001:db8::1]/payload"));
    }

    #[test]
    fn domain_that_contains_digits_is_not_raw_ip() {
        assert!(!host_is_raw_ip("https://3m.com/product"));
        assert!(!host_is_raw_ip("https://192-168-1-1.example.com/x"));
    }

    #[test]
    fn rejects_javascript_and_data_schemes() {
        assert!(validate_http_https_url("javascript:alert(1)").is_err());
        assert!(validate_http_https_url("data:text/html,hi").is_err());
        assert!(validate_http_https_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn rejects_localhost_and_metadata_hosts() {
        assert!(validate_http_https_url("http://localhost/x").is_err());
        assert!(validate_http_https_url("https://foo.localhost/x").is_err());
        assert!(validate_http_https_url("http://metadata.google.internal/").is_err());
        assert!(validate_http_https_url("http://127.0.0.1/").is_err());
    }

    #[test]
    fn rejects_reserved_hosts() {
        assert!(validate_http_https_url("https://example.com/").is_err());
        assert!(validate_http_https_url("https://foo.example/").is_err());
        assert!(validate_http_https_url("https://foo.test/").is_err());
        assert!(validate_http_https_url("https://foo.invalid/").is_err());
    }

    #[test]
    fn accepts_normal_https() {
        assert!(validate_http_https_url("https://iana.org/me").is_ok());
    }
}
