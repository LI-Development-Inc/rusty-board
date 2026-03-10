//! `ApiError` — the unified HTTP error type for all handlers.
//!
//! All service and domain errors are mapped to `ApiError` at the handler boundary.
//! `ApiError` knows how to render itself as an HTTP response with an appropriate
//! status code and JSON body.

use domains::errors::DomainError;
use serde::Serialize;
use thiserror::Error;

/// A structured error body returned by all API endpoints.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Short machine-readable error code, e.g. `"NOT_FOUND"` or `"RATE_LIMITED"`.
    pub error:   String,
    /// Human-readable description of the error, safe to display to end users.
    pub message: String,
    /// Optional structured details (e.g. validation field errors). Omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// The unified API error type.
///
/// Handlers return `Result<T, ApiError>`. `ApiError` implements Axum's
/// `IntoResponse` (in the `axum` feature) so it can be returned from handlers directly.
#[derive(Debug, Error)]
pub enum ApiError {
    /// 400 Bad Request — the request was malformed or failed validation.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// 401 Unauthorized — no valid authentication token was provided.
    #[error("unauthorized")]
    Unauthorized,

    /// 403 Forbidden — the authenticated user lacks permission.
    #[error("forbidden")]
    Forbidden,

    /// 404 Not Found — the requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// 409 Conflict — the request conflicts with existing state.
    #[error("conflict: {0}")]
    Conflict(String),

    /// 422 Unprocessable Entity — semantic validation failed.
    #[error("validation error: {0}")]
    UnprocessableEntity(String),

    /// 429 Too Many Requests — rate limit exceeded.
    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited {
        /// Number of seconds the client should wait before retrying.
        retry_after_secs: u32,
    },

    /// 403 Banned — the poster's IP is banned.
    #[error("banned: {reason}")]
    Banned {
        /// The ban reason shown to the poster.
        reason:     String,
        /// When the ban expires, or `None` for a permanent ban.
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },

    /// 500 Internal Server Error — an unexpected error occurred.
    #[error("internal server error")]
    Internal(String),

    /// 400 Bad Request with a specific user-facing message.
    #[error("validation: {message}")]
    Validation {
        /// Human-readable description of what failed.
        message: String,
    },

    /// 501 Not Implemented — the endpoint exists but has not been wired up yet.
    #[error("not implemented")]
    NotImplemented,
}

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::NotFound { resource } => ApiError::NotFound(resource),
            DomainError::Validation(v)          => ApiError::UnprocessableEntity(v.to_string()),
            DomainError::Auth                   => ApiError::Unauthorized,
            DomainError::MediaProcessing { reason } => ApiError::UnprocessableEntity(reason),
            DomainError::Banned { reason, expires_at } => ApiError::Banned { reason, expires_at },
            DomainError::RateLimit { retry_after_secs } => ApiError::RateLimited { retry_after_secs },
            DomainError::Internal { reason }    => ApiError::Internal(reason),
        }
    }
}

impl From<services::board::BoardError> for ApiError {
    fn from(e: services::board::BoardError) -> Self {
        match e {
            services::board::BoardError::NotFound { slug } => ApiError::NotFound(slug),
            services::board::BoardError::InvalidSlug { slug } => {
                ApiError::BadRequest(format!("invalid slug: {slug}"))
            }
            services::board::BoardError::SlugConflict { slug } => {
                ApiError::Conflict(format!("board with slug '{slug}' already exists"))
            }
            services::board::BoardError::Internal(d) => ApiError::from(d),
        }
    }
}

impl From<services::post::PostError> for ApiError {
    fn from(e: services::post::PostError) -> Self {
        match e {
            services::post::PostError::Banned { reason, expires_at } => {
                ApiError::Banned { reason, expires_at }
            }
            services::post::PostError::RateLimited { retry_after_secs } => {
                ApiError::RateLimited { retry_after_secs }
            }
            services::post::PostError::SpamDetected { .. } => {
                ApiError::UnprocessableEntity("post rejected by spam filter".to_owned())
            }
            services::post::PostError::DuplicatePost => {
                ApiError::UnprocessableEntity("duplicate post detected".to_owned())
            }
            services::post::PostError::Validation { reason } => {
                ApiError::UnprocessableEntity(reason)
            }
            services::post::PostError::ThreadNotFound { id } => ApiError::NotFound(id),
            services::post::PostError::ThreadClosed => {
                ApiError::UnprocessableEntity("thread is closed".to_owned())
            }
            services::post::PostError::MediaError { reason } => {
                ApiError::UnprocessableEntity(reason)
            }
            services::post::PostError::Internal(d) => ApiError::from(d),
        }
    }
}

impl From<services::user::UserError> for ApiError {
    fn from(e: services::user::UserError) -> Self {
        match e {
            services::user::UserError::NotFound { .. }      => ApiError::Unauthorized,
            services::user::UserError::Validation { reason } => ApiError::BadRequest(reason),
            services::user::UserError::InvalidCredentials   => ApiError::Unauthorized,
            services::user::UserError::Deactivated          => ApiError::Forbidden,
            services::user::UserError::Internal(d)          => ApiError::from(d),
        }
    }
}

impl From<services::moderation::ModerationError> for ApiError {
    fn from(e: services::moderation::ModerationError) -> Self {
        match e {
            services::moderation::ModerationError::NotFound { resource } => {
                ApiError::NotFound(resource)
            }
            services::moderation::ModerationError::PermissionDenied => ApiError::Forbidden,
            services::moderation::ModerationError::Internal(d) => ApiError::from(d),
        }
    }
}

impl From<services::thread::ThreadError> for ApiError {
    fn from(e: services::thread::ThreadError) -> Self {
        match e {
            services::thread::ThreadError::NotFound { id } => ApiError::NotFound(id),
            services::thread::ThreadError::Closed { id } => {
                ApiError::UnprocessableEntity(format!("thread {id} is closed"))
            }
            services::thread::ThreadError::Internal(d) => ApiError::from(d),
        }
    }
}

impl From<services::staff_request::StaffRequestError> for ApiError {
    fn from(e: services::staff_request::StaffRequestError) -> Self {
        match e {
            services::staff_request::StaffRequestError::NotFound { id } => {
                ApiError::NotFound(id)
            }
            services::staff_request::StaffRequestError::Validation { reason } => {
                ApiError::BadRequest(reason)
            }
            services::staff_request::StaffRequestError::NotPending => {
                ApiError::Conflict("request is not in pending state".to_owned())
            }
            services::staff_request::StaffRequestError::PermissionDenied => ApiError::Forbidden,
            services::staff_request::StaffRequestError::Internal(d) => ApiError::from(d),
        }
    }
}

impl From<services::staff_message::StaffMessageError> for ApiError {
    fn from(e: services::staff_message::StaffMessageError) -> Self {
        match e {
            services::staff_message::StaffMessageError::NotFound { id } => {
                ApiError::NotFound(id)
            }
            services::staff_message::StaffMessageError::Validation { reason } => {
                ApiError::BadRequest(reason)
            }
            services::staff_message::StaffMessageError::PermissionDenied { reason: _ } => {
                ApiError::Forbidden
            }
            services::staff_message::StaffMessageError::Internal(d) => ApiError::from(d),
        }
    }
}
