//! Error type for `ModerationService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `ModerationService` methods.
#[derive(Debug, Error)]
pub enum ModerationError {
    /// The target entity (post, thread, user, ban, flag) was not found.
    #[error("not found: {resource}")]
    NotFound {
        /// A human-readable identifier for the missing resource.
        resource: String,
    },

    /// The operation requires a permission level the actor does not have.
    #[error("permission denied")]
    PermissionDenied,

    /// A domain-level error that could not be handled at this level.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
