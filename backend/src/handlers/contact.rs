use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::AppState;
use crate::utils::html_escape;
use crate::utils::validation::validation_error_message;

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ContactRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1, max = 100))]
    pub subject: String,
    #[validate(length(min = 10, max = 5000))]
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContactResponse {
    pub success: bool,
    pub message: String,
}

/// Send a contact form message to admin
#[utoipa::path(
    post,
    path = "/contact",
    request_body = ContactRequest,
    responses(
        (status = 200, description = "Message sent successfully", body = ContactResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Failed to send message"),
        (status = 503, description = "Email is not configured on this instance; nothing was sent"),
    ),
    tag = "Contact"
)]
pub async fn send_contact_message(
    State(state): State<AppState>,
    Json(payload): Json<ContactRequest>,
) -> impl IntoResponse {
    // Validate input
    if let Err(e) = payload.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ContactResponse {
                success: false,
                message: validation_error_message(&e),
            }),
        )
            .into_response();
    }

    // Without a working mail transport the message has nowhere to go: it is
    // not stored anywhere. Answering "sent" dropped it silently while telling
    // the visitor it had arrived, so say it was not delivered instead.
    let email_service = match &state.email_service {
        Some(service) if service.is_configured() => service,
        _ => {
            tracing::warn!("Contact form submission refused: email is not configured");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ContactResponse {
                    success: false,
                    message: "This site cannot send email right now, so your message was not \
                              delivered. Please reach the operator another way."
                        .to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get admin email from environment
    let admin_email = std::env::var("ADMIN_EMAIL").unwrap_or_else(|_| {
        std::env::var("SMTP_FROM_EMAIL").unwrap_or_else(|_| "admin@opn.onl".to_string())
    });

    // Build email content
    let subject = format!("[opn.onl Contact] {}: {}", payload.subject, payload.name);
    let html_body = format!(
        r#"
        <h2>New Contact Form Submission</h2>
        <p><strong>From:</strong> {} &lt;{}&gt;</p>
        <p><strong>Subject:</strong> {}</p>
        <hr>
        <h3>Message:</h3>
        <p style="white-space: pre-wrap;">{}</p>
        <hr>
        <p style="color: #666; font-size: 12px;">
            This message was sent via the opn.onl contact form.
            <br>Reply directly to this email to respond to the sender.
        </p>
        "#,
        html_escape(&payload.name),
        html_escape(&payload.email),
        html_escape(&payload.subject),
        html_escape(&payload.message),
    );

    let text_body = format!(
        "New contact form submission\n\nFrom: {} <{}>\nSubject: {}\n\n{}\n\n\
         This message was sent via the opn.onl contact form.\n\
         Reply directly to this email to respond to the sender.\n",
        payload.name, payload.email, payload.subject, payload.message,
    );
    let body = crate::utils::email::EmailBody::from_parts(html_body, text_body);

    // Send email to admin
    match email_service
        .send_email_with_reply_to(&admin_email, &subject, &body, &payload.email)
        .await
    {
        Ok(_) => {
            // Don't log the sender's name/email — that is PII sitting in log
            // aggregation. The message was delivered to the admin inbox already.
            tracing::info!("Contact form message forwarded to admin inbox");
            (
                StatusCode::OK,
                Json(ContactResponse {
                    success: true,
                    message: "Message sent successfully. We'll get back to you soon.".to_string(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to send contact form email: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ContactResponse {
                    success: false,
                    message: "Failed to send message. Please try again later or email us directly."
                        .to_string(),
                }),
            )
                .into_response()
        }
    }
}
