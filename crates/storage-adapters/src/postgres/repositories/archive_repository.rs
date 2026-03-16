//! PostgreSQL implementation of `ArchiveRepository`.
//!
//! Archived threads live in the `archived_threads` table (migration 015).
//! Posts remain in the `posts` table — only the thread row is moved.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::{
    errors::DomainError,
    models::{BoardId, Page, Paginated, PostId, Thread, ThreadId},
    ports::ArchiveRepository,
};
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL-backed archive store.
#[derive(Clone)]
pub struct PgArchiveRepository {
    pool: PgPool,
}

impl PgArchiveRepository {
    /// Construct a new repository wrapping an existing connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct ArchivedThreadRow {
    id:          Uuid,
    board_id:    Uuid,
    op_post_id:  Option<Uuid>,
    reply_count: i32,
    bumped_at:   DateTime<Utc>,
    sticky:      bool,
    closed:      bool,
    cycle:       bool,
    created_at:  DateTime<Utc>,
}

fn thread_from_archived(r: ArchivedThreadRow) -> Thread {
    Thread {
        id:          ThreadId(r.id),
        board_id:    BoardId(r.board_id),
        op_post_id:  r.op_post_id.map(PostId),
        reply_count: r.reply_count as u32,
        bumped_at:   r.bumped_at,
        sticky:      r.sticky,
        closed:      r.closed,
        cycle:       r.cycle,
        created_at:  r.created_at,
    }
}

#[async_trait]
impl ArchiveRepository for PgArchiveRepository {
    async fn archive_thread(&self, thread: &Thread) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO archived_threads
             (id, board_id, op_post_id, reply_count, bumped_at, sticky, closed, cycle, created_at, archived_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(thread.id.0)
        .bind(thread.board_id.0)
        .bind(thread.op_post_id.map(|p| p.0))
        .bind(thread.reply_count as i32)
        .bind(thread.bumped_at)
        .bind(thread.sticky)
        .bind(thread.closed)
        .bind(thread.cycle)
        .bind(thread.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn find_archived(
        &self,
        board_id: BoardId,
        page: Page,
    ) -> Result<Paginated<Thread>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset    = page.offset(page_size) as i64;
        let limit     = page_size as i64;

        let rows = sqlx::query_as::<_, ArchivedThreadRow>(
            "SELECT id, board_id, op_post_id, reply_count, bumped_at, sticky, closed, cycle, created_at
             FROM   archived_threads
             WHERE  board_id = $1
             ORDER  BY archived_at DESC
             LIMIT  $2 OFFSET $3",
        )
        .bind(board_id.0)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM archived_threads WHERE board_id = $1",
        )
        .bind(board_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(thread_from_archived).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }
}

/// No-op archive repository — used when archiving is disabled.
///
/// `archive_thread` silently succeeds (caller still deletes the thread),
/// `find_archived` returns an empty page.
#[derive(Clone, Default)]
pub struct NoopArchiveRepository;

#[async_trait]
impl ArchiveRepository for NoopArchiveRepository {
    async fn archive_thread(&self, _thread: &Thread) -> Result<(), DomainError> {
        Ok(())
    }
    async fn find_archived(
        &self,
        _board_id: BoardId,
        page: Page,
    ) -> Result<Paginated<Thread>, DomainError> {
        Ok(Paginated::new(vec![], 0, page, Page::DEFAULT_PAGE_SIZE))
    }
}
