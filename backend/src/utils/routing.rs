//! Smart conditional routing: resolve a link's destination per-request based on
//! the visitor's device, OS, country and language, with optional weighted A/B
//! splits. Pure and unit-testable — no DB or env access.

use crate::entity::routing_rules::Model as RoutingRule;
use crate::utils::geoip::{GeoLocation, UserAgentInfo};

/// Extract the primary BCP-47 subtag of the first `Accept-Language` entry.
/// e.g. "en-US,en;q=0.9" → "en".
fn primary_lang(accept_language: Option<&str>) -> Option<String> {
    let first = accept_language?.split(',').next()?;
    let tag = first.split(';').next().unwrap_or("").trim();
    let primary = tag.split('-').next().unwrap_or("").trim();
    if primary.is_empty() {
        None
    } else {
        Some(primary.to_lowercase())
    }
}

/// A rule matches when every non-NULL condition matches. All-NULL is a wildcard
/// (the catch-all default). `lang` is the visitor's primary language subtag.
pub fn rule_matches(
    rule: &RoutingRule,
    ua: &UserAgentInfo,
    geo: &GeoLocation,
    lang: Option<&str>,
) -> bool {
    if let Some(want) = &rule.match_device {
        match &ua.device {
            Some(have) if have.eq_ignore_ascii_case(want) => {}
            _ => return false,
        }
    }
    if let Some(want) = &rule.match_os {
        // Prefix match so a rule of "Windows" catches "Windows 10" / "Windows 11".
        match &ua.os {
            Some(have) if have.to_lowercase().starts_with(&want.to_lowercase()) => {}
            _ => return false,
        }
    }
    if let Some(want) = &rule.match_country {
        match &geo.country_code {
            Some(have) if have.eq_ignore_ascii_case(want) => {}
            _ => return false,
        }
    }
    if let Some(want) = &rule.match_lang {
        match lang {
            Some(have) if have.eq_ignore_ascii_case(want) => {}
            _ => return false,
        }
    }
    true
}

/// Resolve the destination for a request. Rules are evaluated by ascending
/// `priority`; the first priority level with one or more matches wins. When
/// several rules tie at that priority, one is chosen by weighted random (A/B).
/// Falls back to `fallback` (the link's own URL) when nothing matches.
pub fn resolve_destination(
    rules: &[RoutingRule],
    ua: &UserAgentInfo,
    geo: &GeoLocation,
    accept_language: Option<&str>,
    fallback: &str,
) -> String {
    let lang = primary_lang(accept_language);
    let mut matches: Vec<&RoutingRule> = rules
        .iter()
        .filter(|r| rule_matches(r, ua, geo, lang.as_deref()))
        .collect();

    if matches.is_empty() {
        return fallback.to_string();
    }

    let min_priority = matches.iter().map(|r| r.priority).min().unwrap_or(0);
    matches.retain(|r| r.priority == min_priority);

    if matches.len() == 1 {
        return matches[0].destination_url.clone();
    }

    // Weighted A/B split among the tied rules.
    let total: i32 = matches.iter().map(|r| r.weight.max(0)).sum();
    if total <= 0 {
        return matches[0].destination_url.clone();
    }
    let mut roll = (rand::random::<f64>() * total as f64) as i32;
    for r in &matches {
        roll -= r.weight.max(0);
        if roll < 0 {
            return r.destination_url.clone();
        }
    }
    matches[0].destination_url.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: i32, priority: i32, dest: &str) -> RoutingRule {
        RoutingRule {
            id,
            link_id: 1,
            priority,
            match_device: None,
            match_os: None,
            match_country: None,
            match_lang: None,
            destination_url: dest.to_string(),
            weight: 1,
            created_at: chrono::Utc::now().naive_utc(),
        }
    }

    fn ua(device: &str, os: &str) -> UserAgentInfo {
        UserAgentInfo {
            browser: None,
            os: Some(os.to_string()),
            device: Some(device.to_string()),
        }
    }

    fn geo(country_code: Option<&str>) -> GeoLocation {
        GeoLocation {
            country_code: country_code.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn primary_lang_extracts_subtag() {
        assert_eq!(primary_lang(Some("en-US,en;q=0.9")), Some("en".to_string()));
        assert_eq!(primary_lang(Some("de")), Some("de".to_string()));
        assert_eq!(primary_lang(None), None);
        assert_eq!(primary_lang(Some("")), None);
    }

    #[test]
    fn all_null_rule_is_wildcard() {
        let r = rule(1, 0, "https://default.example");
        assert!(rule_matches(&r, &ua("Desktop", "Linux"), &geo(None), None));
    }

    #[test]
    fn device_condition() {
        let mut r = rule(1, 0, "https://m.example");
        r.match_device = Some("Mobile".to_string());
        assert!(rule_matches(&r, &ua("Mobile", "iOS"), &geo(None), None));
        assert!(!rule_matches(&r, &ua("Desktop", "Windows 10"), &geo(None), None));
    }

    #[test]
    fn os_prefix_match() {
        let mut r = rule(1, 0, "https://win.example");
        r.match_os = Some("Windows".to_string());
        assert!(rule_matches(&r, &ua("Desktop", "Windows 10"), &geo(None), None));
        assert!(rule_matches(&r, &ua("Desktop", "Windows 11"), &geo(None), None));
        assert!(!rule_matches(&r, &ua("Desktop", "macOS"), &geo(None), None));
    }

    #[test]
    fn country_and_lang_conditions() {
        let mut r = rule(1, 0, "https://de.example");
        r.match_country = Some("DE".to_string());
        r.match_lang = Some("de".to_string());
        assert!(rule_matches(&r, &ua("Desktop", "Linux"), &geo(Some("de")), Some("de")));
        // Wrong country.
        assert!(!rule_matches(&r, &ua("Desktop", "Linux"), &geo(Some("FR")), Some("de")));
        // Wrong language.
        assert!(!rule_matches(&r, &ua("Desktop", "Linux"), &geo(Some("DE")), Some("en")));
    }

    #[test]
    fn resolve_falls_back_when_nothing_matches() {
        let mut r = rule(1, 0, "https://m.example");
        r.match_device = Some("Mobile".to_string());
        let dest = resolve_destination(
            &[r],
            &ua("Desktop", "Windows 10"),
            &geo(None),
            None,
            "https://fallback.example",
        );
        assert_eq!(dest, "https://fallback.example");
    }

    #[test]
    fn resolve_picks_by_priority() {
        let mut high = rule(1, 0, "https://first.example");
        high.match_device = Some("Mobile".to_string());
        let mut low = rule(2, 5, "https://second.example");
        low.match_device = Some("Mobile".to_string());
        // Both match an iPhone; lower priority value (0) wins.
        let dest = resolve_destination(
            &[low, high],
            &ua("Mobile", "iOS"),
            &geo(None),
            None,
            "https://fallback.example",
        );
        assert_eq!(dest, "https://first.example");
    }

    #[test]
    fn resolve_device_specific_over_default() {
        // Catch-all default at higher priority value; device rule at 0.
        let default = rule(1, 10, "https://web.example");
        let mut mobile = rule(2, 0, "https://app.example");
        mobile.match_device = Some("Mobile".to_string());
        let rules = vec![default, mobile];
        assert_eq!(
            resolve_destination(&rules, &ua("Mobile", "Android"), &geo(None), None, "https://fb"),
            "https://app.example"
        );
        assert_eq!(
            resolve_destination(&rules, &ua("Desktop", "Linux"), &geo(None), None, "https://fb"),
            "https://web.example"
        );
    }

    #[test]
    fn weighted_split_returns_a_candidate() {
        let mut a = rule(1, 0, "https://a.example");
        a.weight = 1;
        let mut b = rule(2, 0, "https://b.example");
        b.weight = 1;
        let dest = resolve_destination(&[a, b], &ua("Desktop", "Linux"), &geo(None), None, "https://fb");
        assert!(dest == "https://a.example" || dest == "https://b.example");
    }
}
