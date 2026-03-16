//! PostgreSQL implementation of `BoardRepository`.
//!
//! Uses runtime `sqlx::query_as` / `sqlx::query` calls instead of compile-time
//! `query!` macros so the crate builds without a live database or sqlx-data.json.

use async_trait::async_trait;
use domains::errors::DomainError;
use domains::models::{Board, BoardConfig, BoardId, FileSizeKb, Page, Paginated, Slug};
use domains::ports::BoardRepository;
use sqlx::{PgPool, Row};
use chrono;
use tracing::instrument;
use uuid::Uuid;

/// PostgreSQL-backed `BoardRepository`.
#[derive(Clone)]
pub struct PgBoardRepository {
    pool: PgPool,
}

impl PgBoardRepository {
    /// Construct a `PgBoardRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_sqlx_err(e: sqlx::Error, resource: impl Into<String>) -> DomainError {
    match e {
        sqlx::Error::RowNotFound => DomainError::not_found(resource),
        other => DomainError::internal(other.to_string()),
    }
}

#[derive(sqlx::FromRow)]
struct BoardRow {
    id:         Uuid,
    slug:       String,
    title:      String,
    rules:      String,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn board_from_row(r: BoardRow) -> Result<Board, DomainError> {
    Ok(Board {
        id:         BoardId(r.id),
        slug:       Slug::new(&r.slug).map_err(|e| DomainError::internal(e.to_string()))?,
        title:      r.title,
        rules:      r.rules,
        created_at: r.created_at,
    })
}

#[derive(sqlx::FromRow)]
struct BoardConfigRow {
    bump_limit:             i32,
    max_threads:            i32,
    max_files:              i16,
    max_file_size_kb:       i32,
    allowed_mimes:          Vec<String>,
    max_post_length:        i32,
    rate_limit_enabled:     bool,
    rate_limit_window_secs: i32,
    rate_limit_posts:       i16,
    spam_filter_enabled:    bool,
    spam_score_threshold:   f32,
    duplicate_check:        bool,
    forced_anon:            bool,
    allow_sage:             bool,
    allow_tripcodes:        bool,
    captcha_required:       bool,
    nsfw:                   bool,
    search_enabled:         bool,
    archive_enabled:        bool,
    federation_enabled:     bool,
    link_blacklist:              Vec<String>,
    name_rate_limit_window_secs: i32,
}

fn board_config_from_row(r: BoardConfigRow) -> BoardConfig {
    BoardConfig {
        bump_limit:             r.bump_limit as u32,
        max_threads:            r.max_threads as u32,
        max_files:              r.max_files as u8,
        max_file_size:          FileSizeKb(r.max_file_size_kb as u32),
        allowed_mimes:          r.allowed_mimes,
        max_post_length:        r.max_post_length as u32,
        rate_limit_enabled:     r.rate_limit_enabled,
        rate_limit_window_secs: r.rate_limit_window_secs as u32,
        rate_limit_posts:       r.rate_limit_posts as u32,
        spam_filter_enabled:    r.spam_filter_enabled,
        spam_score_threshold:   r.spam_score_threshold,
        duplicate_check:        r.duplicate_check,
        forced_anon:            r.forced_anon,
        allow_sage:             r.allow_sage,
        allow_tripcodes:        r.allow_tripcodes,
        captcha_required:       r.captcha_required,
        nsfw:                   r.nsfw,
        search_enabled:              r.search_enabled,
        archive_enabled:             r.archive_enabled,
        federation_enabled:          r.federation_enabled,
        link_blacklist:              r.link_blacklist,
        name_rate_limit_window_secs: r.name_rate_limit_window_secs as u32,
    }
}

#[async_trait]
impl BoardRepository for PgBoardRepository {
    #[instrument(skip(self), fields(board_id = %id))]
    async fn find_by_id(&self, id: BoardId) -> Result<Board, DomainError> {
        let row = sqlx::query_as::<_, BoardRow>(
            "SELECT id, slug, title, rules, created_at FROM boards WHERE id = $1"
        )
        .bind(id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_sqlx_err(e, id.to_string()))?;
        board_from_row(row)
    }

    #[instrument(skip(self), fields(slug = %slug))]
    async fn find_by_slug(&self, slug: &Slug) -> Result<Board, DomainError> {
        let row = sqlx::query_as::<_, BoardRow>(
            "SELECT id, slug, title, rules, created_at FROM boards WHERE slug = $1"
        )
        .bind(slug.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_sqlx_err(e, format!("board/{slug}")))?;
        board_from_row(row)
    }

    #[instrument(skip(self), fields(page = page.0))]
    async fn find_all(&self, page: Page) -> Result<Paginated<Board>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, BoardRow>(
            "SELECT id, slug, title, rules, created_at FROM boards \
             ORDER BY created_at ASC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM boards")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter()
            .map(board_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    /// Upsert a board record and guarantee its `board_configs` row exists.
    ///
    /// Conflicts on `slug` (the natural unique key) rather than `id`, so that
    /// re-running seeds or imports with fresh UUIDs updates the existing row
    /// instead of failing with a unique-constraint violation.
    ///
    /// The `RETURNING id` clause captures the canonical UUID (the one already
    /// in the database when the slug conflicts) so the subsequent
    /// `board_configs` insert targets the correct primary key.
    ///
    /// Returns `DomainError::Internal` if either query fails.
    async fn save(&self, board: &Board) -> Result<(), DomainError> {
        // Upsert on slug (the natural key) so re-running seed doesn't fail on duplicate slugs.
        // When slug already exists, update title/rules but keep the original id and created_at.
        let board_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO boards (id, slug, title, rules, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (slug) DO UPDATE
             SET title = EXCLUDED.title, rules = EXCLUDED.rules
             RETURNING id"
        )
        .bind(board.id.0)
        .bind(board.slug.as_str())
        .bind(&board.title)
        .bind(&board.rules)
        .bind(board.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        // Ensure a default board_configs row exists (all columns have DB defaults).
        // ON CONFLICT DO NOTHING preserves any config set by prior updates.
        sqlx::query(
            "INSERT INTO board_configs (board_id) VALUES ($1) ON CONFLICT DO NOTHING"
        )
        .bind(board_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: BoardId) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM boards WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::not_found(id.to_string()));
        }
        Ok(())
    }

    #[instrument(skip(self), fields(board_id = %board_id))]
    async fn find_config(&self, board_id: BoardId) -> Result<BoardConfig, DomainError> {
        let row = sqlx::query_as::<_, BoardConfigRow>(
            "SELECT bump_limit, max_threads, max_files, max_file_size_kb, allowed_mimes, max_post_length,
                    rate_limit_enabled, rate_limit_window_secs, rate_limit_posts,
                    spam_filter_enabled, spam_score_threshold, duplicate_check,
                    forced_anon, allow_sage, allow_tripcodes, captcha_required, nsfw,
                    search_enabled, archive_enabled, federation_enabled,
                    link_blacklist, name_rate_limit_window_secs
             FROM board_configs WHERE board_id = $1"
        )
        .bind(board_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_sqlx_err(e, format!("board_config/{board_id}")))?;
        Ok(board_config_from_row(row))
    }

    async fn save_config(&self, board_id: BoardId, config: &BoardConfig) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO board_configs (
                board_id, bump_limit, max_threads, max_files, max_file_size_kb, allowed_mimes,
                max_post_length, rate_limit_enabled, rate_limit_window_secs, rate_limit_posts,
                spam_filter_enabled, spam_score_threshold, duplicate_check,
                forced_anon, allow_sage, allow_tripcodes, captcha_required, nsfw,
                search_enabled, archive_enabled, federation_enabled,
                link_blacklist, name_rate_limit_window_secs
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
             ON CONFLICT (board_id) DO UPDATE SET
                bump_limit = EXCLUDED.bump_limit,
                max_threads = EXCLUDED.max_threads,
                max_files = EXCLUDED.max_files,
                max_file_size_kb = EXCLUDED.max_file_size_kb,
                allowed_mimes = EXCLUDED.allowed_mimes,
                max_post_length = EXCLUDED.max_post_length,
                rate_limit_enabled = EXCLUDED.rate_limit_enabled,
                rate_limit_window_secs = EXCLUDED.rate_limit_window_secs,
                rate_limit_posts = EXCLUDED.rate_limit_posts,
                spam_filter_enabled = EXCLUDED.spam_filter_enabled,
                spam_score_threshold = EXCLUDED.spam_score_threshold,
                duplicate_check = EXCLUDED.duplicate_check,
                forced_anon = EXCLUDED.forced_anon,
                allow_sage = EXCLUDED.allow_sage,
                allow_tripcodes = EXCLUDED.allow_tripcodes,
                captcha_required = EXCLUDED.captcha_required,
                nsfw = EXCLUDED.nsfw,
                search_enabled = EXCLUDED.search_enabled,
                archive_enabled = EXCLUDED.archive_enabled,
                federation_enabled = EXCLUDED.federation_enabled,
                link_blacklist = EXCLUDED.link_blacklist,
                name_rate_limit_window_secs = EXCLUDED.name_rate_limit_window_secs"
        )
        .bind(board_id.0)
        .bind(config.bump_limit as i32)
        .bind(config.max_threads as i32)
        .bind(config.max_files as i16)
        .bind(config.max_file_size.0 as i32)
        .bind(&config.allowed_mimes)
        .bind(config.max_post_length as i32)
        .bind(config.rate_limit_enabled)
        .bind(config.rate_limit_window_secs as i32)
        .bind(config.rate_limit_posts as i16)
        .bind(config.spam_filter_enabled)
        .bind(config.spam_score_threshold)
        .bind(config.duplicate_check)
        .bind(config.forced_anon)
        .bind(config.allow_sage)
        .bind(config.allow_tripcodes)
        .bind(config.captcha_required)
        .bind(config.nsfw)
        .bind(config.search_enabled)
        .bind(config.archive_enabled)
        .bind(config.federation_enabled)
        .bind(&config.link_blacklist)
        .bind(config.name_rate_limit_window_secs as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

}

#[async_trait]
impl domains::ports::BoardVolunteerRepository for PgBoardRepository {
    async fn list_volunteers(
        &self,
        board_id: BoardId,
    ) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, DomainError> {
        let rows = sqlx::query(
            "SELECT bv.user_id, u.username, bv.assigned_at
             FROM board_volunteers bv
             JOIN users u ON u.id = bv.user_id
             WHERE bv.board_id = $1
             ORDER BY bv.assigned_at ASC"
        )
        .bind(board_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        Ok(rows.into_iter().map(|r| {
            let uid: uuid::Uuid = r.get("user_id");
            let uname: String   = r.get("username");
            let at: chrono::DateTime<chrono::Utc> = r.get("assigned_at");
            (domains::models::UserId(uid), uname, at)
        }).collect())
    }

    async fn add_volunteer_by_username(
        &self,
        board_id:    BoardId,
        username:    &str,
        assigned_by: domains::models::UserId,
    ) -> Result<(), DomainError> {
        let row = sqlx::query("SELECT id FROM users WHERE username = $1 AND is_active = true")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?
            .ok_or_else(|| DomainError::not_found(format!("user '{username}' not found")))?;
        let user_id: uuid::Uuid = row.get("id");

        sqlx::query(
            "INSERT INTO board_volunteers (board_id, user_id, assigned_by)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        )
        .bind(board_id.0)
        .bind(user_id)
        .bind(assigned_by.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn remove_volunteer(
        &self,
        board_id: BoardId,
        user_id:  domains::models::UserId,
    ) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM board_volunteers WHERE board_id = $1 AND user_id = $2")
            .bind(board_id.0)
            .bind(user_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }
}
