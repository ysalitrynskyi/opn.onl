//! Reading settings from the environment.
//!
//! Compose passes every optional setting as `NAME: ${NAME:-}`, so an option the
//! operator left alone arrives as an empty string rather than as an unset
//! variable. Blank therefore has to mean "not set" wherever a setting is
//! optional, and an on/off switch has to accept the spellings an operator
//! actually types (`False`, `0`, `no`), not just a lower-case `false`.

use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

/// The value of `name`, or `None` when it is unset or blank. A non-blank value
/// is returned as written: a password with a trailing space stays intact.
pub fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Parse an on/off spelling: `true`/`false`, `1`/`0`, `yes`/`no`, `on`/`off`,
/// in any case, with surrounding whitespace ignored.
pub fn parse_flag(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// An on/off switch. Unset or blank gives `default`. A value that is not one of
/// the spellings [`parse_flag`] knows also gives `default`, and is reported
/// once per process: guessing the operator's meaning could switch a protection
/// off, and a log line on every request would drown the real ones.
pub fn env_flag(name: &str, default: bool) -> bool {
    let Some(raw) = env_nonempty(name) else {
        return default;
    };
    parse_flag(&raw).unwrap_or_else(|| {
        if first_report(name, &raw) {
            tracing::warn!(
                "{name}={raw:?} is not a recognised on/off value (use true or false); using the default, {default}"
            );
        }
        default
    })
}

/// A numeric setting. Unset or blank gives `default`; so does a value that does
/// not parse, which is reported once per process.
pub fn env_parse<T: FromStr + Display>(name: &str, default: T) -> T {
    let Some(raw) = env_nonempty(name) else {
        return default;
    };
    raw.trim().parse().unwrap_or_else(|_| {
        if first_report(name, &raw) {
            tracing::warn!("{name}={raw:?} is not a valid value; using the default, {default}");
        }
        default
    })
}

/// `configured` with surrounding whitespace and trailing slashes removed.
/// `None` when nothing is left (`""`, `"/"`, `"   "`), which callers treat as unset.
///
/// Short links are `{base}/{code}`. A base of `https://opn.onl/` would make
/// `https://opn.onl//code`, which the frontend proxy does not route as a code.
pub fn normalize_public_base(configured: &str) -> Option<String> {
    let trimmed = configured.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// [`normalize_public_base`] of `configured`, or of `default` when that is
/// missing or blank. Blank means unset, the same rule as [`env_nonempty`].
pub fn public_base_or(configured: Option<&str>, default: &str) -> String {
    configured
        .and_then(normalize_public_base)
        .or_else(|| normalize_public_base(default))
        .unwrap_or_default()
}

/// `FRONTEND_URL` without a trailing slash. Unset or blank is the dev SPA.
pub fn frontend_base_url() -> String {
    let configured = std::env::var("FRONTEND_URL").ok();
    public_base_or(configured.as_deref(), "http://localhost:5173")
}

/// `BASE_URL` without a trailing slash. Unset or blank is the dev API.
pub fn api_base_url() -> String {
    let configured = std::env::var("BASE_URL").ok();
    public_base_or(configured.as_deref(), "http://localhost:3000")
}

/// True the first time `name` is seen with `value` in this process.
fn first_report(name: &str, value: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    SEEN.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((name.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        env_flag, env_nonempty, env_parse, first_report, normalize_public_base, parse_flag,
        public_base_or,
    };
    use crate::utils::test_env::{remove_env, set_env};

    // Each test uses variable names of its own: the environment is process
    // wide and the tests run in parallel.

    #[test]
    fn public_base_drops_trailing_slashes_and_blank_means_the_default() {
        assert_eq!(
            public_base_or(Some("https://opn.onl/"), "http://localhost:5173"),
            "https://opn.onl"
        );
        assert_eq!(
            public_base_or(Some("https://opn.onl///"), "http://localhost:5173"),
            "https://opn.onl"
        );
        assert_eq!(
            public_base_or(Some("  https://opn.onl/app/  "), "http://localhost:5173"),
            "https://opn.onl/app"
        );
        assert_eq!(
            public_base_or(Some("https://opn.onl"), "http://localhost:5173"),
            "https://opn.onl"
        );
        assert_eq!(
            public_base_or(None, "http://localhost:5173/"),
            "http://localhost:5173"
        );
        assert_eq!(
            public_base_or(Some(""), "http://localhost:5173/"),
            "http://localhost:5173"
        );
        assert_eq!(
            public_base_or(Some("   "), "http://localhost:5173"),
            "http://localhost:5173"
        );
        assert_eq!(
            public_base_or(Some("/"), "http://localhost:5173"),
            "http://localhost:5173"
        );
        assert_eq!(
            normalize_public_base("https://opn.onl/"),
            Some("https://opn.onl".to_string())
        );
        assert_eq!(normalize_public_base(" / "), None);
        assert_eq!(normalize_public_base(""), None);
    }

    #[test]
    fn flag_spellings_are_case_insensitive_and_trimmed() {
        for on in ["true", "TRUE", "True", "1", "yes", "YES", "on", " true "] {
            assert_eq!(parse_flag(on), Some(true), "{on:?}");
        }
        for off in ["false", "FALSE", "False", "0", "no", "No", "off", " off\n"] {
            assert_eq!(parse_flag(off), Some(false), "{off:?}");
        }
        for junk in ["", "  ", "maybe", "2", "disabled", "tru"] {
            assert_eq!(parse_flag(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn flag_defaults_apply_when_unset_blank_or_garbage() {
        remove_env("OPN_TEST_FLAG_UNSET");
        assert!(env_flag("OPN_TEST_FLAG_UNSET", true));
        assert!(!env_flag("OPN_TEST_FLAG_UNSET", false));

        set_env("OPN_TEST_FLAG_BLANK", "   ");
        assert!(env_flag("OPN_TEST_FLAG_BLANK", true));
        assert!(!env_flag("OPN_TEST_FLAG_BLANK", false));

        set_env("OPN_TEST_FLAG_GARBAGE", "perhaps");
        assert!(env_flag("OPN_TEST_FLAG_GARBAGE", true));
        assert!(!env_flag("OPN_TEST_FLAG_GARBAGE", false));
    }

    #[test]
    fn flag_value_overrides_the_default_in_both_directions() {
        // Every spelling an operator types for a kill switch that is on by
        // default must turn it off; the old exact "false" check missed all but
        // one of them.
        for off in ["false", "False", "FALSE", "0", "no", "off"] {
            set_env("OPN_TEST_FLAG_OFF", off);
            assert!(!env_flag("OPN_TEST_FLAG_OFF", true), "{off}");
        }
        // And an opt-in feature must come on for `True` and `1`, which the
        // lower-case-only parse left off.
        for on in ["true", "True", "TRUE", "1", "yes", "on"] {
            set_env("OPN_TEST_FLAG_ON", on);
            assert!(env_flag("OPN_TEST_FLAG_ON", false), "{on}");
        }
    }

    #[test]
    fn nonempty_treats_blank_as_unset_and_keeps_the_value_as_written() {
        remove_env("OPN_TEST_NONEMPTY");
        assert_eq!(env_nonempty("OPN_TEST_NONEMPTY"), None);
        set_env("OPN_TEST_NONEMPTY", "");
        assert_eq!(env_nonempty("OPN_TEST_NONEMPTY"), None);
        set_env("OPN_TEST_NONEMPTY", " \t ");
        assert_eq!(env_nonempty("OPN_TEST_NONEMPTY"), None);
        set_env("OPN_TEST_NONEMPTY", "secret ");
        assert_eq!(
            env_nonempty("OPN_TEST_NONEMPTY").as_deref(),
            Some("secret ")
        );
    }

    #[test]
    fn parse_uses_the_default_for_blank_and_garbage() {
        remove_env("OPN_TEST_PARSE");
        assert_eq!(env_parse("OPN_TEST_PARSE", 5usize), 5);
        set_env("OPN_TEST_PARSE", "");
        assert_eq!(env_parse("OPN_TEST_PARSE", 5usize), 5);
        set_env("OPN_TEST_PARSE", "many");
        assert_eq!(env_parse("OPN_TEST_PARSE", 5usize), 5);
        set_env("OPN_TEST_PARSE", " 12 ");
        assert_eq!(env_parse("OPN_TEST_PARSE", 5usize), 12);
    }

    #[test]
    fn garbage_is_reported_once_per_value() {
        assert!(first_report("OPN_TEST_REPORT", "maybe"));
        assert!(!first_report("OPN_TEST_REPORT", "maybe"));
        // A different value is a different mistake worth one more line.
        assert!(first_report("OPN_TEST_REPORT", "perhaps"));
    }
}
