use async_trait::async_trait;
use rb_core::models::{Board, Post, Thread};
use rb_core::traits::BoardRepo;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use crate::SqliteBoardRepo as SqliteDatabase;
use sqlx::Pool;
use sqlx::Sqlite;
use uuid::Uuid;
use chrono::Utc;

pub struct SqliteBoardRepo {
    pool: Pool<Sqlite>,
}

impl SqliteBoardRepo {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl BoardRepo for SqliteBoardRepo {
    async fn get_board(&self, slug: &str) -> anyhow::Result<Option<Board>> {
        let row = sqlx::query!(
            r#"SELECT id, slug, title, description, created_at, metadata FROM boards WHERE slug = ?"#,
            slug
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Board {
            id: Uuid::from_slice(r.id.as_deref().unwrap_or(&[])).unwrap_or_default(),
            slug: r.slug,
            title: r.title,
            description: r.description,
            created_at: r.created_at.map(|dt| dt.and_utc()).unwrap_or_else(Utc::now),
            settings: serde_json::from_str(&r.metadata.unwrap_or_default()).unwrap_or_default(),
        }))
    }

    async fn list_boards(&self) -> anyhow::Result<Vec<Board>> {
        let rows = sqlx::query!(r#"SELECT id, slug, title, description, created_at, metadata FROM boards"#)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|r| Board {
            id: Uuid::from_slice(r.id.as_deref().unwrap_or(&[])).unwrap_or_default(),
            slug: r.slug,
            title: r.title,
            description: r.description,
            created_at: r.created_at.map(|dt| dt.and_utc()).unwrap_or_else(Utc::now),
            settings: serde_json::from_str(&r.metadata.unwrap_or_default()).unwrap_or_default(),
        }).collect())
    }

    async fn create_thread(&self, thread: Thread, post: Post) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            "INSERT INTO threads (id, board_id, last_bump, is_sticky, is_locked, metadata) VALUES (?, ?, ?, ?, ?, ?)",
            thread.id, thread.board_id, thread.last_bump, thread.is_sticky, thread.is_locked, thread.metadata
        ).execute(&mut *tx).await?;

        sqlx::query!(
            "INSERT INTO posts (id, thread_id, user_id_in_thread, content, media_id, is_op, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            post.id, post.thread_id, post.user_id_in_thread, post.content, post.media_id, post.is_op, post.created_at, post.metadata
        ).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn create_post(&self, post: Post) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO posts (id, thread_id, user_id_in_thread, content, media_id, is_op, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            post.id, post.thread_id, post.user_id_in_thread, post.content, post.media_id, post.is_op, post.created_at, post.metadata
        ).execute(&self.pool).await?;
        Ok(())
    }

async fn get_thread(&self, thread_id: Uuid) -> anyhow::Result<Option<(Thread, Vec<Post>)>> {
        let t_row = sqlx::query!(
            r#"SELECT id, board_id, last_bump, is_sticky, is_locked, metadata FROM threads WHERE id = ?"#,
            thread_id
        ).fetch_optional(&self.pool).await?;

        if let Some(r) = t_row {
            let thread = Thread {
                id: Uuid::from_slice(r.id.as_deref().unwrap_or(&[])).unwrap_or_default(),
                board_id: Uuid::from_slice(&r.board_id).unwrap_or_default(),
                last_bump: r.last_bump.and_utc(),
                // SQLx already converted these to bool!
                is_sticky: r.is_sticky.unwrap_or(false),
                is_locked: r.is_locked.unwrap_or(false),
                metadata: serde_json::from_str(&r.metadata.unwrap_or_default()).unwrap_or_default(),
            };

            let p_rows = sqlx::query!(
                r#"SELECT id, thread_id, user_id_in_thread, content, media_id, is_op, created_at, metadata FROM posts WHERE thread_id = ?"#,
                thread_id
            ).fetch_all(&self.pool).await?;

            let posts = p_rows.into_iter().map(|pr| Post {
                id: Uuid::from_slice(pr.id.as_deref().unwrap_or(&[])).unwrap_or_default(),
                thread_id: Uuid::from_slice(&pr.thread_id).unwrap_or_default(),
                user_id_in_thread: pr.user_id_in_thread.unwrap_or_else(|| "Anonymous".to_string()),
                content: pr.content,
                media_id: pr.media_id.and_then(|m| {
                    let s = String::from_utf8_lossy(&m).to_string();
                    if s.is_empty() { None } else { Some(s) }
                }),                
                is_op: pr.is_op.unwrap_or(false), // Simplified
                created_at: pr.created_at.map(|dt| dt.and_utc()).unwrap_or_else(Utc::now),
                metadata: serde_json::from_str(&pr.metadata.unwrap_or_default()).unwrap_or_default(),
            }).collect();

            Ok(Some((thread, posts)))
        } else {
            Ok(None)
        }
    }

    async fn get_threads_by_board(&self, board_id: Uuid) -> anyhow::Result<Vec<(Thread, Post)>> {
        let rows = sqlx::query!(
            r#"SELECT 
                t.id as t_id, t.board_id as t_board_id, t.last_bump as t_last_bump, t.is_sticky as t_is_sticky, t.is_locked as t_is_locked, t.metadata as t_meta,
                p.id as p_id, p.thread_id as p_thread_id, p.user_id_in_thread as p_user_id, p.content as p_content, p.media_id as p_media, p.is_op as p_is_op, p.created_at as p_created, p.metadata as p_meta
               FROM threads t 
               JOIN posts p ON p.thread_id = t.id 
               WHERE t.board_id = ? AND p.is_op = 1"#,
            board_id
        )
        .fetch_all(&self.pool)
        .await?;

        let results = rows.into_iter().map(|row| {
            let thread = Thread {
                id: Uuid::from_slice(row.t_id.as_deref().unwrap_or(&[])).unwrap_or_default(),
                board_id: Uuid::from_slice(&row.t_board_id).unwrap_or_default(),
                last_bump: row.t_last_bump.and_utc(),
                is_sticky: row.t_is_sticky.unwrap_or(false), // Simplified
                is_locked: row.t_is_locked.unwrap_or(false), // Simplified
                metadata: serde_json::from_str(&row.t_meta.unwrap_or_default()).unwrap_or_default(),
            };

            let post = Post {
                id: Uuid::from_slice(row.p_id.as_deref().unwrap_or(&[])).unwrap_or_default(),
                thread_id: Uuid::from_slice(&row.p_thread_id).unwrap_or_default(),
                user_id_in_thread: row.p_user_id.unwrap_or_else(|| "Anonymous".to_string()),
                content: row.p_content,
                media_id: row.p_media.and_then(|m| {
                    let s = String::from_utf8_lossy(&m).to_string();
                    if s.is_empty() { None } else { Some(s) }
                    }),
                is_op: row.p_is_op.unwrap_or(false), // Simplified
                created_at: row.p_created.map(|dt| dt.and_utc()).unwrap_or_else(Utc::now),
                metadata: serde_json::from_str(&row.p_meta.unwrap_or_default()).unwrap_or_default(),
            };
            (thread, post)
        }).collect();

        Ok(results)
    }

    async fn list_threads_paginated(&self, board_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<Thread>> {
        let rows = sqlx::query!(
            r#"SELECT id, board_id, last_bump, is_sticky, is_locked, metadata 
               FROM threads WHERE board_id = ? 
               ORDER BY is_sticky DESC, last_bump DESC 
               LIMIT ? OFFSET ?"#,
            board_id, limit, offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Thread {
            id: Uuid::from_slice(r.id.as_deref().unwrap_or(&[])).unwrap_or_default(),
            board_id: Uuid::from_slice(&r.board_id).unwrap_or_default(),
            last_bump: r.last_bump.and_utc(),
            is_sticky: r.is_sticky.unwrap_or(false), // Simplified
            is_locked: r.is_locked.unwrap_or(false), // Simplified
            metadata: serde_json::from_str(&r.metadata.unwrap_or_default()).unwrap_or_default(),
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_thread_creation() {
        // Use the actual constructor which handles the pool internally
        let db = SqliteBoardRepo::new("sqlite::memory:").await.unwrap();
        
        // Use v7 to match your handlers
        let board_id = Uuid::now_v7();
        
        // Note: Ensure create_thread signature matches your Trait (Thread, Post)
        // If you are using the simplified test helper, ensure types align
    }
}