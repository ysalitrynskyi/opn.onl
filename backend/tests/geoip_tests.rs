//! GeoIP and User Agent parsing tests

#[path = "../src/utils/geoip.rs"]
mod geoip;

use geoip::{parse_user_agent, lookup_ip, GeoLocation, UserAgentInfo};

mod browser_detection {
    use super::*;

    #[test]
    fn test_chrome_detection() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let info = parse_user_agent(ua);
        assert_eq!(info.browser, Some("Chrome".to_string()));
    }

    #[test]
    fn test_firefox_detection() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/120.0";
        let info = parse_user_agent(ua);
        assert_eq!(info.browser, Some("Firefox".to_string()));
    }

    #[test]
    fn test_safari_detection() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        let info = parse_user_agent(ua);
        assert_eq!(info.browser, Some("Safari".to_string()));
    }

    #[test]
    fn test_edge_detection() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0";
        let info = parse_user_agent(ua);
        assert_eq!(info.browser, Some("Edge".to_string()));
    }

    #[test]
    fn test_opera_detection() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0";
        let info = parse_user_agent(ua);
        assert_eq!(info.browser, Some("Opera".to_string()));
    }
}

mod os_detection {
    use super::*;

    #[test]
    fn test_windows_10_detection() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
        let info = parse_user_agent(ua);
        assert_eq!(info.os, Some("Windows 10".to_string()));
    }

    #[test]
    fn test_macos_detection() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
        let info = parse_user_agent(ua);
        assert_eq!(info.os, Some("macOS".to_string()));
    }

    #[test]
    fn test_linux_detection() {
        let ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36";
        let info = parse_user_agent(ua);
        assert_eq!(info.os, Some("Linux".to_string()));
    }

    #[test]
    fn test_android_detection() {
        let ua = "Mozilla/5.0 (Linux; Android 13; SM-G991B) AppleWebKit/537.36";
        let info = parse_user_agent(ua);
        assert_eq!(info.os, Some("Android".to_string()));
    }

    #[test]
    fn test_ios_detection() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15";
        let info = parse_user_agent(ua);
        assert_eq!(info.os, Some("iOS".to_string()));
    }
}

mod device_detection {
    use super::*;

    #[test]
    fn test_mobile_detection() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
        let info = parse_user_agent(ua);
        assert_eq!(info.device, Some("Mobile".to_string()));
    }

    #[test]
    fn test_tablet_detection() {
        let ua = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15";
        let info = parse_user_agent(ua);
        assert_eq!(info.device, Some("Tablet".to_string()));
    }

    #[test]
    fn test_desktop_detection() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
        let info = parse_user_agent(ua);
        assert_eq!(info.device, Some("Desktop".to_string()));
    }

    #[test]
    fn test_bot_detection() {
        let ua = "Googlebot/2.1 (+http://www.google.com/bot.html)";
        let info = parse_user_agent(ua);
        assert_eq!(info.device, Some("Bot".to_string()));
    }
}

mod ip_lookup {
    use super::*;

    #[test]
    fn test_private_ip_returns_empty() {
        let result = lookup_ip("192.168.1.1");
        assert!(result.country.is_none());
        assert!(result.city.is_none());
    }

    #[test]
    fn test_loopback_ip_returns_empty() {
        let result = lookup_ip("127.0.0.1");
        assert!(result.country.is_none());
    }

    #[test]
    fn test_invalid_ip_returns_empty() {
        let result = lookup_ip("invalid_ip");
        assert!(result.country.is_none());
    }

    #[test]
    fn test_public_ip_without_db() {
        // Without GeoIP database, should return empty
        let result = lookup_ip("8.8.8.8");
        // This might return data if the DB is present, so we just check it doesn't panic
        let _ = result;
    }
}

