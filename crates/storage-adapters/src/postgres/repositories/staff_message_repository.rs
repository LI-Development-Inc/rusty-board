//! PostgreSQL implementation of `StaffMessageRepository`.
//!
//! Messages are stored in the `staff_messages` table (migration 015).
//! Expiry is handled on demand via `delete_expired` — no database-level TTL.
//! Uses runtime sqlx queries — no `query!` macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{Page, Paginated, StaffMessage, StaffMessageId, UserId};
use domains::ports::StaffMessageRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL-backed `StaffMessageRepository`.
#[derive(Clone)]
pub struct PgStaffMessageRepository {
    pool: PgPool,
}

impl PgStaffMessageRepository {
    /// Construct a `PgStaffMessageRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id:           Uuid,
    from_user_id: Uuid,
    to_user_id:   Uuid,
    body:         String,
    read_at:      Option<DateTime<Utc>>,
    created_at:   DateTime<Utc>,
}

fn message_from_row(r: MessageRow) -> StaffMessage {
    StaffMessage {
        id:           StaffMessageId(r.id),
        from_user_id: UserId(r.from_user_id),
        to_user_id:   UserId(r.to_user_id),
        body:         r.body,
        read_at:      r.read_at,
        created_at:   r.created_at,
    }
}

#[async_trait]
impl StaffMessageRepository for PgStaffMessageRepository {
    async fn find_for_user(
        &self,
        user_id: UserId,
        page: Page,
    ) -> Result<Paginated<StaffMessage>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset    = page.offset(page_size) as i64;
        let limit     = page_size as i64;

        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, from_user_id, to_user_id, body, read_at, created_at
             FROM staff_messages
             WHERE to_user_id = $1
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(user_id.0)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM staff_messages WHERE to_user_id = $1",
        )
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(message_from_row).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn count_unread(&self, user_id: UserId) -> Result<u32, DomainError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM staff_messages WHERE to_user_id = $1 AND read_at IS NULL",
        )
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(count as u32)
    }

    async fn save(&self, message: &StaffMessage) -> Result<StaffMessageId, DomainError> {
        sqlx::query(
            "INSERT INTO staff_messages
             (id, from_user_id, to_user_id, body, read_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(message.id.0)
        .bind(message.from_user_id.0)
        .bind(message.to_user_id.0)
        .bind(&message.body)
        .bind(message.read_at)
        .bind(message.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(message.id)
    }

    async fn mark_read(&self, id: StaffMessageId) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE staff_messages SET read_at = NOW() WHERE id = $1 AND read_at IS NULL",
        )
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn delete_expired(&self, older_than_days: u32) -> Result<u32, DomainError> {
        let result = sqlx::query(
            "DELETE FROM staff_messages
             WHERE created_at < NOW() - ($1 || ' days')::INTERVAL",
        )
        .bind(older_than_days as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(result.rows_affected() as u32)
    }
}
