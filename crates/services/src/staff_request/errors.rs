//! Error type for `StaffRequestService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `StaffRequestService` methods.
#[derive(Debug, Error)]
pub enum StaffRequestError {
    /// No request with the given ID exists.
    #[error("staff request not found: {id}")]
    NotFound {
        /// The ID of the request that was not found.
        id: String,
    },

    /// The request cannot be submitted due to invalid input.
    #[error("validation failed: {reason}")]
    Validation {
        /// Human-readable description of the validation failure.
        reason: String,
    },

    /// The request is in a state that does not allow the attempted operation
    /// (e.g. approving an already-approved request).
    #[error("request is not in pending state")]
    NotPending,

    /// The caller is not authorised to review this request type.
    #[error("permission denied")]
    PermissionDenied,

    /// A domain-level error that could not be handled at this level.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
