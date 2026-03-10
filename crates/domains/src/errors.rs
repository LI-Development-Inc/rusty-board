//! Domain error types.
//!
//! `DomainError` is the single error type that crosses the port boundary in both
//! directions: adapters map their internal errors (sqlx, aws, etc.) to `DomainError`
//! before returning to services, and services propagate `DomainError` up to handlers
//! where it is mapped to `ApiError`.
//!
//! No adapter-specific error type ever escapes its adapter crate.

use thiserror::Error;

/// The unified error type returned by all port trait methods.
///
/// Services receive this type from ports and may wrap it in their own service-level
/// error enums (e.g. `PostError`, `BoardError`) before returning to handlers.
#[derive(Debug, Error)]
pub enum DomainError {
    /// The requested resource does not exist.
    #[error("not found: {resource}")]
    NotFound {
        /// A human-readable identifier for the missing resource (e.g. `"board/tech"`).
        resource: String,
    },

    /// Input failed a domain validation rule.
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Authentication or authorisation failed.
    #[error("authentication failure")]
    Auth,

    /// Media file processing failed (EXIF strip, thumbnail generation, etc.).
    #[error("media processing error: {reason}")]
    MediaProcessing {
        /// Human-readable description of what went wrong during media processing.
        reason: String,
    },

    /// The poster's IP hash has an active ban.
    #[error("ip is banned: {reason}")]
    Banned {
        /// The ban reason shown to the poster.
        reason: String,
        /// When the ban expires, or `None` for a permanent ban.
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },

    /// Rate limit exceeded for this key.
    #[error("rate limit exceeded; retry after {retry_after_secs}s")]
    RateLimit {
        /// Number of seconds the caller should wait before retrying.
        retry_after_secs: u32,
    },

    /// An unexpected internal error occurred (should never happen in normal operation).
    #[error("internal error: {reason}")]
    Internal {
        /// Internal error detail for logging. Never exposed to end users.
        reason: String,
    },
}

impl DomainError {
    /// Convenience constructor for `NotFound`.
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound { resource: resource.into() }
    }

    /// Convenience constructor for `Internal`.
    pub fn internal(reason: impl Into<String>) -> Self {
        Self::Internal { reason: reason.into() }
    }

    /// Convenience constructor for `MediaProcessing`.
    pub fn media_processing(reason: impl Into<String>) -> Self {
        Self::MediaProcessing { reason: reason.into() }
    }

    /// Convenience constructor for `Auth`.
    pub fn auth() -> Self {
        Self::Auth
    }
}

/// Field-level validation errors raised inside domain models and services.
///
/// Each variant encodes exactly which rule was violated and on which field,
/// giving handlers enough information to produce a user-facing message without
/// knowing anything about the internal validation logic.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// A slug did not match the allowed pattern `^[a-z0-9_-]{1,16}$`.
    #[error("invalid slug '{value}': must match ^[a-z0-9_-]{{1,16}}$")]
    InvalidSlug {
        /// The invalid slug value that was rejected.
        value: String,
    },

    /// A string field exceeded or fell below its allowed length range.
    #[error("field '{field}' length {actual} is outside allowed range {min}..={max}")]
    LengthOutOfRange {
        /// Name of the field that failed the length check.
        field: String,
        /// The actual length of the provided value.
        actual: usize,
        /// Minimum allowed length (inclusive).
        min: usize,
        /// Maximum allowed length (inclusive).
        max: usize,
    },

    /// A numeric value fell outside the allowed range.
    #[error("field '{field}' value {actual} is outside allowed range {min}..={max}")]
    ValueOutOfRange {
        /// Name of the field that failed the range check.
        field: String,
        /// The actual value, formatted as a string for the error message.
        actual: String,
        /// Minimum allowed value, formatted as a string.
        min: String,
        /// Maximum allowed value, formatted as a string.
        max: String,
    },

    /// A MIME type is not accepted by the board configuration.
    #[error("mime type '{mime}' is not allowed on this board")]
    DisallowedMime {
        /// The MIME type that was rejected (e.g. `"video/mp4"`).
        mime: String,
    },

    /// An uploaded file exceeds the board's maximum allowed size.
    #[error("file size {size_kb}KB exceeds board maximum {max_kb}KB")]
    FileTooLarge {
        /// The actual file size in kilobytes.
        size_kb: u32,
        /// The board's configured maximum file size in kilobytes.
        max_kb: u32,
    },

    /// A post body or other text field failed content validation.
    #[error("field '{field}' failed content validation: {reason}")]
    InvalidContent {
        /// Name of the field that failed validation.
        field: String,
        /// Human-readable explanation of why the content was rejected.
        reason: String,
    },

    /// A duplicate post was detected (content hash matches a recent post on this board).
    #[error("duplicate post detected")]
    DuplicatePost,

    /// Username does not meet requirements.
    #[error("invalid username '{value}': {reason}")]
    InvalidUsername {
        /// The username value that was rejected.
        value: String,
        /// Human-readable explanation of why the username was rejected.
        reason: String,
    },

    /// Password does not meet minimum requirements.
    #[error("password does not meet requirements: {reason}")]
    WeakPassword {
        /// Human-readable explanation of which password requirement was not met.
        reason: String,
    },
}
