//! Error type for `StaffMessageService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `StaffMessageService` methods.
#[derive(Debug, Error)]
pub enum StaffMessageError {
    /// The specified message does not exist.
    #[error("message not found: {id}")]
    NotFound {
        /// The ID of the message that was not found.
        id: String,
    },

    /// The message body failed validation (empty, too long, etc.).
    #[error("validation failed: {reason}")]
    Validation {
        /// Human-readable description of the validation failure.
        reason: String,
    },

    /// The caller is not authorised to send to the given recipient
    /// (e.g. a board owner trying to message a non-volunteer).
    #[error("permission denied: {reason}")]
    PermissionDenied {
        /// Why the operation was denied.
        reason: String,
    },

    /// A domain-level error that could not be handled at this level.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
