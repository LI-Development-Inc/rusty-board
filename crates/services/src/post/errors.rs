//! Error type for `PostService` operations.

use domains::errors::DomainError;
use thiserror::Error;

/// Errors that can occur in `PostService::create_post`.
#[derive(Debug, Error)]
pub enum PostError {
    /// The poster's IP hash has an active ban.
    #[error("ip is banned: {reason}")]
    Banned {
        /// The ban reason shown to the poster.
        reason: String,
        /// When the ban expires, or `None` for a permanent ban.
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },

    /// The poster has exceeded the board's rate limit.
    #[error("rate limit exceeded; retry after {retry_after_secs}s")]
    RateLimited {
        /// Number of seconds the poster should wait before trying again.
        retry_after_secs: u32,
    },

    /// The post was rejected by spam heuristics.
    #[error("post rejected as spam (score: {score:.2})")]
    SpamDetected {
        /// The computed spam score (0.0–1.0). Exceeded `BoardConfig::spam_score_threshold`.
        score: f32,
    },

    /// A duplicate post was detected.
    #[error("duplicate post detected")]
    DuplicatePost,

    /// Post body or attachment failed validation (too long, disallowed MIME, etc.).
    #[error("validation failed: {reason}")]
    Validation {
        /// Human-readable description of which validation rule was violated.
        reason: String,
    },

    /// The thread does not exist.
    #[error("thread not found: {id}")]
    ThreadNotFound {
        /// The thread UUID that was not found.
        id: String,
    },

    /// The thread is closed and does not accept new posts.
    #[error("thread is closed")]
    ThreadClosed,

    /// Media processing failed.
    #[error("media processing failed: {reason}")]
    MediaError {
        /// Human-readable description of what went wrong during media processing.
        reason: String,
    },

    /// An unexpected internal error occurred.
    #[error("internal error: {0}")]
    Internal(#[from] DomainError),
}
