//! Error type for `ThreadService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `ThreadService` methods.
#[derive(Debug, Error)]
pub enum ThreadError {
    /// The requested thread does not exist.
    #[error("thread not found: {id}")]
    NotFound {
        /// The UUID of the thread that was not found, as a string.
        id: String,
    },

    /// The thread is closed and does not accept new posts.
    #[error("thread {id} is closed")]
    Closed {
        /// The UUID of the closed thread, as a string.
        id: String,
    },

    /// A domain-level error that could not be handled at this level.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
