//! PostgreSQL implementation of `PostRepository`.
//! Uses runtime sqlx queries — no query! macros.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domains::errors::DomainError;
use domains::models::{BoardId, ContentHash, IpHash, OverboardPost, Page, Paginated, Post, PostId, ThreadId};
use domains::ports::PostRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL-backed `PostRepository`.
#[derive(Clone)]
pub struct PgPostRepository {
    pool: PgPool,
}

impl PgPostRepository {
    /// Construct a `PgPostRepository` backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_err(e: sqlx::Error, resource: impl Into<String>) -> DomainError {
    match e {
        sqlx::Error::RowNotFound => DomainError::not_found(resource),
        other => DomainError::internal(other.to_string()),
    }
}

#[derive(sqlx::FromRow)]
struct PostRow {
    id:          Uuid,
    thread_id:   Uuid,
    body:        String,
    ip_hash:     String,
    name:        Option<String>,
    tripcode:    Option<String>,
    email:       Option<String>,
    created_at:  DateTime<Utc>,
    post_number: i64,
    pinned:      bool,
}

fn post_from_row(r: PostRow) -> Post {
    Post {
        id:          PostId(r.id),
        thread_id:   ThreadId(r.thread_id),
        body:        r.body,
        ip_hash:     IpHash::new(r.ip_hash),
        name:        r.name,
        tripcode:    r.tripcode,
        email:       r.email,
        created_at:  r.created_at,
        post_number: r.post_number as u64,
        pinned:      r.pinned,
    }
}

#[async_trait]
impl PostRepository for PgPostRepository {
    async fn find_by_id(&self, id: PostId) -> Result<Post, DomainError> {
        let row = sqlx::query_as::<_, PostRow>(
            "SELECT id, thread_id, body, ip_hash, name, tripcode, email, created_at, post_number, pinned \
             FROM posts WHERE id = $1"
        )
        .bind(id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_err(e, id.to_string()))?;
        Ok(post_from_row(row))
    }

    async fn find_by_thread(&self, thread_id: ThreadId, page: Page) -> Result<Paginated<Post>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT id, thread_id, body, ip_hash, name, tripcode, email, created_at, post_number, pinned \
             FROM posts WHERE thread_id = $1 \
             ORDER BY post_number ASC LIMIT $2 OFFSET $3"
        )
        .bind(thread_id.0)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM posts WHERE thread_id = $1"
        )
        .bind(thread_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(post_from_row).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn find_by_ip_hash(&self, ip_hash: &IpHash) -> Result<Vec<Post>, DomainError> {
        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT id, thread_id, body, ip_hash, name, tripcode, email, created_at, post_number, pinned \
             FROM posts WHERE ip_hash = $1 ORDER BY created_at DESC"
        )
        .bind(&ip_hash.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(rows.into_iter().map(post_from_row).collect())
    }

    async fn find_recent_hashes(&self, board_id: BoardId, limit: u32) -> Result<Vec<ContentHash>, DomainError> {
        #[derive(sqlx::FromRow)]
        struct BodyRow { body: String }

        let rows = sqlx::query_as::<_, BodyRow>(
            "SELECT p.body FROM posts p
             JOIN threads t ON t.id = p.thread_id
             WHERE t.board_id = $1
             ORDER BY p.created_at DESC LIMIT $2"
        )
        .bind(board_id.0)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        use sha2::{Digest, Sha256};
        let hashes = rows.into_iter().map(|r| {
            let mut h = Sha256::new();
            h.update(r.body.as_bytes());
            ContentHash::new(hex::encode(h.finalize()))
        }).collect();
        Ok(hashes)
    }

    async fn save(&self, post: &Post) -> Result<(PostId, u64), DomainError> {
        let row: (Uuid, i64) = sqlx::query_as(
            "WITH board_cte AS (
                 SELECT t.board_id FROM threads t WHERE t.id = $2
             ),
             bump AS (
                 UPDATE boards
                 SET    post_counter = post_counter + 1
                 WHERE  id = (SELECT board_id FROM board_cte)
                 RETURNING post_counter
             )
             INSERT INTO posts (id, thread_id, post_number, body, ip_hash, name, tripcode, email, pinned, created_at)
             SELECT $1, $2, bump.post_counter, $3, $4, $5, $6, $7, $8, $9
             FROM   bump
             RETURNING id, post_number"
        )
        .bind(post.id.0)
        .bind(post.thread_id.0)
        .bind(&post.body)
        .bind(&post.ip_hash.0)
        .bind(&post.name)
        .bind(&post.tripcode)
        .bind(&post.email)
        .bind(post.pinned)
        .bind(post.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok((PostId(row.0), row.1 as u64))
    }

    async fn delete(&self, id: PostId) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM posts WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::not_found(id.to_string()));
        }
        Ok(())
    }

    async fn delete_by_ip_in_thread(
        &self,
        ip_hash: &IpHash,
        thread_id: ThreadId,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "DELETE FROM posts WHERE ip_hash = $1 AND thread_id = $2"
        )
        .bind(&ip_hash.0)
        .bind(thread_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn save_attachments(&self, attachments: &[domains::models::Attachment]) -> Result<(), DomainError> {
        for a in attachments {
            sqlx::query(
                "INSERT INTO attachments (id, post_id, filename, mime, hash, size_kb, media_key, thumbnail_key, spoiler) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
            )
            .bind(a.id)
            .bind(a.post_id.0)
            .bind(&a.filename)
            .bind(&a.mime)
            .bind(&a.hash.0)
            .bind(a.size_kb as i32)
            .bind(&a.media_key.0)
            .bind(a.thumbnail_key.as_ref().map(|k| &k.0))
            .bind(a.spoiler)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        }
        Ok(())
    }

    async fn find_attachments_by_post_ids(
        &self,
        post_ids: &[PostId],
    ) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, DomainError> {
        if post_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let ids: Vec<Uuid> = post_ids.iter().map(|p| p.0).collect();

        #[derive(sqlx::FromRow)]
        struct AttachRow {
            id:            Uuid,
            post_id:       Uuid,
            filename:      String,
            mime:          String,
            hash:          String,
            size_kb:       i32,
            media_key:     String,
            thumbnail_key: Option<String>,
            spoiler:       bool,
        }

        let rows = sqlx::query_as::<_, AttachRow>(
            "SELECT id, post_id, filename, mime, hash, size_kb, media_key, thumbnail_key, spoiler \
             FROM attachments WHERE post_id = ANY($1) ORDER BY id ASC"
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        use domains::models::{Attachment, ContentHash, MediaKey};
        let mut map: std::collections::HashMap<PostId, Vec<Attachment>> = std::collections::HashMap::new();
        for r in rows {
            let a = Attachment {
                id:            r.id,
                post_id:       PostId(r.post_id),
                filename:      r.filename,
                mime:          r.mime,
                hash:          ContentHash::new(r.hash),
                size_kb:       r.size_kb as u32,
                media_key:     MediaKey::new(r.media_key),
                thumbnail_key: r.thumbnail_key.map(MediaKey::new),
                spoiler:       r.spoiler,
            };
            map.entry(PostId(r.post_id)).or_default().push(a);
        }
        Ok(map)
    }

    async fn find_overboard(&self, page: Page) -> Result<Paginated<OverboardPost>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset = page.offset(page_size) as i64;
        let limit  = page_size as i64;

        #[derive(sqlx::FromRow)]
        struct OverboardRow {
            id:          Uuid,
            thread_id:   Uuid,
            board_slug:  String,
            body:        String,
            name:        Option<String>,
            tripcode:    Option<String>,
            ip_hash:     String,
            created_at:  chrono::DateTime<chrono::Utc>,
            post_number: i64,
        }

        let rows = sqlx::query_as::<_, OverboardRow>(
            "SELECT p.id, p.thread_id, b.slug AS board_slug, p.body, p.name, p.tripcode, p.ip_hash, p.created_at, p.post_number \
             FROM posts p \
             JOIN threads t ON t.id = p.thread_id \
             JOIN boards  b ON b.id = t.board_id \
             ORDER BY p.created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(|r| OverboardPost {
            id:          PostId(r.id),
            thread_id:   ThreadId(r.thread_id),
            board_slug:  r.board_slug,
            body:        r.body,
            name:        r.name,
            tripcode:    r.tripcode,
            ip_hash:     IpHash(r.ip_hash),
            created_at:  r.created_at,
            post_number: r.post_number as u64,
        }).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    /// Full-text search using PostgreSQL's `plainto_tsquery` against the GIN index on `posts.body`.
    ///
    /// Results are ranked by `ts_rank` descending. Scoped to a single board via the
    /// `threads.board_id` join. Returns an empty page when no results match.
    async fn search_fulltext(
        &self,
        board_id: BoardId,
        query: &str,
        page: Page,
    ) -> Result<Paginated<Post>, DomainError> {
        let page_size = Page::DEFAULT_PAGE_SIZE;
        let offset    = page.offset(page_size) as i64;
        let limit     = page_size as i64;

        // Uses the GIN index on posts.body (created in migration 006).
        // `plainto_tsquery` safely handles user input without injection risk.
        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT p.id, p.thread_id, p.body, p.ip_hash, p.name, p.tripcode, p.email,
                    p.created_at, p.post_number
             FROM   posts p
             JOIN   threads t ON t.id = p.thread_id
             WHERE  t.board_id = $1
               AND  to_tsvector('english', p.body) @@ plainto_tsquery('english', $2)
             ORDER  BY ts_rank(to_tsvector('english', p.body),
                               plainto_tsquery('english', $2)) DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(board_id.0)
        .bind(query)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM   posts p
             JOIN   threads t ON t.id = p.thread_id
             WHERE  t.board_id = $1
               AND  to_tsvector('english', p.body) @@ plainto_tsquery('english', $2)",
        )
        .bind(board_id.0)
        .bind(query)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        let items = rows.into_iter().map(post_from_row).collect();
        Ok(Paginated::new(items, total as u64, page, page_size))
    }

    async fn find_all_by_thread(&self, thread_id: ThreadId) -> Result<Vec<Post>, DomainError> {
        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT id, thread_id, body, ip_hash, name, tripcode, email, created_at, post_number, pinned
             FROM   posts
             WHERE  thread_id = $1
             ORDER  BY post_number ASC
             LIMIT  500",
        )
        .bind(thread_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        Ok(rows.into_iter().map(post_from_row).collect())
    }

    async fn find_thread_id_by_post_number(
        &self,
        board_id: BoardId,
        post_number: u64,
    ) -> Result<Option<ThreadId>, DomainError> {
        // Looks up the thread containing the given board-scoped post number.
        // Used by the cross-board >>>/{slug}/{N} redirect handler.
        let row: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT p.thread_id
             FROM   posts p
             JOIN   threads t ON t.id = p.thread_id
             WHERE  t.board_id = $1
               AND  p.post_number = $2
             LIMIT  1",
        )
        .bind(board_id.0)
        .bind(post_number as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        Ok(row.map(|(id,)| ThreadId(id)))
    }

    async fn set_pinned(&self, id: PostId, pinned: bool) -> Result<(), DomainError> {
        sqlx::query("UPDATE posts SET pinned = $2 WHERE id = $1")
            .bind(id.0)
            .bind(pinned)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }

    async fn find_oldest_unpinned_reply(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<PostId>, DomainError> {
        // Returns the OP-excluded, non-pinned reply with the smallest post_number.
        // The OP is the post with the minimum post_number in the thread.
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM posts
             WHERE  thread_id = $1
               AND  pinned = FALSE
               AND  post_number > (
                   SELECT MIN(post_number) FROM posts WHERE thread_id = $1
               )
             ORDER  BY post_number ASC
             LIMIT  1",
        )
        .bind(thread_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(row.map(|(id,)| PostId(id)))
    }

    async fn find_attachment_by_hash(
        &self,
        hash: &domains::models::ContentHash,
    ) -> Result<Option<domains::models::Attachment>, DomainError> {
        #[derive(sqlx::FromRow)]
        struct AttRow {
            id:            uuid::Uuid,
            post_id:       uuid::Uuid,
            filename:      String,
            mime:          String,
            hash:          String,
            size_kb:       i32,
            media_key:     String,
            thumbnail_key: Option<String>,
            spoiler:       bool,
        }
        let row = sqlx::query_as::<_, AttRow>(
            "SELECT id, post_id, filename, mime, hash, size_kb, media_key, thumbnail_key, spoiler
             FROM attachments WHERE hash = $1 LIMIT 1",
        )
        .bind(&hash.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(e.to_string()))?;

        Ok(row.map(|r| domains::models::Attachment {
            id:            r.id,
            post_id:       PostId(r.post_id),
            filename:      r.filename,
            mime:          r.mime,
            hash:          domains::models::ContentHash(r.hash),
            size_kb:       r.size_kb as u32,
            media_key:     domains::models::MediaKey::new(r.media_key),
            thumbnail_key: r.thumbnail_key.map(domains::models::MediaKey::new),
            spoiler:       r.spoiler,
        }))
    }

    async fn delete_by_id(&self, id: PostId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM posts WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        Ok(())
    }
}
