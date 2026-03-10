//! PostgreSQL implementation of `ThreadRepository`.
//! Uses runtime sqlx queries — no query! macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{
    BoardId, MediaKey, Page, Paginated, PostId, Thread, ThreadId, ThreadSummary,
};
use domains::ports::ThreadRepository;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

/// PostgreSQL-backed `ThreadRepository`.
#[derive(Clone)]
pub struct PgThreadRepository {
    pool: PgPool,
}

impl PgThreadRepository {
    /// Construct a `PgThreadRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_sqlx_err(e: sqlx::Error, resource: impl Into<String>) -> DomainError {
    match e {
        sqlx::Error::RowNotFound => DomainError::not_found(resource),
        other => DomainError::internal(other.to_string()),
    }
}

#[derive(sqlx::FromRow)]
struct ThreadRow {
    id:           Uuid,
    board_id:     Uuid,
    op_post_id:   Option<Uuid>,
    reply_count:  i32,
    bumped_at:    DateTime<Utc>,
    sticky:       bool,
    closed:       bool,
    created_at:   DateTime<Utc>,
}

fn thread_from_row(r: ThreadRow) -> Thread {
    Thread {
        id:          ThreadId(r.id),
        board_id:    BoardId(r.board_id),
        op_post_id:  r.op_post_id.map(PostId),
        reply_count: r.reply_count as u32,
        bumped_at:   r.bumped_at,
        sticky:      r.sticky,
        closed:      r.closed,
        created_at:  r.created_at,
    }
}

#[derive(sqlx::FromRow)]
struct ThreadSummaryRow {
    thread_id:     Uuid,
    board_id:      Uuid,
    reply_count:   i32,
    sticky:        bool,
    closed:        bool,
    bumped_at:     DateTime<Utc>,
    op_body:       Option<String>,
    thumbnail_key: Option<String>,
}

#[async_trait]
impl ThreadRepository for PgThreadRepository {
    #[instrument(skip(self), fields(thread_id = %id))]
    async fn find_by_id(&self, id: ThreadId) -> Result<Thread, DomainError> {
        let row = sqlx::query_as::<_, ThreadRow>(
            "SELECT id, board_id, op_post_id, reply_count, bumped_at, sticky, closed, created_at \
             FROM threads WHERE id = $1"
        )
        .bind(id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_sqlx_err(e, id.to_string()))?;
        Ok(thread_from_row(row))
    }

    #[instrument(skip(self), fields(board_id = %board_id, page = page.0))]
    async fn find_by_board(&self, board_id: BoardId, page: Page) -> Result<Paginated<Thread>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, ThreadRow>(
            "SELECT id, board_id, op_post_id, reply_count, bumped_at, sticky, closed, created_at \
             FROM threads WHERE board_id = $1 \
             ORDER BY sticky DESC, bumped_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(board_id.0)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM threads WHERE board_id = $1"
        )
        .bind(board_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(thread_from_row).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    #[instrument(skip(self), fields(board_id = %board_id))]
    async fn find_catalog(&self, board_id: BoardId) -> Result<Vec<ThreadSummary>, DomainError> {
        // Left join with op post and first attachment thumbnail
        let rows = sqlx::query_as::<_, ThreadSummaryRow>(
            "SELECT t.id AS thread_id, t.board_id, t.reply_count, t.sticky, t.closed, t.bumped_at,
                    p.body AS op_body, a.thumbnail_key
             FROM threads t
             LEFT JOIN posts p ON p.id = t.op_post_id
             LEFT JOIN LATERAL (
               SELECT thumbnail_key FROM attachments
               WHERE post_id = t.op_post_id
               ORDER BY id ASC LIMIT 1
             ) a ON true
             WHERE t.board_id = $1
             ORDER BY t.sticky DESC, t.bumped_at DESC"
        )
        .bind(board_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        Ok(rows.into_iter().map(|r| ThreadSummary {
            thread_id:     ThreadId(r.thread_id),
            board_id:      BoardId(r.board_id),
            op_body:       r.op_body.unwrap_or_default(),
            thumbnail_key: r.thumbnail_key.map(MediaKey::new),
            reply_count:   r.reply_count as u32,
            sticky:        r.sticky,
            closed:        r.closed,
            bumped_at:     r.bumped_at,
        }).collect())
    }

    #[instrument(skip(self, thread), fields(board_id = %thread.board_id))]
    async fn save(&self, thread: &Thread) -> Result<ThreadId, DomainError> {
        sqlx::query(
            "INSERT INTO threads (id, board_id, op_post_id, reply_count, bumped_at, sticky, closed, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
               op_post_id = EXCLUDED.op_post_id,
               reply_count = EXCLUDED.reply_count,
               bumped_at = EXCLUDED.bumped_at,
               sticky = EXCLUDED.sticky,
               closed = EXCLUDED.closed"
        )
        .bind(thread.id.0)
        .bind(thread.board_id.0)
        .bind(thread.op_post_id.map(|p| p.0))
        .bind(thread.reply_count as i32)
        .bind(thread.bumped_at)
        .bind(thread.sticky)
        .bind(thread.closed)
        .bind(thread.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(thread.id)
    }

    #[instrument(skip(self), fields(thread_id = %id))]
    async fn bump(&self, id: ThreadId, bumped_at: DateTime<Utc>) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE threads SET bumped_at = $2, reply_count = reply_count + 1 WHERE id = $1"
        )
        .bind(id.0)
        .bind(bumped_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self), fields(thread_id = %id))]
    async fn set_op_post(&self, id: ThreadId, op_post_id: PostId) -> Result<(), DomainError> {
        sqlx::query("UPDATE threads SET op_post_id = $2 WHERE id = $1")
            .bind(id.0)
            .bind(op_post_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn set_sticky(&self, id: ThreadId, sticky: bool) -> Result<(), DomainError> {
        sqlx::query("UPDATE threads SET sticky = $2 WHERE id = $1")
            .bind(id.0)
            .bind(sticky)
            .execute(&self.pool)
            .await
            .map_err(|e| map_sqlx_err(e, id.to_string()))?;
        Ok(())
    }

    async fn set_closed(&self, id: ThreadId, closed: bool) -> Result<(), DomainError> {
        sqlx::query("UPDATE threads SET closed = $2 WHERE id = $1")
            .bind(id.0)
            .bind(closed)
            .execute(&self.pool)
            .await
            .map_err(|e| map_sqlx_err(e, id.to_string()))?;
        Ok(())
    }

    async fn count_by_board(&self, board_id: BoardId) -> Result<u32, DomainError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM threads WHERE board_id = $1"
        )
        .bind(board_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(count as u32)
    }

    #[instrument(skip(self), fields(board_id = %board_id, keep = keep))]
    async fn prune_oldest(&self, board_id: BoardId, keep: u32) -> Result<u32, DomainError> {
        let result = sqlx::query(
            "DELETE FROM threads WHERE id IN (
               SELECT id FROM threads
               WHERE board_id = $1 AND sticky = false
               ORDER BY bumped_at ASC
               LIMIT GREATEST((SELECT COUNT(*) FROM threads WHERE board_id = $1 AND sticky = false) - $2, 0)
             )"
        )
        .bind(board_id.0)
        .bind(keep as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(result.rows_affected() as u32)
    }

    async fn delete(&self, id: ThreadId) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM threads WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::not_found(id.to_string()));
        }
        Ok(())
    }
}
