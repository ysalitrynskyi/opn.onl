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
    "hta", "exe", "scr", "bat", "cmd", "com", "pif", "cpl", "dll", "msc",
    "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "sct", "gadget",
    // Installers / packages
    "msi", "msp", "msix", "appx", "jar", "jnlp", "reg", "inf", "lnk", "apk",
    "deb", "rpm", "run",
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
        Err(_) => url
            .split(['?', '#'])
            .next()
            .unwrap_or(url)
            .to_string(),
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
        url::Url::parse(url).ok().and_then(|u| u.host().map(|h| h.to_owned())),
        Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_))
    )
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
        assert_eq!(dangerous_extension("https://example.com/report.pdf"), None);
        assert_eq!(dangerous_extension("https://example.com/photo.jpg"), None);
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
}
