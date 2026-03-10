//! PostgreSQL implementation of `AuditRepository`.
//! Uses runtime sqlx queries — no query! macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{AuditAction, AuditEntry, IpHash, Page, Paginated, UserId};
use domains::ports::AuditRepository;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// PostgreSQL-backed `AuditRepository`.
#[derive(Clone)]
pub struct PgAuditRepository {
    pool: PgPool,
}

impl PgAuditRepository {
    /// Construct a `PgAuditRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id:             Uuid,
    actor_id:       Option<Uuid>,
    actor_ip_hash:  Option<String>,
    action:         String,
    target_id:      Option<Uuid>,
    target_type:    Option<String>,
    details:        Option<serde_json::Value>,
    created_at:     DateTime<Utc>,
}

fn audit_from_row(r: AuditRow) -> Result<AuditEntry, DomainError> {
    Ok(AuditEntry {
        id:             r.id,
        actor_id:       r.actor_id.map(UserId),
        actor_ip_hash:  r.actor_ip_hash.map(IpHash::new),
        action:         AuditAction::from_str(&r.action)
                            .map_err(|e| DomainError::internal(e.to_string()))?,
        target_id:      r.target_id,
        target_type:    r.target_type,
        details:        r.details,
        created_at:     r.created_at,
    })
}

#[async_trait]
impl AuditRepository for PgAuditRepository {
    async fn record(&self, entry: &AuditEntry) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO audit_logs
             (id, actor_id, actor_ip_hash, action, target_id, target_type, details, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(entry.id)
        .bind(entry.actor_id.map(|u| u.0))
        .bind(entry.actor_ip_hash.as_ref().map(|h| h.0.as_str()))
        .bind(entry.action.to_string())
        .bind(entry.target_id)
        .bind(&entry.target_type)
        .bind(&entry.details)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn find_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, DomainError> {
        let rows = sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor_id, actor_ip_hash, action, target_id, target_type, details, created_at \
             FROM audit_logs ORDER BY created_at DESC LIMIT $1"
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        rows.into_iter().map(audit_from_row).collect()
    }

    async fn find_by_actor(&self, actor_id: UserId, page: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor_id, actor_ip_hash, action, target_id, target_type, details, created_at \
             FROM audit_logs WHERE actor_id = $1 \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(actor_id.0)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = $1"
        )
        .bind(actor_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter()
            .map(audit_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn find_by_target(&self, target_id: uuid::Uuid, page: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor_id, actor_ip_hash, action, target_id, target_type, details, created_at \
             FROM audit_logs WHERE target_id = $1 \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(target_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE target_id = $1"
        )
        .bind(target_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter()
            .map(audit_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn find_all(&self, page: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset    = page.offset(page_size) as i64;
        let limit     = page_size as i64;

        let rows = sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor_id, actor_ip_hash, action, target_id, target_type, details, created_at              FROM audit_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(audit_from_row).collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn find_by_board(
        &self,
        board_id: domains::models::BoardId,
        page: Page,
    ) -> Result<Paginated<AuditEntry>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset    = page.offset(page_size) as i64;
        let limit     = page_size as i64;
        let board_str = board_id.0.to_string();

        // Scoped by board_id stored in the JSON details column.
        // Covers all moderation actions that include board context.
        let rows = sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor_id, actor_ip_hash, action, target_id, target_type, details, created_at              FROM audit_logs              WHERE details->>'board_id' = $1              ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(&board_str)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE details->>'board_id' = $1"
        )
        .bind(&board_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(audit_from_row).collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }
}
