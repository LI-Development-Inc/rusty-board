//! Axum `IntoResponse` implementation for `ApiError`.

use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::common::errors::{ApiError, ErrorBody};

/// Extension trait for easy result conversion in handlers.
pub mod into_response_ext {
    /// Extension trait for converting `Result<T, E>` into `Result<T, ApiError>` in handlers.
    pub trait IntoApiResponse<T> {
        /// Convert this result into an `ApiError`-bearing result for use as a handler return type.
        fn api_response(self) -> Result<T, super::ApiError>;
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone())
            }
            ApiError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "authentication required".to_owned())
            }
            ApiError::Forbidden => {
                (StatusCode::FORBIDDEN, "FORBIDDEN", "permission denied".to_owned())
            }
            ApiError::NotFound(resource) => {
                (StatusCode::NOT_FOUND, "NOT_FOUND", format!("not found: {resource}"))
            }
            ApiError::Conflict(msg) => {
                (StatusCode::CONFLICT, "CONFLICT", msg.clone())
            }
            ApiError::UnprocessableEntity(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR", msg.clone())
            }
            ApiError::RateLimited { retry_after_secs } => {
                let mut resp = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorBody {
                        error:   "RATE_LIMITED".to_owned(),
                        message: format!("rate limit exceeded; retry after {retry_after_secs}s"),
                        details: None,
                    }),
                )
                    .into_response();
                resp.headers_mut().insert(
                    axum::http::header::RETRY_AFTER,
                    HeaderValue::from_str(&retry_after_secs.to_string())
                        .unwrap_or(HeaderValue::from_static("60")),
                );
                return resp;
            }
            ApiError::Banned { reason, expires_at } => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorBody {
                        error:   "BANNED".to_owned(),
                        message: format!("you are banned: {reason}"),
                        details: Some(serde_json::json!({ "expires_at": expires_at })),
                    }),
                )
                    .into_response();
            }
            ApiError::Internal(msg) => {
                tracing::error!(error = %msg, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "an unexpected error occurred".to_owned(),
                )
            }
            ApiError::Validation { message } => {
                (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", message.clone())
            }
            ApiError::NotImplemented => {
                (StatusCode::NOT_IMPLEMENTED, "NOT_IMPLEMENTED", "not implemented yet".to_owned())
            }
        };

        (
            status,
            Json(ErrorBody {
                error:   code.to_owned(),
                message,
                details: None,
            }),
        )
            .into_response()
    }
}
