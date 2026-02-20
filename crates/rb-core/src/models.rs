//! # Domain Models
//! 
//! These structs represent the core entities of Rusty-Board.
//! We use UUID v7 for time-ordered, globally unique identification.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Represents a single Imageboard (e.g., /b/, /v/)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: Uuid,
    /// The URL slug (e.g., "b" for /b/)
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    /// JSON bucket for board-specific rules (e.g., max_file_size, allowed_mimes)
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A Thread contains a collection of Posts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: Uuid,
    pub board_id: Uuid,
    /// The timestamp used for sorting threads by activity
    pub last_bump: DateTime<Utc>,
    pub is_sticky: bool,
    pub is_locked: bool,
    pub metadata: serde_json::Value,
}

/// The fundamental unit of conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: Uuid,
    pub thread_id: Uuid,
    /// Ephemeral ID unique to a user within a specific thread
    pub user_id_in_thread: String,
    pub content: String,
    /// Path or ID of the media handled by MediaStore
    pub media_id: Option<String>,
    pub is_op: bool,
    pub created_at: DateTime<Utc>,
    /// Metadata bucket for plugins (e.g., AI labels, tripcode hashes)
    pub metadata: serde_json::Value,
}

/// Represents a moderation action against an IP address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ban {
    pub id: Uuid,
    pub ip_address: String, // Stored as string to support IPv4/v6/CIDR
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}