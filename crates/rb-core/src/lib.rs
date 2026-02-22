//! rusty-board/crates/rb-core/src/lib.rs
//!
//! The central domain logic and interface definitions for Rusty-Board.

pub mod models;
pub mod traits;
pub mod error;

// Re-exporting for easier access in other crates
pub use models::*;
pub use traits::*;
pub use error::*;


#[cfg(test)]
mod tests {
    use super::models::*;
    use uuid::Uuid;

    #[test]
    fn test_post_creation_v7() {
        let id = Uuid::now_v7();
        let post = Post {
            id,
            thread_id: Uuid::now_v7(),
            user_id_in_thread: "abc12345".to_string(),
            content: "Hello Rust!".to_string(),
            media_id: None,
            is_op: true,
            created_at: chrono::Utc::now(),
            metadata: serde_json::json!({ "version": 1 }),
        };
        assert_eq!(post.id, id);
        assert!(post.is_op);
    }
}