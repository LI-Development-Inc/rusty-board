//! PostgreSQL implementation of `SessionRepository`.
//!
//! Persists server-side cookie sessions in the `user_sessions` table (migration 016).
//! Used exclusively by `CookieAuthProvider`. JWT auth ignores this entirely.
//!
//! Uses runtime sqlx queries — no `query!` macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::UserId;
use domains::ports::{Session, SessionRepository};
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL-backed `SessionRepository`.
///
/// Concrete sessions are stored in `user_sessions`. Expired sessions are not
/// automatically deleted — call [`PgSessionRepository::purge_expired`] from a
/// maintenance task or use the `pg_cron` extension for periodic cleanup.
#[derive(Clone)]
pub struct PgSessionRepository {
    pool: PgPool,
}

impl PgSessionRepository {
    /// Construct a `PgSessionRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    session_id:  String,
    user_id:     Uuid,
    claims_json: String,
    expires_at:  DateTime<Utc>,
}

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn save(&self, session: &Session) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO user_sessions (session_id, user_id, claims_json, expires_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (session_id) DO UPDATE
               SET claims_json = EXCLUDED.claims_json,
                   expires_at  = EXCLUDED.expires_at",
        )
        .bind(&session.session_id)
        .bind(session.user_id.0)
        .bind(&session.claims_json)
        .bind(session.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, session_id: &str) -> Result<Session, DomainError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT session_id, user_id, claims_json, expires_at
             FROM user_sessions
             WHERE session_id = $1 AND expires_at > NOW()",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?
        .ok_or_else(|| DomainError::auth())?;

        Ok(Session {
            session_id:  row.session_id,
            user_id:     UserId(row.user_id),
            claims_json: row.claims_json,
            expires_at:  row.expires_at,
        })
    }

    async fn delete(&self, session_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM user_sessions WHERE session_id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn delete_for_user(&self, user_id: UserId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
            .bind(user_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn purge_expired(&self) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM user_sessions WHERE expires_at <= NOW()")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }
}
