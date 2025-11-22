use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::AppState;

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
                message: format!("Validation error: {}", e),
            }),
        ).into_response();
    }

    // Check if email service is available
    let email_service = match &state.email_service {
        Some(service) => service,
        None => {
            tracing::warn!("Contact form submission received but email service not configured");
            // Return success anyway - we can log it or store it
            return (
                StatusCode::OK,
                Json(ContactResponse {
                    success: true,
                    message: "Message received. We'll get back to you soon.".to_string(),
                }),
            ).into_response();
        }
    };

    // Get admin email from environment
    let admin_email = std::env::var("ADMIN_EMAIL")
        .unwrap_or_else(|_| std::env::var("SMTP_FROM_EMAIL").unwrap_or_else(|_| "admin@opn.onl".to_string()));

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

    // Send email to admin
    match email_service.send_email_with_reply_to(&admin_email, &subject, &html_body, &payload.email).await {
        Ok(_) => {
            tracing::info!("Contact form message sent from {} <{}>", payload.name, payload.email);
            (
                StatusCode::OK,
                Json(ContactResponse {
                    success: true,
                    message: "Message sent successfully. We'll get back to you soon.".to_string(),
                }),
            ).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to send contact form email: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ContactResponse {
                    success: false,
                    message: "Failed to send message. Please try again later or email us directly.".to_string(),
                }),
            ).into_response()
        }
    }
}

/// Simple HTML escape for security
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

