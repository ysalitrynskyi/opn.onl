//! GeoIP and User Agent parsing tests

mod common;

use axum::http::header;
use opn_onl_backend::utils::geoip::{lookup_ip, parse_user_agent};
use serde_json::{Value, json};

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

/// Full User-Agent strings as browsers and fetchers really send them. The
/// shortened strings above pass for the wrong reasons (a real iPad UA also says
/// "Mobile", iOS Chrome says "Safari", Googlebot smartphone says "Mobile").
mod real_world_user_agents {
    use super::*;

    const CHROME_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    const EDGE_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0";
    const OPERA_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0";
    const IE11_WINDOWS: &str =
        "Mozilla/5.0 (Windows NT 10.0; WOW64; Trident/7.0; rv:11.0) like Gecko";
    const FIREFOX_LINUX: &str =
        "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0";
    const SAFARI_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
        (KHTML, like Gecko) Version/17.2 Safari/605.1.15";
    const CHROME_OS: &str = "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    const SAFARI_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1";
    const CHROME_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/120.0.6099.119 Mobile/15E148 Safari/604.1";
    const FIREFOX_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/121.0 Mobile/15E148 Safari/605.1.15";
    const EDGE_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 EdgiOS/120.0.2210.126 Mobile/15E148 \
        Safari/604.1";
    const OPERA_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 12_1 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 OPiOS/16.0.15.124050 Safari/605.1.15";

    const SAFARI_IPAD: &str = "Mozilla/5.0 (iPad; CPU OS 16_6 like Mac OS X) AppleWebKit/605.1.15 \
        (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";
    const CHROME_IPAD: &str = "Mozilla/5.0 (iPad; CPU OS 17_2 like Mac OS X) AppleWebKit/605.1.15 \
        (KHTML, like Gecko) CriOS/120.0.6099.119 Mobile/15E148 Safari/604.1";

    const CHROME_ANDROID: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/120.0.6099.144 Mobile Safari/537.36";
    const EDGE_ANDROID: &str = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36 EdgA/120.0.2210.84";
    const SAMSUNG_ANDROID: &str = "Mozilla/5.0 (Linux; Android 13; SAMSUNG SM-S918B) \
        AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/23.0 Chrome/115.0.0.0 Mobile \
        Safari/537.36";
    const FIREFOX_ANDROID: &str =
        "Mozilla/5.0 (Android 14; Mobile; rv:121.0) Gecko/121.0 Firefox/121.0";
    const FIREFOX_ANDROID_TABLET: &str =
        "Mozilla/5.0 (Android 14; Tablet; rv:121.0) Gecko/121.0 Firefox/121.0";
    // Chrome on an Android tablet omits the "Mobile" token and has no "Tablet".
    const CHROME_ANDROID_TABLET: &str = "Mozilla/5.0 (Linux; Android 13; SM-X700) \
        AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    fn classify(ua: &str) -> (Option<String>, Option<String>, Option<String>) {
        let info = parse_user_agent(ua);
        (info.browser, info.os, info.device)
    }

    fn expect(ua: &str, browser: &str, os: &str, device: &str) {
        assert_eq!(
            classify(ua),
            (
                Some(browser.to_string()),
                Some(os.to_string()),
                Some(device.to_string())
            ),
            "(browser, os, device) of {ua}"
        );
    }

    #[test]
    fn desktop_browsers() {
        expect(CHROME_WINDOWS, "Chrome", "Windows 10", "Desktop");
        expect(EDGE_WINDOWS, "Edge", "Windows 10", "Desktop");
        expect(OPERA_WINDOWS, "Opera", "Windows 10", "Desktop");
        expect(IE11_WINDOWS, "Internet Explorer", "Windows 10", "Desktop");
        expect(FIREFOX_LINUX, "Firefox", "Linux", "Desktop");
        expect(SAFARI_MAC, "Safari", "macOS", "Desktop");
        expect(CHROME_OS, "Chrome", "Chrome OS", "Desktop");
    }

    /// Every iOS browser is a WebKit shell: its UA says "Safari/" and only its
    /// own token (CriOS, FxiOS, EdgiOS, OPiOS) says which browser it is.
    #[test]
    fn iphone_browsers_are_named_after_the_app_not_the_engine() {
        expect(SAFARI_IPHONE, "Safari", "iOS", "Mobile");
        expect(CHROME_IPHONE, "Chrome", "iOS", "Mobile");
        expect(FIREFOX_IPHONE, "Firefox", "iOS", "Mobile");
        expect(EDGE_IPHONE, "Edge", "iOS", "Mobile");
        expect(OPERA_IPHONE, "Opera", "iOS", "Mobile");
    }

    /// Android browsers built on Chrome all say "Chrome/"; their own token
    /// (EdgA, SamsungBrowser) has to win.
    #[test]
    fn android_browsers_are_named_after_the_app_not_the_engine() {
        expect(CHROME_ANDROID, "Chrome", "Android", "Mobile");
        expect(EDGE_ANDROID, "Edge", "Android", "Mobile");
        expect(SAMSUNG_ANDROID, "Samsung Internet", "Android", "Mobile");
        expect(FIREFOX_ANDROID, "Firefox", "Android", "Mobile");
    }

    /// A real iPad UA also carries "Mobile/15E148", and Chrome on an Android
    /// tablet carries neither "Mobile" nor "Tablet", so neither was ever a
    /// "Tablet" (which made a Tablet routing rule unmatchable).
    #[test]
    fn tablets_are_not_mobile() {
        expect(SAFARI_IPAD, "Safari", "iOS", "Tablet");
        expect(CHROME_IPAD, "Chrome", "iOS", "Tablet");
        expect(CHROME_ANDROID_TABLET, "Chrome", "Android", "Tablet");
        expect(FIREFOX_ANDROID_TABLET, "Firefox", "Android", "Tablet");
    }

    /// Crawlers and link-preview fetchers are not visitors. Several carry the
    /// word "Mobile" (Googlebot smartphone) or no marker at all.
    #[test]
    fn bots_are_bots_whatever_else_their_user_agent_says() {
        for ua in [
            "Mozilla/5.0 (Linux; Android 6.0.1; Nexus 5X Build/MMB29P) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0.6099.71 Mobile Safari/537.36 \
             (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Googlebot/2.1; \
             +http://www.google.com/bot.html) Chrome/120.0.6099.71 Safari/537.36",
            "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_5) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/13.1.1 Safari/605.1.15 (Applebot/0.1; \
             +http://www.apple.com/go/applebot)",
            "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)",
            "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
            "WhatsApp/2.23.20.0 A",
            "TelegramBot (like TwitterBot)",
            "Mozilla/5.0 (Windows NT 6.1; WOW64) SkypeUriPreview Preview/0.5 \
             skype-url-preview@microsoft.com",
            "curl/8.4.0",
            "Wget/1.21.4",
            "python-requests/2.31.0",
            "Go-http-client/2.0",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             HeadlessChrome/120.0.0.0 Safari/537.36",
        ] {
            assert_eq!(parse_user_agent(ua).device.as_deref(), Some("Bot"), "{ua}");
        }
    }

    /// Controls: the bot rules must not swallow real browsers.
    #[test]
    fn browsers_are_not_bots() {
        for ua in [
            CHROME_WINDOWS,
            SAFARI_MAC,
            FIREFOX_LINUX,
            SAFARI_IPHONE,
            CHROME_IPHONE,
            SAFARI_IPAD,
            CHROME_ANDROID,
            CHROME_ANDROID_TABLET,
        ] {
            assert_ne!(parse_user_agent(ua).device.as_deref(), Some("Bot"), "{ua}");
        }
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

/// A routing rule matches on the device the same parser reports for the click.
mod device_routing {
    use super::*;

    const IPAD_UA: &str = "Mozilla/5.0 (iPad; CPU OS 16_6 like Mac OS X) AppleWebKit/605.1.15 \
        (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";
    const IPHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1";
    const ANDROID_TABLET_UA: &str = "Mozilla/5.0 (Linux; Android 13; SM-X700) \
        AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    const DESKTOP_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15";

    const FALLBACK: &str = "https://iana.org/fallback";
    const TABLET: &str = "https://iana.org/tablet";
    const MOBILE: &str = "https://iana.org/mobile";

    /// Where a visitor with this `User-Agent` is sent.
    async fn visit(server: &axum_test::TestServer, code: &str, ua: &str) -> String {
        let res = server
            .get(&format!("/{code}"))
            .add_header(header::USER_AGENT, ua)
            .await;
        assert_eq!(res.status_code(), 307, "{ua}: {}", res.text());
        res.header(header::LOCATION)
            .to_str()
            .expect("utf-8 Location")
            .to_string()
    }

    /// iPads and Android tablets used to be classified as "Mobile", so a
    /// "Tablet" rule never fired for them and the Mobile rule caught them.
    #[tokio::test]
    async fn a_tablet_rule_matches_ipads_and_android_tablets_but_not_phones() {
        let (server, db) = common::spawn_real_app().await;
        let res = server
            .post("/auth/register")
            .json(&json!({ "email": common::unique_email(), "password": "password123" }))
            .await;
        assert_eq!(res.status_code(), 201, "register: {}", res.text());
        let body: Value = res.json();
        let token = body["token"].as_str().expect("token").to_string();
        common::mark_email_verified(&db, body["user_id"].as_i64().expect("user_id") as i32).await;

        let res = server
            .post("/links")
            .authorization_bearer(&token)
            .json(&json!({ "original_url": FALLBACK }))
            .await;
        assert_eq!(res.status_code(), 201, "create link: {}", res.text());
        let link: Value = res.json();
        let (id, code) = (
            link["id"].as_i64().expect("id"),
            link["code"].as_str().expect("code").to_string(),
        );

        let res = server
            .put(&format!("/links/{id}/rules"))
            .authorization_bearer(&token)
            .json(&json!({ "rules": [
                { "destination_url": TABLET, "match_device": "Tablet", "priority": 0 },
                { "destination_url": MOBILE, "match_device": "Mobile", "priority": 1 },
            ] }))
            .await;
        assert_eq!(res.status_code(), 200, "PUT rules: {}", res.text());

        assert_eq!(visit(&server, &code, IPAD_UA).await, TABLET);
        assert_eq!(visit(&server, &code, ANDROID_TABLET_UA).await, TABLET);
        // Controls: phones still take the Mobile rule, desktops the fallback.
        assert_eq!(visit(&server, &code, IPHONE_UA).await, MOBILE);
        assert_eq!(visit(&server, &code, DESKTOP_UA).await, FALLBACK);
    }
}
