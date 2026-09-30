use validator::ValidationErrors;

/// Field-level validation text for an API error body.
///
/// `ValidationErrors`'s `Display` includes the submitted value (`String("…")`).
/// That echoes passwords and other secrets back to the client, so callers use
/// this instead and keep only the field name and the kind of failure.
pub fn validation_error_message(errors: &ValidationErrors) -> String {
    let mut parts = Vec::new();
    for (field, field_errors) in errors.field_errors() {
        for err in field_errors {
            let detail = if let Some(message) = err.message.as_deref() {
                message.to_string()
            } else {
                match err.code.as_ref() {
                    "email" => "must be an email address".to_string(),
                    "length" => "does not meet the length requirement".to_string(),
                    "range" => "is out of range".to_string(),
                    "url" => "must be a URL".to_string(),
                    other => format!("is invalid ({other})"),
                }
            };
            parts.push(format!("{field}: {detail}"));
        }
    }
    parts.sort();
    if parts.is_empty() {
        "Invalid request".to_string()
    } else {
        parts.join("; ")
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
    }

    #[test]
    fn message_names_the_field_and_not_the_submitted_value() {
        let sample = Sample {
            email: "not-an-email".to_string(),
            new_password: "short".to_string(),
        };
        let errors = sample.validate().unwrap_err();
        let message = validation_error_message(&errors);
        assert!(!message.contains("not-an-email"));
        assert!(!message.contains("short"));
        assert!(!message.contains("String("));
        assert!(message.contains("email: must be an email address"));
        assert!(message.contains("new_password: does not meet the length requirement"));
    }
}
