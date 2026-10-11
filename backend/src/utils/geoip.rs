use crate::utils::unfurler::is_link_unfurler;
use maxminddb::{Reader, geoip2};
use once_cell::sync::Lazy;
use std::net::IpAddr;
use std::path::Path;

/// GeoIP location data
#[derive(Debug, Clone, Default)]
pub struct GeoLocation {
    pub country: Option<String>,
    #[allow(dead_code)]
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Try to load the MaxMind GeoIP database
/// The database should be placed at: ./data/GeoLite2-City.mmdb
static GEOIP_READER: Lazy<Option<Reader<Vec<u8>>>> = Lazy::new(|| {
    let paths = [
        "data/GeoLite2-City.mmdb",
        "./data/GeoLite2-City.mmdb",
        "/opt/geoip/GeoLite2-City.mmdb",
        "GeoLite2-City.mmdb",
    ];

    for path in &paths {
        if Path::new(path).exists() {
            match Reader::open_readfile(path) {
                Ok(reader) => {
                    tracing::info!("Loaded GeoIP database from: {}", path);
                    return Some(reader);
                }
                Err(e) => {
                    tracing::warn!("Failed to load GeoIP database from {}: {}", path, e);
                }
            }
        }
    }

    tracing::warn!("GeoIP database not found. GeoIP lookups will be disabled.");
    tracing::info!(
        "To enable GeoIP, download GeoLite2-City.mmdb from MaxMind and place it in ./data/"
    );
    None
});

/// Look up IP address and return location data
pub fn lookup_ip(ip_str: &str) -> GeoLocation {
    let reader = match GEOIP_READER.as_ref() {
        Some(r) => r,
        None => return GeoLocation::default(),
    };

    let ip: IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return GeoLocation::default(),
    };

    // Skip private/local IPs
    if is_private_ip(&ip) {
        return GeoLocation::default();
    }

    // A proxy may report an IPv4 client as `::ffff:a.b.c.d`; look it up as the
    // IPv4 address it is.
    match reader.lookup(ip.to_canonical()) {
        Ok(result) => match result.decode::<geoip2::City>() {
            Ok(Some(city)) => GeoLocation {
                country: city.country.names.english.map(str::to_string),
                country_code: city.country.iso_code.map(str::to_string),
                city: city.city.names.english.map(str::to_string),
                region: city
                    .subdivisions
                    .first()
                    .and_then(|subdivision| subdivision.names.english.map(str::to_string)),
                latitude: city.location.latitude,
                longitude: city.location.longitude,
            },
            _ => GeoLocation::default(),
        },
        Err(_) => GeoLocation::default(),
    }
}

/// Check if an IP address is private/local. An IPv4-mapped IPv6 address is
/// judged as the IPv4 address it wraps.
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ipv4) => ipv4.is_private() || ipv4.is_loopback() || ipv4.is_link_local(),
        IpAddr::V6(ipv6) => ipv6.is_loopback(),
    }
}

/// Parse user agent string to extract device, browser, and OS info
pub fn parse_user_agent(ua: &str) -> UserAgentInfo {
    // An empty header tells us nothing; calling it "Desktop" would feed a guess
    // to the device chart and to `match_device` rules.
    if ua.trim().is_empty() {
        return UserAgentInfo::default();
    }
    UserAgentInfo {
        browser: detect_browser(ua),
        os: detect_os(ua),
        device: Some(detect_device(ua)),
    }
}

/// User agent parsed information
#[derive(Debug, Clone, Default)]
pub struct UserAgentInfo {
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device: Option<String>,
}

/// Detect browser from user agent
fn detect_browser(ua: &str) -> Option<String> {
    let ua_lower = ua.to_lowercase();
    let has = |tokens: &[&str]| tokens.iter().any(|token| ua_lower.contains(token));

    // Order matters. Nearly every browser also says "Chrome/" and "Safari/",
    // and the iOS ones (Chrome, Firefox, Edge, Opera) are WebKit shells whose
    // only mark is their own token next to a plain "Safari/".
    if has(&["edg/", "edge/", "edga/", "edgios/"]) {
        Some("Edge".to_string())
    } else if has(&["opr/", "opera", "opios/"]) {
        Some("Opera".to_string())
    } else if ua_lower.contains("samsungbrowser/") {
        Some("Samsung Internet".to_string())
    } else if has(&["chrome/", "crios/"]) && !ua_lower.contains("chromium") {
        Some("Chrome".to_string())
    } else if has(&["firefox/", "fxios/"]) {
        Some("Firefox".to_string())
    } else if ua_lower.contains("safari/") && !ua_lower.contains("chrome") {
        Some("Safari".to_string())
    } else if has(&["msie", "trident/"]) {
        Some("Internet Explorer".to_string())
    } else {
        None
    }
}

/// Detect OS from user agent
fn detect_os(ua: &str) -> Option<String> {
    let ua_lower = ua.to_lowercase();

    // Check iOS before macOS since iOS UAs contain "Mac OS X"
    if ua_lower.contains("iphone")
        || ua_lower.contains("ipad")
        || ua_lower.contains("ipod")
        || ua_lower.contains("ios")
    {
        Some("iOS".to_string())
    } else if ua_lower.contains("android") {
        Some("Android".to_string())
    } else if ua_lower.contains("windows nt 10") {
        // Windows 11 also sends "NT 10.0"; only Client Hints tell them apart.
        Some("Windows 10".to_string())
    } else if ua_lower.contains("windows") {
        Some("Windows".to_string())
    } else if ua_lower.contains("mac os x") || ua_lower.contains("macos") {
        Some("macOS".to_string())
    } else if ua_lower.contains("linux") {
        Some("Linux".to_string())
    } else if ua_lower.contains("chromeos") || ua_lower.contains("cros") {
        Some("Chrome OS".to_string())
    } else {
        None
    }
}

/// Markers of automated clients that `is_link_unfurler` does not already name:
/// crawlers (Googlebot, Bingbot, ... all say "bot"), uptime and performance
/// checkers, and command-line or library HTTP clients.
const BOT_TOKENS: &[&str] = &[
    "bot",
    "crawler",
    "spider",
    "slurp",
    "bingpreview",
    "headlesschrome",
    "lighthouse",
    "curl/",
    "wget/",
    "python-requests",
    "python-urllib",
    "go-http-client",
    "libwww-perl",
    "node-fetch",
    "axios/",
    "java/",
    "apache-httpclient",
];

/// Detect device type from user agent
fn detect_device(ua: &str) -> String {
    let ua_lower = ua.to_lowercase();

    // Bots first: Googlebot smartphone also says "Mobile", and a link-preview
    // fetcher is no visitor at all.
    if is_link_unfurler(ua) || BOT_TOKENS.iter().any(|token| ua_lower.contains(token)) {
        return "Bot".to_string();
    }

    // Tablet before mobile: an iPad UA carries "Mobile/15E148" too, and Chrome
    // on an Android tablet sends the phone UA minus the "Mobile" token. (An
    // iPad that asks for the desktop site sends a Mac UA and cannot be told
    // apart.)
    if ua_lower.contains("ipad")
        || ua_lower.contains("tablet")
        || (ua_lower.contains("android")
            && ua_lower.contains("mozilla/")
            && !ua_lower.contains("mobile"))
    {
        return "Tablet".to_string();
    }

    if ua_lower.contains("mobile")
        || ua_lower.contains("iphone")
        || ua_lower.contains("ipod")
        || ua_lower.contains("android")
    {
        return "Mobile".to_string();
    }

    "Desktop".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_detection() {
        assert_eq!(
            detect_browser(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
            ),
            Some("Chrome".to_string())
        );
        assert_eq!(
            detect_browser(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0"
            ),
            Some("Edge".to_string())
        );
        assert_eq!(
            detect_browser(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/120.0"
            ),
            Some("Firefox".to_string())
        );
    }

    #[test]
    fn test_device_detection() {
        assert_eq!(
            detect_device("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)"),
            "Mobile"
        );
        assert_eq!(
            detect_device("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X)"),
            "Tablet"
        );
        assert_eq!(
            detect_device("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            "Desktop"
        );
    }

    #[test]
    fn test_private_ip() {
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"192.168.1.1".parse().unwrap()));
        assert!(is_private_ip(&"10.0.0.1".parse().unwrap()));
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn ipv4_mapped_addresses_are_judged_as_ipv4() {
        assert!(is_private_ip(&"::ffff:10.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_private_ip(&"::ffff:8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn an_empty_user_agent_is_unknown_not_desktop() {
        for ua in ["", "   "] {
            let info = parse_user_agent(ua);
            assert_eq!((info.browser, info.os, info.device), (None, None, None));
        }
    }
}
