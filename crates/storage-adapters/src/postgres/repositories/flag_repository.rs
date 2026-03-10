//! PostgreSQL implementation of `FlagRepository`.
//! Uses runtime sqlx queries — no query! macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{Flag, FlagId, FlagResolution, FlagStatus, IpHash, Page, Paginated, PostId, UserId};
use domains::ports::FlagRepository;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// PostgreSQL-backed `FlagRepository`.
#[derive(Clone)]
pub struct PgFlagRepository {
    pool: PgPool,
}

impl PgFlagRepository {
    /// Construct a `PgFlagRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct FlagRow {
    id:               Uuid,
    post_id:          Uuid,
    reason:           String,
    reporter_ip_hash: String,
    status:           String,
    resolved_by:      Option<Uuid>,
    created_at:       DateTime<Utc>,
}

fn flag_from_row(r: FlagRow) -> Result<Flag, DomainError> {
    Ok(Flag {
        id:               FlagId(r.id),
        post_id:          PostId(r.post_id),
        reason:           r.reason,
        reporter_ip_hash: IpHash::new(r.reporter_ip_hash),
        status:           FlagStatus::from_str(&r.status).map_err(DomainError::internal)?,
        resolved_by:      r.resolved_by.map(UserId),
        created_at:       r.created_at,
    })
}

#[async_trait]
impl FlagRepository for PgFlagRepository {
    async fn find_by_id(&self, id: FlagId) -> Result<Flag, DomainError> {
        let row = sqlx::query_as::<_, FlagRow>(
            "SELECT id, post_id, reason, reporter_ip_hash, status, resolved_by, created_at \
             FROM flags WHERE id = $1"
        )
        .bind(id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::not_found(id.to_string()),
            other => DomainError::internal(other.to_string()),
        })?;
        flag_from_row(row)
    }

    async fn find_pending(&self, page: Page) -> Result<Paginated<Flag>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, FlagRow>(
            "SELECT id, post_id, reason, reporter_ip_hash, status, resolved_by, created_at \
             FROM flags WHERE status = 'pending' \
             ORDER BY created_at ASC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM flags WHERE status = 'pending'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter()
            .map(flag_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn save(&self, flag: &Flag) -> Result<FlagId, DomainError> {
        sqlx::query(
            "INSERT INTO flags (id, post_id, reason, reporter_ip_hash, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(flag.id.0)
        .bind(flag.post_id.0)
        .bind(&flag.reason)
        .bind(&flag.reporter_ip_hash.0)
        .bind(flag.status.to_string())
        .bind(flag.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(flag.id)
    }

    async fn resolve(
        &self,
        id: FlagId,
        resolution: FlagResolution,
        resolved_by: UserId,
    ) -> Result<(), DomainError> {
        let status = match resolution {
            FlagResolution::Approved => "approved",
            FlagResolution::Rejected => "rejected",
        };
        let result = sqlx::query(
            "UPDATE flags SET status = $2, resolved_by = $3 WHERE id = $1 AND status = 'pending'"
        )
        .bind(id.0)
        .bind(status)
        .bind(resolved_by.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::not_found(id.to_string()));
        }
        Ok(())
    }
}
