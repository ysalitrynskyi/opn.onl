use maxminddb::{geoip2, Reader};
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
    tracing::info!("To enable GeoIP, download GeoLite2-City.mmdb from MaxMind and place it in ./data/");
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

    match reader.lookup::<geoip2::City>(ip) {
        Ok(city) => {
            let country = city.country.as_ref().and_then(|c| {
                c.names.as_ref().and_then(|n| n.get("en").map(|s| s.to_string()))
            });
            let country_code = city.country.as_ref().and_then(|c| c.iso_code.map(|s| s.to_string()));
            let city_name = city.city.as_ref().and_then(|c| {
                c.names.as_ref().and_then(|n| n.get("en").map(|s| s.to_string()))
            });
            let region = city.subdivisions.as_ref().and_then(|subs| {
                subs.first().and_then(|s| {
                    s.names.as_ref().and_then(|n| n.get("en").map(|s| s.to_string()))
                })
            });
            let (latitude, longitude) = city.location.as_ref().map(|l| {
                (l.latitude, l.longitude)
            }).unwrap_or((None, None));

            GeoLocation {
                country,
                country_code,
                city: city_name,
                region,
                latitude,
                longitude,
            }
        }
        Err(_) => GeoLocation::default(),
    }
}

/// Check if an IP address is private/local
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_private() || ipv4.is_loopback() || ipv4.is_link_local()
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
        }
    }
}

/// Parse user agent string to extract device, browser, and OS info
pub fn parse_user_agent(ua: &str) -> UserAgentInfo {
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
    
    if ua_lower.contains("edg/") || ua_lower.contains("edge/") {
        Some("Edge".to_string())
    } else if ua_lower.contains("opr/") || ua_lower.contains("opera") {
        Some("Opera".to_string())
    } else if ua_lower.contains("chrome/") && !ua_lower.contains("chromium") {
        Some("Chrome".to_string())
    } else if ua_lower.contains("firefox/") {
        Some("Firefox".to_string())
    } else if ua_lower.contains("safari/") && !ua_lower.contains("chrome") {
        Some("Safari".to_string())
    } else if ua_lower.contains("msie") || ua_lower.contains("trident/") {
        Some("Internet Explorer".to_string())
    } else {
        None
    }
}

/// Detect OS from user agent
fn detect_os(ua: &str) -> Option<String> {
    let ua_lower = ua.to_lowercase();
    
    // Check iOS before macOS since iOS UAs contain "Mac OS X"
    if ua_lower.contains("iphone") || ua_lower.contains("ipad") || ua_lower.contains("ios") {
        Some("iOS".to_string())
    } else if ua_lower.contains("android") {
        Some("Android".to_string())
    } else if ua_lower.contains("windows nt 10") {
        Some("Windows 10".to_string())
    } else if ua_lower.contains("windows nt 11") {
        Some("Windows 11".to_string())
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

/// Detect device type from user agent
fn detect_device(ua: &str) -> String {
    let ua_lower = ua.to_lowercase();
    
    if ua_lower.contains("mobile") || ua_lower.contains("iphone") || 
       (ua_lower.contains("android") && !ua_lower.contains("tablet")) {
        "Mobile".to_string()
    } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
        "Tablet".to_string()
    } else if ua_lower.contains("bot") || ua_lower.contains("crawler") || ua_lower.contains("spider") {
        "Bot".to_string()
    } else {
        "Desktop".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_detection() {
        assert_eq!(detect_browser("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"), Some("Chrome".to_string()));
        assert_eq!(detect_browser("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0"), Some("Edge".to_string()));
        assert_eq!(detect_browser("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/120.0"), Some("Firefox".to_string()));
    }

    #[test]
    fn test_device_detection() {
        assert_eq!(detect_device("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)"), "Mobile");
        assert_eq!(detect_device("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X)"), "Tablet");
        assert_eq!(detect_device("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"), "Desktop");
    }

    #[test]
    fn test_private_ip() {
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"192.168.1.1".parse().unwrap()));
        assert!(is_private_ip(&"10.0.0.1".parse().unwrap()));
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
    }
}
