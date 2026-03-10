//! PostgreSQL implementation of `StaffRequestRepository`.
//!
//! Persists staff promotion requests in the `staff_requests` table (migration 014).
//! Uses runtime sqlx queries — no `query!` macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{
    Slug, StaffRequest, StaffRequestId, StaffRequestStatus, StaffRequestType, UserId,
};
use domains::ports::StaffRequestRepository;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// PostgreSQL-backed `StaffRequestRepository`.
#[derive(Clone)]
pub struct PgStaffRequestRepository {
    pool: PgPool,
}

impl PgStaffRequestRepository {
    /// Construct a `PgStaffRequestRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct RequestRow {
    id:           Uuid,
    from_user_id: Uuid,
    request_type: String,
    target_slug:  Option<String>,
    payload:      serde_json::Value,
    status:       String,
    reviewed_by:  Option<Uuid>,
    review_note:  Option<String>,
    created_at:   DateTime<Utc>,
    updated_at:   DateTime<Utc>,
}

fn request_from_row(r: RequestRow) -> Result<StaffRequest, DomainError> {
    Ok(StaffRequest {
        id:           StaffRequestId(r.id),
        from_user_id: UserId(r.from_user_id),
        request_type: StaffRequestType::from_str(&r.request_type)
            .map_err(|e| DomainError::internal(e.to_string()))?,
        target_slug:  r.target_slug.map(|s| {
            Slug::new(s).map_err(|e| DomainError::internal(e.to_string()))
        }).transpose()?,
        payload:      r.payload,
        status:       StaffRequestStatus::from_str(&r.status)
            .map_err(|e| DomainError::internal(e.to_string()))?,
        reviewed_by:  r.reviewed_by.map(UserId),
        review_note:  r.review_note,
        created_at:   r.created_at,
        updated_at:   r.updated_at,
    })
}

const SELECT_COLS: &str =
    "id, from_user_id, request_type, target_slug, payload, status, \
     reviewed_by, review_note, created_at, updated_at";

#[async_trait]
impl StaffRequestRepository for PgStaffRequestRepository {
    async fn save(&self, request: &StaffRequest) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO staff_requests
             (id, from_user_id, request_type, target_slug, payload,
              status, reviewed_by, review_note, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE
               SET status      = EXCLUDED.status,
                   reviewed_by = EXCLUDED.reviewed_by,
                   review_note = EXCLUDED.review_note,
                   updated_at  = EXCLUDED.updated_at"
        )
        .bind(request.id.0)
        .bind(request.from_user_id.0)
        .bind(request.request_type.to_string())
        .bind(request.target_slug.as_ref().map(|s| s.as_str()))
        .bind(&request.payload)
        .bind(request.status.to_string())
        .bind(request.reviewed_by.map(|u| u.0))
        .bind(&request.review_note)
        .bind(request.created_at)
        .bind(request.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: StaffRequestId) -> Result<StaffRequest, DomainError> {
        let row = sqlx::query_as::<_, RequestRow>(
            &format!("SELECT {SELECT_COLS} FROM staff_requests WHERE id = $1"),
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?
        .ok_or_else(|| DomainError::not_found(format!("staff_request:{}", id.0)))?;
        request_from_row(row)
    }

    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<StaffRequest>, DomainError> {
        let rows = sqlx::query_as::<_, RequestRow>(
            &format!(
                "SELECT {SELECT_COLS} FROM staff_requests \
                 WHERE from_user_id = $1 ORDER BY created_at DESC"
            ),
        )
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        rows.into_iter().map(request_from_row).collect()
    }

    async fn find_pending(&self) -> Result<Vec<StaffRequest>, DomainError> {
        let rows = sqlx::query_as::<_, RequestRow>(
            &format!(
                "SELECT {SELECT_COLS} FROM staff_requests \
                 WHERE status = 'pending' ORDER BY created_at ASC"
            ),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        rows.into_iter().map(request_from_row).collect()
    }

    async fn find_pending_for_board(&self, slug: &Slug) -> Result<Vec<StaffRequest>, DomainError> {
        let rows = sqlx::query_as::<_, RequestRow>(
            &format!(
                "SELECT {SELECT_COLS} FROM staff_requests \
                 WHERE status = 'pending' AND target_slug = $1 ORDER BY created_at ASC"
            ),
        )
        .bind(slug.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        rows.into_iter().map(request_from_row).collect()
    }

    async fn update_status(
        &self,
        id:           StaffRequestId,
        status:       StaffRequestStatus,
        reviewed_by:  UserId,
        note:         Option<String>,
    ) -> Result<(), DomainError> {
        let rows_affected = sqlx::query(
            "UPDATE staff_requests
             SET status = $2, reviewed_by = $3, review_note = $4, updated_at = NOW()
             WHERE id = $1 AND status = 'pending'",
        )
        .bind(id.0)
        .bind(status.to_string())
        .bind(reviewed_by.0)
        .bind(note)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(DomainError::not_found(format!(
                "staff_request:{} (not found or already reviewed)", id.0
            )));
        }
        Ok(())
    }
}
