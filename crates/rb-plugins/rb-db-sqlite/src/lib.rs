//! # rb-db-sqlite Implementation
//! 
//! This module implements the data mapping between the SQLite relational model
//! and the `rb-core` domain models.

use async_trait::async_trait;
use rb_core::models::{Board, Thread, Post};
use rb_core::traits::BoardRepo;
use rb_core::error::{AppError, Result};
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

pub struct SqliteBoardRepo {
    pool: SqlitePool,
}

// Helper for UUID conversion
fn uuid_to_blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

fn blob_to_uuid(blob: &[u8]) -> Uuid {
    Uuid::from_slice(blob).unwrap_or_default()
}

#[async_trait]
impl BoardRepo for SqliteBoardRepo {
    /// Retrieves a board by its slug.
    /// Maps SQL TEXT and BLOB fields back to Domain Models.
    async fn get_board(&self, slug: &str) -> anyhow::Result<Option<Board>> {
        let row = sqlx::query(
            "SELECT id, slug, title, description, settings, created_at FROM boards WHERE slug = ?"
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(Board {
                id: blob_to_uuid(row.get::<Vec<u8>, _>("id").as_slice()),
                slug: row.get("slug"),
                title: row.get("title"),
                description: row.get("description"),
                settings: serde_json::from_str(&row.get::<String, _>("settings")).unwrap_or_default(),
                created_at: row.get("created_at"),
            }))
        } else {
            Ok(None)
        }
    }

    /// Atomic operation to create a thread and its first post.
    /// 
    /// # Developer Note
    /// Using a Transaction (tx) ensures we don't end up with "ghost threads" 
    /// that have no initial post if the second insert fails.
    async fn create_thread(&self, thread: Thread, initial_post: Post) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        // 1. Insert Thread
        sqlx::query("INSERT INTO threads (id, board_id, last_bump, is_sticky, is_locked, metadata) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(uuid_to_blob(thread.id))
            .bind(uuid_to_blob(thread.board_id))
            .bind(thread.last_bump)
            .bind(thread.is_sticky)
            .bind(thread.is_locked)
            .bind(serde_json::to_string(&thread.metadata)?)
            .execute(&mut *tx)
            .await?;

        // 2. Insert OP Post
        sqlx::query("INSERT INTO posts (id, thread_id, user_id_in_thread, content, media_id, is_op, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(uuid_to_blob(initial_post.id))
            .bind(uuid_to_blob(initial_post.thread_id))
            .bind(initial_post.user_id_in_thread)
            .bind(initial_post.content)
            .bind(initial_post.media_id)
            .bind(initial_post.is_op)
            .bind(initial_post.created_at)
            .bind(serde_json::to_string(&initial_post.metadata)?)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Retrieves a thread and all its posts in a single logical operation.
    async fn get_thread(&self, id: Uuid) -> anyhow::Result<Option<(Thread, Vec<Post>)>> {
        let thread_row = sqlx::query("SELECT * FROM threads WHERE id = ?")
            .bind(uuid_to_blob(id))
            .fetch_optional(&self.pool)
            .await?;

        let thread = match thread_row {
            Some(row) => Thread {
                id: blob_to_uuid(row.get::<Vec<u8>, _>("id").as_slice()),
                board_id: blob_to_uuid(row.get::<Vec<u8>, _>("board_id").as_slice()),
                last_bump: row.get("last_bump"),
                is_sticky: row.get("is_sticky"),
                is_locked: row.get("is_locked"),
                metadata: serde_json::from_str(&row.get::<String, _>("metadata")).unwrap_or_default(),
            },
            None => return Ok(None),
        };

        let posts = sqlx::query("SELECT * FROM posts WHERE thread_id = ? ORDER BY created_at ASC")
            .bind(uuid_to_blob(id))
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| Post {
                id: blob_to_uuid(row.get::<Vec<u8>, _>("id").as_slice()),
                thread_id: blob_to_uuid(row.get::<Vec<u8>, _>("thread_id").as_slice()),
                user_id_in_thread: row.get("user_id_in_thread"),
                content: row.get("content"),
                media_id: row.get("media_id"),
                is_op: row.get("is_op"),
                created_at: row.get("created_at"),
                metadata: serde_json::from_str(&row.get::<String, _>("metadata")).unwrap_or_default(),
            })
            .collect();

        Ok(Some((thread, posts)))
    }

    async fn create_post(&self, post: Post) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO posts (id, thread_id, user_id_in_thread, content, media_id, is_op, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(uuid_to_blob(post.id))
            .bind(uuid_to_blob(post.thread_id))
            .bind(post.user_id_in_thread)
            .bind(post.content)
            .bind(post.media_id)
            .bind(post.is_op)
            .bind(post.created_at)
            .bind(serde_json::to_string(&post.metadata)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_boards(&self) -> anyhow::Result<Vec<Board>> {
        let rows = sqlx::query("SELECT * FROM boards")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| Board {
            id: blob_to_uuid(row.get::<Vec<u8>, _>("id").as_slice()),
            slug: row.get("slug"),
            title: row.get("title"),
            description: row.get("description"),
            settings: serde_json::from_str(&row.get::<String, _>("settings")).unwrap_or_default(),
            created_at: row.get("created_at"),
        }).collect())
    }

    async fn list_threads_paginated(&self, board_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<Thread>> {
        let rows = sqlx::query("SELECT * FROM threads WHERE board_id = ? ORDER BY last_bump DESC LIMIT ? OFFSET ?")
            .bind(uuid_to_blob(board_id))
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| Thread {
            id: blob_to_uuid(row.get::<Vec<u8>, _>("id").as_slice()),
            board_id: blob_to_uuid(row.get::<Vec<u8>, _>("board_id").as_slice()),
            last_bump: row.get("last_bump"),
            is_sticky: row.get("is_sticky"),
            is_locked: row.get("is_locked"),
            metadata: serde_json::from_str(&row.get::<String, _>("settings")).unwrap_or_default(), // SQLite uses settings for metadata sometimes
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::models::{Thread, Post};
    
    #[tokio::test]
    async fn test_create_and_get_thread() {
        let repo = SqliteBoardRepo::new("sqlite::memory:").await.unwrap();
        
        let board_id = Uuid::now_v7();
        let thread_id = Uuid::now_v7();
        
        // Setup dummy board first (due to foreign key)
        sqlx::query("INSERT INTO boards (id, slug, title) VALUES (?, ?, ?)")
            .bind(uuid_to_blob(board_id)).bind("test").bind("Test Board")
            .execute(&repo.pool).await.unwrap();

        let thread = Thread { 
            id: thread_id, board_id, last_bump: chrono::Utc::now(), 
            is_sticky: false, is_locked: false, metadata: serde_json::json!({}) 
        };
        
        let post = Post {
            id: Uuid::now_v7(), thread_id, user_id_in_thread: "123".into(),
            content: "OP".into(), media_id: None, is_op: true,
            created_at: chrono::Utc::now(), metadata: serde_json::json!({})
        };

        repo.create_thread(thread, post).await.expect("Failed to create thread");
        
        let result = repo.get_thread(thread_id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().1.len(), 1);
    }
}