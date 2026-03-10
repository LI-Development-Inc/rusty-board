//! PostgreSQL implementation of `UserRepository`.
//! Uses runtime sqlx queries — no query! macros.

use async_trait::async_trait;
use domains::errors::DomainError;
use domains::models::{BoardId, Page, Paginated, PasswordHash, Role, User, UserId};
use domains::ports::UserRepository;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// PostgreSQL-backed `UserRepository`.
#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    /// Construct a `PgUserRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_err(e: sqlx::Error, resource: impl Into<String>) -> DomainError {
    match e {
        sqlx::Error::RowNotFound => DomainError::not_found(resource),
        other => DomainError::internal(other.to_string()),
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id:            Uuid,
    username:      String,
    password_hash: String,
    role:          String,
    is_active:     bool,
    created_at:    chrono::DateTime<chrono::Utc>,
}

fn user_from_row(r: UserRow) -> Result<User, DomainError> {
    Ok(User {
        id:            UserId(r.id),
        username:      r.username,
        password_hash: PasswordHash::new(r.password_hash),
        role:          Role::from_str(&r.role).map_err(DomainError::internal)?,
        is_active:     r.is_active,
        created_at:    r.created_at,
    })
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, id: UserId) -> Result<User, DomainError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, is_active, created_at \
             FROM users WHERE id = $1"
        )
        .bind(id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_err(e, id.to_string()))?;
        user_from_row(row)
    }

    async fn find_by_username(&self, username: &str) -> Result<User, DomainError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, is_active, created_at \
             FROM users WHERE username = $1"
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_err(e, format!("user/{username}")))?;
        user_from_row(row)
    }

    async fn save(&self, user: &User) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, is_active, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
               username = EXCLUDED.username,
               password_hash = EXCLUDED.password_hash,
               role = EXCLUDED.role,
               is_active = EXCLUDED.is_active"
        )
        .bind(user.id.0)
        .bind(&user.username)
        .bind(&user.password_hash.0)
        .bind(user.role.to_string())
        .bind(user.is_active)
        .bind(user.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn deactivate(&self, id: UserId) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE users SET is_active = false WHERE id = $1"
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

    async fn find_all(&self, page: Page) -> Result<Paginated<User>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, is_active, created_at \
             FROM users ORDER BY created_at ASC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter()
            .map(user_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn find_owned_boards(&self, user_id: UserId) -> Result<Vec<BoardId>, DomainError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT board_id FROM board_owners WHERE user_id = $1"
        )
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(|(id,)| BoardId(id)).collect())
    }

    async fn find_volunteer_boards(&self, user_id: UserId) -> Result<Vec<BoardId>, DomainError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT board_id FROM board_volunteers WHERE user_id = $1"
        )
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(|(id,)| BoardId(id)).collect())
    }

    async fn add_board_owner(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO board_owners (board_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        )
        .bind(board_id.0)
        .bind(user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn remove_board_owner(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError> {
        sqlx::query(
            "DELETE FROM board_owners WHERE board_id = $1 AND user_id = $2"
        )
        .bind(board_id.0)
        .bind(user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn add_volunteer(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO board_volunteers (board_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        )
        .bind(board_id.0)
        .bind(user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn remove_volunteer(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError> {
        sqlx::query(
            "DELETE FROM board_volunteers WHERE board_id = $1 AND user_id = $2"
        )
        .bind(board_id.0)
        .bind(user_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }
}
