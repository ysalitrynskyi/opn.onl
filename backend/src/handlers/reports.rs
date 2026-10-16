use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

use crate::AppState;
use crate::entity::{abuse_reports, links};
use crate::handlers::json_error;
use crate::handlers::links::optional_user_id_from_header;
use crate::utils::email_domain_policy::normalize_email;
use crate::utils::validation::validation_error_message;

/// Longest short code a report will accept. Alias length is an unclamped
/// env var, so this stays well above any code the redirect route can store.
const MAX_CODE_CHARS: u64 = 2048;
const MAX_DETAILS_CHARS: u64 = 2000;
const MAX_EMAIL_CHARS: u64 = 254;

fn validate_reason(reason: &str) -> Result<(), ValidationError> {
    match reason {
        "phishing" | "malware" | "spam" | "illegal" | "other" => Ok(()),
        _ => {
            let mut error = ValidationError::new("reason");
            error.message = Some("must be phishing, malware, spam, illegal, or other".into());
            Err(error)
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReportRequest {
    /// Short code, without a leading slash.
    pub code: String,
    /// phishing, malware, spam, illegal, or other.
    pub reason: String,
    /// Optional note, at most 2000 characters.
    pub details: Option<String>,
    /// Optional address the admin can reply to. Not required.
    pub email: Option<String>,
}

/// Fields after trim and email normalisation. Validated here so a blank
/// email or a blank note is "absent", not a failed email or length check.
#[derive(Debug, Validate)]
struct ReportFields {
    #[validate(length(min = 1, max = MAX_CODE_CHARS))]
    code: String,
    #[validate(custom(function = "validate_reason"))]
    reason: String,
    #[validate(length(max = MAX_DETAILS_CHARS))]
    details: Option<String>,
    #[validate(email, length(max = MAX_EMAIL_CHARS))]
    email: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReportReceived {
    pub status: String,
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// Accept a report of a live short link. Public: no account is required.
/// A present `Authorization` header that does not authenticate is a 401,
/// never a quiet anonymous success — the caller believes they are signed in.
/// The report itself stores no user id, no IP address and no user agent.
#[utoipa::path(
    post,
    path = "/report",
    request_body = ReportRequest,
    responses(
        (status = 201, description = "Report stored", body = ReportReceived),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Authorization header present but not valid"),
        (status = 404, description = "No live link for that code"),
    ),
    tag = "Reports",
    security((), ("bearer_auth" = []), ("api_key" = []))
)]
pub async fn submit_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReportRequest>,
) -> Response {
    // Discard the user id on purpose. The check still has to run: a bad
    // Authorization header must not be treated as an anonymous report.
    if let Err(rejected) = optional_user_id_from_header(&state.db, &headers).await {
        return rejected.into_response();
    }

    let fields = ReportFields {
        code: payload.code.trim().to_string(),
        reason: payload.reason.trim().to_string(),
        details: blank_to_none(payload.details),
        email: blank_to_none(payload.email).map(|email| normalize_email(&email)),
    };
    if let Err(errors) = fields.validate() {
        return json_error(StatusCode::BAD_REQUEST, validation_error_message(&errors));
    }

    // Unknown and soft-deleted codes are the same 404. A live but expired,
    // inactive or owner-disabled link can still be reported: it exists.
    let link = match links::Entity::find()
        .filter(links::Column::Code.eq(&fields.code))
        .filter(links::Column::DeletedAt.is_null())
        .one(&state.db)
        .await
    {
        Ok(Some(link)) => link,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "Link not found"),
        Err(error) => {
            tracing::error!("abuse report lookup failed: {error}");
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not save the report",
            );
        }
    };

    let report = abuse_reports::ActiveModel {
        code: Set(fields.code.clone()),
        link_id: Set(Some(link.id)),
        // Freeze the fallback the reporter could have been sent to. A later
        // edit of the link must not erase it from the queue.
        destination_url: Set(link.original_url.clone()),
        reason: Set(fields.reason.clone()),
        details: Set(fields.details),
        reporter_email: Set(fields.email),
        status: Set("open".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    };

    match report.insert(&state.db).await {
        Ok(_) => {
            // No email, no IP, no details: those are the report, not a log line.
            tracing::info!(code = %fields.code, reason = %fields.reason, "abuse report stored");
            (
                StatusCode::CREATED,
                Json(ReportReceived {
                    status: "received".to_string(),
                }),
            )
                .into_response()
        }
        Err(error) => {
            tracing::error!("abuse report insert failed: {error}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not save the report",
            )
        }
    }
}
