//! Error type for `BoardService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `BoardService` methods.
#[derive(Debug, Error)]
pub enum BoardError {
    /// The requested board does not exist.
    #[error("board not found: {slug}")]
    NotFound {
        /// The slug of the board that was not found.
        slug: String,
    },

    /// The slug provided is not valid.
    #[error("invalid slug '{slug}': must match ^[a-z0-9_-]{{1,16}}$")]
    InvalidSlug {
        /// The slug value that failed validation.
        slug: String,
    },

    /// A board with this slug already exists.
    #[error("board with slug '{slug}' already exists")]
    SlugConflict {
        /// The slug that collided with an existing board.
        slug: String,
    },

    /// A domain-level error that could not be handled at the board service level.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
