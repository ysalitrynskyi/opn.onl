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
    if let Some(message) = err.message.as_deref() {
        return message.to_string();
    }
    let name = field_label(field);
    let bound = |key: &str| err.params.get(key).and_then(|v| v.as_f64());
    match err.code.as_ref() {
        "email" => format!("{name} must be a valid email address"),
        "url" => format!("{name} must be a valid URL"),
        // validator counts characters, not bytes, for `length`.
        "length" => match (bound("min"), bound("max")) {
            (Some(min), Some(max)) => format!("{name} must be {min} to {max} characters"),
            (Some(min), None) => format!("{name} must be at least {min} characters"),
            (None, Some(max)) => format!("{name} must be at most {max} characters"),
            (None, None) => format!("{name} has the wrong length"),
        },
        "range" => match (bound("min"), bound("max")) {
            (Some(min), Some(max)) => format!("{name} must be between {min} and {max}"),
            (Some(min), None) => format!("{name} must be at least {min}"),
            (None, Some(max)) => format!("{name} must be at most {max}"),
            (None, None) => format!("{name} is out of range"),
        },
        _ => format!("{name} is invalid"),
    }
}

/// `new_password` → `New password`.
fn field_label(field: &str) -> String {
    let spaced = field.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Field".to_string(),
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
            message.contains("Email must be a valid email address"),
            "{message}"
        );
        assert!(
            message.contains("New password must be at least 8 characters"),
            "{message}"
        );
        assert!(
            message.contains("Message must be 10 to 5000 characters"),
            "{message}"
        );
        assert!(
            message.contains("Password must be at least 8 characters"),
            "{message}"
        );
    }
}
