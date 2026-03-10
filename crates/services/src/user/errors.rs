//! Error type for `UserService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `UserService` methods.
#[derive(Debug, Error)]
pub enum UserError {
    /// No user with the given identifier exists.
    #[error("user not found: {id}")]
    NotFound {
        /// The UUID of the user that was not found, as a string.
        id: String,
    },

    /// Username or password validation failed.
    #[error("validation failed: {reason}")]
    Validation {
        /// Human-readable description of which validation rule was violated.
        reason: String,
    },

    /// Login credentials are invalid (username or password wrong).
    ///
    /// The error message is deliberately vague to prevent username enumeration.
    #[error("invalid username or password")]
    InvalidCredentials,

    /// The account has been deactivated.
    #[error("account is deactivated")]
    Deactivated,

    /// A domain-level error that could not be handled at this level.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
