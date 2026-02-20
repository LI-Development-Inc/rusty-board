//! # Core Traits (Ports)
//! 
//! Any plugin must implement these traits to be used by the binary.

use async_trait::async_trait;
use crate::models::{Board, Thread, Post};
use uuid::Uuid;

/// Data persistence contract for boards, threads, and posts.
#[async_trait]
pub trait BoardRepo: Send + Sync {
    // Board Operations
    async fn get_board(&self, slug: &str) -> anyhow::Result<Option<Board>>;
    async fn list_boards(&self) -> anyhow::Result<Vec<Board>>;

    // Thread Operations
    async fn create_thread(&self, thread: Thread, initial_post: Post) -> anyhow::Result<()>;
    async fn get_thread(&self, id: Uuid) -> anyhow::Result<Option<(Thread, Vec<Post>)>>;
    async fn list_threads_paginated(&self, board_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<Thread>>;

    // Post Operations
    async fn create_post(&self, post: Post) -> anyhow::Result<()>;
    
    // TODO: Add search_posts method for Phase 2
}

/// Media storage contract for handling uploads and thumbnails.
#[async_trait]
pub trait MediaStore: Send + Sync {
    /// Saves raw bytes and returns a media_id for the Post model.
    async fn save_upload(&self, data: Vec<u8>, content_type: &str) -> anyhow::Result<String>;
    /// Returns the URL or path to the original media.
    async fn get_url(&self, media_id: &str) -> String;
    /// Returns the URL or path to the thumbnail.
    async fn get_thumbnail_url(&self, media_id: &str) -> String;
}

/// Identity and Moderation contract.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Generates a unique ID for a user within a specific thread
    fn generate_thread_id(&self, ip: &str, thread_id: &str) -> String;
    
    /// Generates a tripcode from a password
    fn generate_tripcode(&self, password: &str) -> String;
    
    /// Verifies staff/admin credentials
    async fn verify_admin_password(&self, password: &str, hash: &str) -> bool;
    
    /// Checks if an IP is currently restricted
    async fn check_ban(&self, ip: &str) -> anyhow::Result<bool>;
}