//! # AppError
//! 
//! Centralized error handling for the Rusty-Board ecosystem.
//! Maps domain-specific failures to actionable error types.

use thiserror::Error;

/// The primary error type for all rb-core operations.
#[derive(Error, Debug)]
pub enum AppError {
    /// Resource not found (e.g., Board, Thread, Post)
    #[error("{0} not found with ID {1}")]
    NotFound(String, String),

    /// Validation failure (e.g., post too long, invalid file type)
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Security/Auth failure (e.g., banned, invalid admin credentials)
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Infrastructure failure (e.g., DB down, S3 timeout)
    #[error("internal service error: {0}")]
    Internal(String),

    /// Resource already exists (e.g., duplicate board slug)
    #[error("conflict: {0}")]
    Conflict(String),
    
    /// Rate limit exceeded
    #[error("too many requests: {0}")]
    RateLimitExceeded(String),
}

/// A specialized Result type for Rusty-Board logic.
pub type Result<T> = std::result::Result<T, AppError>;