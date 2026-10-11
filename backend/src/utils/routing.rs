//! Smart conditional routing: resolve a link's destination per-request based on
//! the visitor's device, OS, country and language, with optional weighted A/B
//! splits. Pure and unit-testable — no DB or env access.

use crate::entity::routing_rules::Model as RoutingRule;
use crate::utils::geoip::{GeoLocation, UserAgentInfo};
use rand::Rng;

/// Device classes a rule can target: what `geoip::parse_user_agent` reports.
pub const DEVICE_TYPES: [&str; 4] = ["Mobile", "Tablet", "Desktop", "Bot"];

/// A blank condition means "match anything".
fn condition(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|s| !s.is_empty())
}

/// Canonical device class for a rule, or why it can never match.
pub fn normalize_device(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = condition(raw) else {
        return Ok(None);
    };
    DEVICE_TYPES
        .iter()
        .find(|d| d.eq_ignore_ascii_case(value))
        .map(|d| Some((*d).to_string()))
        .ok_or_else(|| format!("device must be one of {}", DEVICE_TYPES.join(", ")))
}

/// Operating-system prefix as reported by the User-Agent parser ("iOS",
/// "Windows 10", ...).
pub fn normalize_os(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = condition(raw) else {
        return Ok(None);
    };
    let plain = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_'));
    if value.len() > 32 || !plain {
        return Err("os must be a short name such as iOS, Android or Windows".to_string());
    }
    Ok(Some(value.to_string()))
}

/// ISO 3166-1 alpha-2 code, the only shape the GeoIP lookup ever returns.
pub fn normalize_country(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = condition(raw) else {
        return Ok(None);
    };
    if value.len() == 2 && value.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(Some(value.to_ascii_uppercase()))
    } else {
        Err("country must be a 2-letter ISO code such as US".to_string())
    }
}

/// Primary language subtag, the only part of Accept-Language rules compare.
pub fn normalize_lang(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = condition(raw) else {
        return Ok(None);
    };
    if (2..=3).contains(&value.len()) && value.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(Some(value.to_ascii_lowercase()))
    } else {
        Err("language must be a primary language code such as en or de".to_string())
    }
}

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

/// The rules that can win for a request: every match at the best (lowest)
/// priority. More than one is a weighted A/B split.
fn winning_rules<'a>(
    rules: &'a [RoutingRule],
    ua: &UserAgentInfo,
    geo: &GeoLocation,
    lang: Option<&str>,
) -> Vec<&'a RoutingRule> {
    let mut matches: Vec<&RoutingRule> = rules
        .iter()
        .filter(|r| rule_matches(r, ua, geo, lang))
        .collect();
    if let Some(min_priority) = matches.iter().map(|r| r.priority).min() {
        matches.retain(|r| r.priority == min_priority);
    }
    matches
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
    let matches = winning_rules(rules, ua, geo, lang.as_deref());

    if matches.is_empty() {
        return fallback.to_string();
    }

    if matches.len() == 1 {
        return matches[0].destination_url.clone();
    }

    // Weighted A/B split among the tied rules.
    let total: u64 = matches.iter().map(|r| weight_of(r)).sum();
    if total == 0 {
        return matches[0].destination_url.clone();
    }
    let roll = rand::thread_rng().gen_range(0..total);
    pick_weighted(&matches, roll).destination_url.clone()
}

fn weight_of(rule: &RoutingRule) -> u64 {
    rule.weight.max(0) as u64
}

/// The tied rule whose share of the total weight contains `roll`
/// (`0 <= roll < total weight`). Summed in `u64`: weights are `i32`, so a few
/// large ones overflow an `i32` total.
fn pick_weighted<'a>(tied: &[&'a RoutingRule], mut roll: u64) -> &'a RoutingRule {
    for rule in tied {
        let weight = weight_of(rule);
        if roll < weight {
            return rule;
        }
        roll -= weight;
    }
    tied[0]
}

/// Every destination `resolve_destination` could return for a request. One
/// entry is certain; several mean an A/B split, where nothing can say in advance
/// which one this visitor will get.
pub fn possible_destinations(
    rules: &[RoutingRule],
    ua: &UserAgentInfo,
    geo: &GeoLocation,
    accept_language: Option<&str>,
    fallback: &str,
) -> Vec<String> {
    let lang = primary_lang(accept_language);
    let winners = winning_rules(rules, ua, geo, lang.as_deref());
    if winners.is_empty() {
        return vec![fallback.to_string()];
    }
    let mut destinations: Vec<String> = Vec::new();
    for rule in winners {
        if !destinations.contains(&rule.destination_url) {
            destinations.push(rule.destination_url.clone());
        }
    }
    destinations
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
        assert!(!rule_matches(
            &r,
            &ua("Desktop", "Windows 10"),
            &geo(None),
            None
        ));
    }

    #[test]
    fn os_prefix_match() {
        let mut r = rule(1, 0, "https://win.example");
        r.match_os = Some("Windows".to_string());
        assert!(rule_matches(
            &r,
            &ua("Desktop", "Windows 10"),
            &geo(None),
            None
        ));
        assert!(rule_matches(
            &r,
            &ua("Desktop", "Windows 11"),
            &geo(None),
            None
        ));
        assert!(!rule_matches(&r, &ua("Desktop", "macOS"), &geo(None), None));
    }

    #[test]
    fn country_and_lang_conditions() {
        let mut r = rule(1, 0, "https://de.example");
        r.match_country = Some("DE".to_string());
        r.match_lang = Some("de".to_string());
        assert!(rule_matches(
            &r,
            &ua("Desktop", "Linux"),
            &geo(Some("de")),
            Some("de")
        ));
        // Wrong country.
        assert!(!rule_matches(
            &r,
            &ua("Desktop", "Linux"),
            &geo(Some("FR")),
            Some("de")
        ));
        // Wrong language.
        assert!(!rule_matches(
            &r,
            &ua("Desktop", "Linux"),
            &geo(Some("DE")),
            Some("en")
        ));
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
            resolve_destination(
                &rules,
                &ua("Mobile", "Android"),
                &geo(None),
                None,
                "https://fb"
            ),
            "https://app.example"
        );
        assert_eq!(
            resolve_destination(
                &rules,
                &ua("Desktop", "Linux"),
                &geo(None),
                None,
                "https://fb"
            ),
            "https://web.example"
        );
    }

    #[test]
    fn weighted_split_returns_a_candidate() {
        let mut a = rule(1, 0, "https://a.example");
        a.weight = 1;
        let mut b = rule(2, 0, "https://b.example");
        b.weight = 1;
        let dest = resolve_destination(
            &[a, b],
            &ua("Desktop", "Linux"),
            &geo(None),
            None,
            "https://fb",
        );
        assert!(dest == "https://a.example" || dest == "https://b.example");
    }

    #[test]
    fn pick_weighted_follows_the_weight_bands() {
        let mut a = rule(1, 0, "https://a.example");
        a.weight = 3;
        let mut b = rule(2, 0, "https://b.example");
        b.weight = 1;
        let tied = [&a, &b];
        // Rolls 0..3 land in a's band, 3 in b's.
        for roll in 0..3 {
            assert_eq!(pick_weighted(&tied, roll).id, 1, "roll {roll}");
        }
        assert_eq!(pick_weighted(&tied, 3).id, 2);
    }

    #[test]
    fn huge_weights_do_not_overflow_the_total() {
        // Two i32::MAX weights sum past i32::MAX; as an i32 total this wrapped
        // negative in release and always picked the first rule.
        let mut a = rule(1, 0, "https://a.example");
        a.weight = i32::MAX;
        let mut b = rule(2, 0, "https://b.example");
        b.weight = i32::MAX;
        let rules = [a, b];
        let seen: std::collections::HashSet<String> = (0..200)
            .map(|_| {
                resolve_destination(
                    &rules,
                    &ua("Desktop", "Linux"),
                    &geo(None),
                    None,
                    "https://fb",
                )
            })
            .collect();
        assert_eq!(seen.len(), 2, "both halves of the split must be reachable");

        let tied = [&rules[0], &rules[1]];
        assert_eq!(pick_weighted(&tied, i32::MAX as u64 - 1).id, 1);
        assert_eq!(pick_weighted(&tied, i32::MAX as u64).id, 2);
    }

    #[test]
    fn zero_weights_fall_back_to_the_first_tied_rule() {
        let mut a = rule(1, 0, "https://a.example");
        a.weight = 0;
        let mut b = rule(2, 0, "https://b.example");
        b.weight = 0;
        assert_eq!(
            resolve_destination(
                &[a, b],
                &ua("Desktop", "Linux"),
                &geo(None),
                None,
                "https://fb"
            ),
            "https://a.example"
        );
    }

    #[test]
    fn device_must_be_a_known_class() {
        assert_eq!(
            normalize_device(Some(" mobile ")),
            Ok(Some("Mobile".into()))
        );
        assert_eq!(normalize_device(Some("TABLET")), Ok(Some("Tablet".into())));
        assert_eq!(normalize_device(Some("")), Ok(None));
        assert_eq!(normalize_device(None), Ok(None));
        assert!(normalize_device(Some("phone")).is_err());
    }

    #[test]
    fn country_must_be_alpha2() {
        assert_eq!(normalize_country(Some("us ")), Ok(Some("US".into())));
        assert_eq!(normalize_country(Some("  ")), Ok(None));
        for bad in ["UK1", "Germany", "U", "1A", "é"] {
            assert!(normalize_country(Some(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn lang_must_be_a_primary_subtag() {
        assert_eq!(normalize_lang(Some("DE")), Ok(Some("de".into())));
        assert_eq!(normalize_lang(Some("fil")), Ok(Some("fil".into())));
        // Rules compare against the primary subtag only, so a regional tag
        // could never match.
        for bad in ["pt-BR", "e", "english", "d3"] {
            assert!(normalize_lang(Some(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn os_is_a_short_plain_name() {
        assert_eq!(
            normalize_os(Some(" Windows 10 ")),
            Ok(Some("Windows 10".into()))
        );
        assert_eq!(normalize_os(Some("")), Ok(None));
        assert!(normalize_os(Some(&"x".repeat(33))).is_err());
        assert!(normalize_os(Some("iOS; DROP")).is_err());
    }

    #[test]
    fn possible_destinations_is_certain_without_a_tie() {
        let default = rule(1, 10, "https://web.example");
        let mut mobile = rule(2, 0, "https://app.example");
        mobile.match_device = Some("Mobile".to_string());
        let rules = vec![default, mobile];
        assert_eq!(
            possible_destinations(&rules, &ua("Mobile", "iOS"), &geo(None), None, "https://fb"),
            vec!["https://app.example".to_string()]
        );
        assert_eq!(
            possible_destinations(&[], &ua("Desktop", "Linux"), &geo(None), None, "https://fb"),
            vec!["https://fb".to_string()]
        );
    }

    #[test]
    fn possible_destinations_lists_every_tied_rule_once() {
        let a = rule(1, 0, "https://a.example");
        let b = rule(2, 0, "https://b.example");
        let b_again = rule(3, 0, "https://b.example");
        let worse = rule(4, 5, "https://worse.example");
        assert_eq!(
            possible_destinations(
                &[a, b, b_again, worse],
                &ua("Desktop", "Linux"),
                &geo(None),
                None,
                "https://fb"
            ),
            vec![
                "https://a.example".to_string(),
                "https://b.example".to_string()
            ]
        );
    }
}
