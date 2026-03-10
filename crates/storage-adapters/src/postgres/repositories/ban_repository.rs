//! PostgreSQL implementation of `BanRepository`.
//! Uses runtime sqlx queries — no query! macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{Ban, BanId, IpHash, Page, Paginated, UserId};
use domains::ports::BanRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL-backed `BanRepository`.
#[derive(Clone)]
pub struct PgBanRepository {
    pool: PgPool,
}

impl PgBanRepository {
    /// Construct a `PgBanRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct BanRow {
    id:         Uuid,
    ip_hash:    String,
    banned_by:  Uuid,
    reason:     String,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

fn ban_from_row(r: BanRow) -> Ban {
    Ban {
        id:         BanId(r.id),
        ip_hash:    IpHash::new(r.ip_hash),
        banned_by:  UserId(r.banned_by),
        reason:     r.reason,
        expires_at: r.expires_at,
        created_at: r.created_at,
    }
}

#[async_trait]
impl BanRepository for PgBanRepository {
    async fn find_active_by_ip(&self, ip_hash: &IpHash) -> Result<Option<Ban>, DomainError> {
        let row = sqlx::query_as::<_, BanRow>(
            "SELECT id, ip_hash, banned_by, reason, expires_at, created_at
             FROM bans
             WHERE ip_hash = $1
               AND (expires_at IS NULL OR expires_at > now())
             ORDER BY created_at DESC LIMIT 1"
        )
        .bind(&ip_hash.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(row.map(ban_from_row))
    }

    async fn save(&self, ban: &Ban) -> Result<BanId, DomainError> {
        sqlx::query(
            "INSERT INTO bans (id, ip_hash, banned_by, reason, expires_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(ban.id.0)
        .bind(&ban.ip_hash.0)
        .bind(ban.banned_by.0)
        .bind(&ban.reason)
        .bind(ban.expires_at)
        .bind(ban.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(ban.id)
    }

    async fn expire(&self, id: BanId) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE bans SET expires_at = now() WHERE id = $1"
        )
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::not_found(id.to_string()));
        }
        Ok(())
    }

    async fn find_all(&self, page: Page) -> Result<Paginated<Ban>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, BanRow>(
            "SELECT id, ip_hash, banned_by, reason, expires_at, created_at \
             FROM bans ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bans")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(ban_from_row).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }
}
