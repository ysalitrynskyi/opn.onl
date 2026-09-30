use validator::{ValidationError, ValidationErrors};

/// Field-level validation text for an API error body.
///
/// `ValidationErrors`'s `Display` includes the submitted value (`String("…")`).
/// That echoes passwords and other secrets back to the client, so callers use
/// this instead: it names the field and the rule, never the value.
pub fn validation_error_message(errors: &ValidationErrors) -> String {
    let mut parts: Vec<String> = errors
        .field_errors()
        .into_iter()
        .flat_map(|(field, field_errors)| {
            field_errors
                .iter()
                .map(move |err| describe(&field, err))
                .collect::<Vec<_>>()
        })
        .collect();
    parts.sort();
    parts.dedup();
    if parts.is_empty() {
        "Invalid request".to_string()
    } else {
        parts.join("; ")
    }
}

fn describe(field: &str, err: &ValidationError) -> String {
    // "field: detail" is the shape these errors have always had (validator's
    // own Display uses it), and API clients and the e2e suite match on it.
    if let Some(message) = err.message.as_deref() {
        return format!("{field}: {message}");
    }
    let bound = |key: &str| err.params.get(key).and_then(|v| v.as_f64());
    let detail = match err.code.as_ref() {
        "email" => "must be a valid email address".to_string(),
        "url" => "must be a valid URL".to_string(),
        // validator counts characters, not bytes, for `length`.
        "length" => match (bound("min"), bound("max")) {
            (Some(min), Some(max)) => format!("must be {min} to {max} characters"),
            (Some(min), None) => format!("must be at least {min} characters"),
            (None, Some(max)) => format!("must be at most {max} characters"),
            (None, None) => "has the wrong length".to_string(),
        },
        "range" => match (bound("min"), bound("max")) {
            (Some(min), Some(max)) => format!("must be between {min} and {max}"),
            (Some(min), None) => format!("must be at least {min}"),
            (None, Some(max)) => format!("must be at most {max}"),
            (None, None) => "is out of range".to_string(),
        },
        _ => "is invalid".to_string(),
    };
    format!("{field}: {detail}")
}

/// Longest name a folder, tag or organization may have.
pub const MAX_NAME_CHARS: usize = 100;

/// A folder, tag or organization name: not blank and at most
/// `MAX_NAME_CHARS` characters.
pub fn check_name(label: &str, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "{label} must be at most {MAX_NAME_CHARS} characters"
        ));
    }
    Ok(())
}

/// `#rgb` or `#rrggbb`, which is what the colour inputs send. The value is
/// rendered straight into a style attribute, so nothing else is stored.
pub fn check_hex_color(color: &str) -> Result<(), String> {
    let hex = color.strip_prefix('#').unwrap_or_default();
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Color must be a hex color such as #2563eb".to_string())
    }
}

/// An optional colour field: absent or empty clears it, anything else must
/// pass `check_hex_color`.
pub fn check_optional_color(color: Option<&str>) -> Result<(), String> {
    match color {
        None | Some("") => Ok(()),
        Some(color) => check_hex_color(color),
    }
}

/// Organization slug: 2 to 64 lowercase letters, digits, `-` or `_`,
/// starting with a letter or digit.
pub fn check_slug(slug: &str) -> Result<(), String> {
    let valid = (2..=64).contains(&slug.len())
        && slug
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(
            "Slug must be 2 to 64 lowercase letters, digits, hyphens or underscores, starting with a letter or digit"
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[derive(Validate)]
    struct Sample {
        #[validate(email)]
        email: String,
        #[validate(length(min = 8))]
        new_password: String,
        #[validate(length(min = 10, max = 5000))]
        message: String,
        #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
        password: String,
    }

    #[test]
    fn message_names_the_field_and_rule_but_not_the_submitted_value() {
        let sample = Sample {
            email: "not-an-email".to_string(),
            new_password: "short".to_string(),
            message: "tiny".to_string(),
            password: "pw".to_string(),
        };
        let errors = sample.validate().unwrap_err();
        let message = validation_error_message(&errors);
        for value in ["not-an-email", "short", "tiny", "\"pw\"", "String("] {
            assert!(
                !message.contains(value),
                "{value:?} leaked into {message:?}"
            );
        }
        assert!(
            message.contains("email: must be a valid email address"),
            "{message}"
        );
        assert!(
            message.contains("new_password: must be at least 8 characters"),
            "{message}"
        );
        assert!(
            message.contains("message: must be 10 to 5000 characters"),
            "{message}"
        );
        assert!(
            message.contains("password: Password must be at least 8 characters"),
            "{message}"
        );
    }

    #[test]
    fn names_colors_and_slugs() {
        assert!(check_name("Tag name", "launch").is_ok());
        assert!(check_name("Tag name", "   ").is_err());
        assert!(check_name("Tag name", &"x".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(check_name("Tag name", &"x".repeat(MAX_NAME_CHARS + 1)).is_err());

        for ok in ["#fff", "#2563EB", "#e11d48"] {
            assert!(check_hex_color(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "red",
            "#12345",
            "#gggggg",
            "2563eb",
            "#2563eb; background:url(x)",
        ] {
            assert!(check_hex_color(bad).is_err(), "{bad}");
        }

        for ok in ["acme", "org-test_1a2b", "a1"] {
            assert!(check_slug(ok).is_ok(), "{ok}");
        }
        for bad in ["a", "Acme", "-acme", "acme corp", "<b>", &"a".repeat(65)] {
            assert!(check_slug(bad).is_err(), "{bad}");
        }
    }
}
