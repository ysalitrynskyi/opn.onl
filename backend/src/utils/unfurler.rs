//! Recognise link unfurlers: the crawlers chat apps and social networks run to
//! fetch a URL and draw a preview card, usually the moment somebody pastes it
//! and well before the recipient clicks. Pure and unit-testable.

/// Product tokens of known unfurlers, matched case-insensitively anywhere in
/// the `User-Agent`.
///
/// The list is deliberately conservative. It names only crawlers whose job is
/// to fetch one URL for a preview card; search engines, monitoring tools and
/// the in-app browsers of these same apps (which send a real browser's
/// `User-Agent` for a real visit) are not listed. A false match turns a real
/// visitor away from a one-time link, while a missed unfurler only costs the
/// click it would have cost before.
const UNFURLER_TOKENS: &[&str] = &[
    "Slackbot-LinkExpanding",
    "Slack-ImgProxy",
    "Discordbot",
    "WhatsApp",
    "facebookexternalhit",
    "Facebot",
    "Twitterbot",
    "TelegramBot",
    "LinkedInBot",
    "SkypeUriPreview",
    "redditbot",
    "Iframely",
    "Embedly",
    "MicrosoftPreview",
    "Mastodon",
];

/// Whether this `User-Agent` belongs to a known link unfurler.
pub fn is_link_unfurler(user_agent: &str) -> bool {
    let user_agent = user_agent.to_ascii_lowercase();
    UNFURLER_TOKENS
        .iter()
        .any(|token| user_agent.contains(&token.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_common_unfurlers() {
        for ua in [
            "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)",
            "Slack-ImgProxy (+https://api.slack.com/robots)",
            "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)",
            "WhatsApp/2.23.20.0 A",
            "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
            "Facebot Twitterbot/1.0",
            "Twitterbot/1.0",
            "TelegramBot (like TwitterBot)",
            "LinkedInBot/1.0 (compatible; Mozilla/5.0; Apache-HttpClient +http://www.linkedin.com)",
            "Mozilla/5.0 (Windows NT 6.1; WOW64) SkypeUriPreview Preview/0.5 skype-url-preview@microsoft.com",
            "Mozilla/5.0 (compatible; redditbot/1.0; +http://www.reddit.com/feedback)",
            "Iframely/1.3.1 (+https://iframely.com/docs/about)",
            "Mozilla/5.0 (compatible; Embedly/0.2; +http://support.embed.ly/)",
            "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 MicrosoftPreview/2.0 +https://aka.ms/MicrosoftPreview",
            "http.rb/5.1.1 (Mastodon/4.2.0; +https://mastodon.social/)",
        ] {
            assert!(is_link_unfurler(ua), "{ua} should be recognised");
        }
    }

    #[test]
    fn matching_ignores_case() {
        assert!(is_link_unfurler("DISCORDBOT/2.0"));
        assert!(is_link_unfurler("twitterbot/1.0"));
    }

    #[test]
    fn browsers_and_other_clients_are_not_unfurlers() {
        for ua in [
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Mobile Safari/537.36 [FB_IAB/FB4A;FBAV/440.0]",
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "curl/8.4.0",
            "",
        ] {
            assert!(!is_link_unfurler(ua), "{ua:?} should not be recognised");
        }
    }
}
