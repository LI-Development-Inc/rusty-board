//! Port trait definitions — the complete set of external boundaries in rusty-board.
//!
//! Every external dependency (database, object storage, authentication, rate limiting,
//! media processing) is represented here as an `async` trait. Concrete implementations
//! live in adapter crates and are selected at compile time via Cargo feature flags.
//!
//! # Rules for port traits
//! - `Send + Sync + 'static` bounds on every trait (required for async dispatch)
//! - `#[async_trait::async_trait]` on every trait and impl (required for Axum 0.7 Send bounds)
//! - Return `Result<T, DomainError>` — never adapter-specific error types
//! - `///` doc comment on every method
//! - `#[cfg_attr(any(test, feature = "testing"), mockall::automock)]` on every trait for unit test mock generation
//!
//! # Adding a new port
//! See `PORTS.md` for the required steps. The trait definition here must be
//! accompanied by an entry in `PORTS.md` before any adapter is written.

use std::time::Duration;

use bytes::Bytes;
use crate::models::OverboardPost;
use chrono::{DateTime, Utc};
use mime::Mime;

use async_trait::async_trait;
use crate::errors::DomainError;
use crate::models::{
    AuditEntry, Ban, BanId, Board, BoardConfig, BoardId, Claims, ContentHash, Flag, FlagId,
    FlagResolution, IpHash, MediaKey, Page, Paginated, PasswordHash, Post, PostId,
    StaffRequest, StaffRequestId, StaffRequestStatus,
    Thread, ThreadId, ThreadSummary, Token, User, UserId,
};

// ─── Repository Ports ────────────────────────────────────────────────────────

/// Persistence boundary for `Board` entities and their associated `BoardConfig`.
///
/// The composition root wires this to `PgBoardRepository` (feature: `db-postgres`).
/// Future adapters: `SqliteBoardRepository` (v1.2), `SurrealBoardRepository` (v2.0).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait BoardRepository: Send + Sync + 'static {
    /// Fetch a board by its UUID.
    ///
    /// Returns `DomainError::NotFound` if no board with the given id exists.
    async fn find_by_id(&self, id: BoardId) -> Result<Board, DomainError>;

    /// Fetch a board by its slug.
    ///
    /// Returns `DomainError::NotFound` if no board with the given slug exists.
    async fn find_by_slug(&self, slug: &crate::models::Slug) -> Result<Board, DomainError>;

    /// Paginated list of all boards ordered by creation date ascending.
    async fn find_all(&self, page: Page) -> Result<Paginated<Board>, DomainError>;

    /// Insert (if new) or update (if existing) a board record.
    ///
    /// Implementors **must** also ensure that a corresponding `board_configs` row
    /// exists for the board after this call returns. If the board is new, a default
    /// config row must be created. If the board already exists, the config row must
    /// be left unchanged. This contract allows callers (and the middleware layer) to
    /// always `find_config` without a prior existence check.
    ///
    /// Returns `DomainError::Internal` if the underlying store is unavailable.
    async fn save(&self, board: &Board) -> Result<(), DomainError>;

    /// Delete a board and cascade-delete all child records (threads, posts, config).
    ///
    /// Returns `DomainError::NotFound` if the board does not exist.
    async fn delete(&self, id: BoardId) -> Result<(), DomainError>;

    /// Fetch the `BoardConfig` for the given board.
    ///
    /// Returns `DomainError::NotFound` if no config row exists (should not happen
    /// in normal operation — config is created automatically when the board is created).
    async fn find_config(&self, board_id: BoardId) -> Result<BoardConfig, DomainError>;

    /// Persist an updated `BoardConfig` for the given board.
    ///
    /// Overwrites all fields. The caller is responsible for merging partial updates.
    async fn save_config(&self, board_id: BoardId, config: &BoardConfig) -> Result<(), DomainError>;

}

/// Volunteer management — kept separate from `BoardRepository` so that
/// `mockall::automock` on `BoardRepository` is not confused by the
/// tuple return type. Implemented by the same `PgBoardRepository` concrete type.
#[async_trait]
pub trait BoardVolunteerRepository: Send + Sync + 'static {
    /// List volunteers for a board. Returns `(UserId, username, assigned_at)`.
    async fn list_volunteers(&self, board_id: BoardId)
        -> Result<Vec<(crate::models::UserId, String, chrono::DateTime<chrono::Utc>)>, DomainError>;

    /// Add a volunteer by username (looks up user_id from username).
    async fn add_volunteer_by_username(
        &self,
        board_id:    BoardId,
        username:    &str,
        assigned_by: crate::models::UserId,
    ) -> Result<(), DomainError>;

    /// Remove a volunteer from a board.
    async fn remove_volunteer(
        &self,
        board_id: BoardId,
        user_id:  crate::models::UserId,
    ) -> Result<(), DomainError>;
}

/// Persistence boundary for `Thread` entities.
///
/// The composition root wires this to `PgThreadRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait ThreadRepository: Send + Sync + 'static {
    /// Fetch a thread by its UUID.
    ///
    /// Returns `DomainError::NotFound` if no thread with the given id exists.
    async fn find_by_id(&self, id: ThreadId) -> Result<Thread, DomainError>;

    /// Paginated thread list for a board, ordered by `bumped_at DESC`.
    ///
    /// Sticky threads appear first regardless of bump time.
    async fn find_by_board(&self, board_id: BoardId, page: Page) -> Result<Paginated<Thread>, DomainError>;

    /// All threads on a board for the catalog view — no pagination.
    ///
    /// Returns only OP post summary (body preview + thumbnail). Used to render the
    /// catalog grid. Ordered by `bumped_at DESC`, sticky first.
    async fn find_catalog(&self, board_id: BoardId) -> Result<Vec<ThreadSummary>, DomainError>;

    /// Insert a new thread row and return the assigned `ThreadId`.
    async fn save(&self, thread: &Thread) -> Result<ThreadId, DomainError>;

    /// Update `bumped_at` and increment `reply_count` atomically.
    ///
    /// This is called after every post that does not sage.
    async fn bump(&self, id: ThreadId, bumped_at: DateTime<Utc>) -> Result<(), DomainError>;

    /// Set the `op_post_id` column after the opening post has been inserted.
    ///
    /// Called once per thread immediately after the OP post is saved.
    async fn set_op_post(&self, id: ThreadId, op_post_id: PostId) -> Result<(), DomainError>;

    /// Set the sticky flag to `sticky` for the given thread.
    async fn set_sticky(&self, id: ThreadId, sticky: bool) -> Result<(), DomainError>;

    /// Set the closed flag to `closed` for the given thread.
    async fn set_closed(&self, id: ThreadId, closed: bool) -> Result<(), DomainError>;

    /// Count threads on a board (used to determine whether pruning is necessary).
    async fn count_by_board(&self, board_id: BoardId) -> Result<u32, DomainError>;

    /// Delete the oldest (by `bumped_at`) non-sticky threads until at most `keep` threads remain.
    ///
    /// Returns the number of threads deleted.
    async fn prune_oldest(&self, board_id: BoardId, keep: u32) -> Result<u32, DomainError>;

    /// Delete a thread and all its posts (cascade).
    ///
    /// Returns `DomainError::NotFound` if the thread does not exist.
    async fn delete(&self, id: ThreadId) -> Result<(), DomainError>;
}

/// Persistence boundary for `Post` entities.
///
/// The composition root wires this to `PgPostRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait PostRepository: Send + Sync + 'static {
    /// Fetch a post by its UUID.
    ///
    /// Returns `DomainError::NotFound` if no post with the given id exists.
    async fn find_by_id(&self, id: PostId) -> Result<Post, DomainError>;

    /// Paginated posts in a thread, ordered by `created_at ASC`.
    async fn find_by_thread(&self, thread_id: ThreadId, page: Page) -> Result<Paginated<Post>, DomainError>;

    /// All posts by a given IP hash across all boards.
    ///
    /// Used in moderation to find all posts by a poster before issuing a ban.
    async fn find_by_ip_hash(&self, ip_hash: &IpHash) -> Result<Vec<Post>, DomainError>;

    /// The `ContentHash` values of the most recent `limit` posts on a board.
    ///
    /// Used for duplicate content detection in spam heuristics. Ordered by
    /// `created_at DESC`, limited to `limit` rows.
    async fn find_recent_hashes(&self, board_id: BoardId, limit: u32) -> Result<Vec<ContentHash>, DomainError>;

    /// Insert a new post and return the assigned `PostId`.
    /// Persist a new post, atomically claiming the next board-scoped post number.
    ///
    /// Returns `(PostId, post_number)` where `post_number` is the board-sequential
    /// number assigned to this post. The post's UUID is returned alongside because
    /// the repository generates UUIDs for new records.
    async fn save(&self, post: &Post) -> Result<(PostId, u64), DomainError>;

    /// Delete a single post.
    ///
    /// Does not cascade to the thread (thread deletion is via `ThreadRepository::delete`).
    /// Returns `DomainError::NotFound` if the post does not exist.
    async fn delete(&self, id: PostId) -> Result<(), DomainError>;

    /// Delete all posts by a given IP hash within a specific thread.
    ///
    /// Used for the [D*] moderation action. Returns the number of deleted posts.
    /// Returns `Ok(0)` if no posts matched (not an error).
    async fn delete_by_ip_in_thread(
        &self,
        ip_hash: &IpHash,
        thread_id: ThreadId,
    ) -> Result<u64, DomainError>;

    /// Persist attachment metadata records for a post.
    ///
    /// Called immediately after `save()`. The files themselves are already in media
    /// storage — this writes the DB rows so the thread view can retrieve them.
    async fn save_attachments(&self, attachments: &[crate::models::Attachment]) -> Result<(), DomainError>;

    /// Fetch all attachments for a batch of post IDs, returned grouped by post_id.
    ///
    /// Used by the thread handler to bulk-load attachments for a page of posts
    /// without N+1 queries.
    async fn find_attachments_by_post_ids(
        &self,
        post_ids: &[PostId],
    ) -> Result<std::collections::HashMap<PostId, Vec<crate::models::Attachment>>, DomainError>;

    /// Recent posts across all boards for the overboard view, ordered by `created_at DESC`.
    ///
    /// Returns `OverboardPost` entries enriched with `board_slug` so the template
    /// can build links to the parent thread without additional lookups.
    async fn find_overboard(&self, page: Page) -> Result<Paginated<OverboardPost>, DomainError>;

    /// Full-text search for posts on a single board.
    ///
    /// Only invoked when `board_config.search_enabled` is true.
    /// The `query` string is passed to `plainto_tsquery` in the PostgreSQL adapter,
    /// using the existing GIN index on `posts.body`. Results are ordered by relevance
    /// (`ts_rank`) descending.
    ///
    /// Returns an empty `Paginated` when no results match — never `NotFound`.
    async fn search_fulltext(
        &self,
        board_id: crate::models::BoardId,
        query: &str,
        page: Page,
    ) -> Result<Paginated<Post>, DomainError>;

    /// All posts in a thread, ordered by `post_number ASC`, up to 500 rows.
    ///
    /// Used by the thread view which shows every post without pagination (up to the
    /// bump limit). Callers should not assume the list is exhaustive beyond 500 posts.
    async fn find_all_by_thread(&self, thread_id: ThreadId) -> Result<Vec<Post>, DomainError>;

    /// Resolve a board-scoped post number to its containing `ThreadId`.
    ///
    /// Used by the cross-board `>>>/{slug}/{N}` redirect handler. Returns `None`
    /// when no post with that number exists on the board.
    async fn find_thread_id_by_post_number(
        &self,
        board_id: crate::models::BoardId,
        post_number: u64,
    ) -> Result<Option<ThreadId>, DomainError>;
}

/// Persistence boundary for `Ban` records.
///
/// The composition root wires this to `PgBanRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait BanRepository: Send + Sync + 'static {
    /// Returns the active (non-expired) ban for the given IP hash, if any.
    ///
    /// Returns `Ok(None)` if no active ban exists. Never returns `NotFound`.
    async fn find_active_by_ip(&self, ip_hash: &IpHash) -> Result<Option<Ban>, DomainError>;

    /// Insert a new ban record and return the assigned `BanId`.
    async fn save(&self, ban: &Ban) -> Result<BanId, DomainError>;

    /// Mark a ban as expired immediately (sets `expires_at` to `now()`).
    ///
    /// Returns `DomainError::NotFound` if the ban does not exist.
    async fn expire(&self, id: BanId) -> Result<(), DomainError>;

    /// Paginated list of all bans (active and expired) for moderator review.
    async fn find_all(&self, page: Page) -> Result<Paginated<Ban>, DomainError>;
}

/// Persistence boundary for `Flag` (report) records.
///
/// The composition root wires this to `PgFlagRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait FlagRepository: Send + Sync + 'static {
    /// Fetch a flag by its UUID.
    ///
    /// Returns `DomainError::NotFound` if no flag with the given id exists.
    async fn find_by_id(&self, id: FlagId) -> Result<Flag, DomainError>;

    /// Paginated list of pending flags for the moderation queue, ordered by creation time.
    async fn find_pending(&self, page: Page) -> Result<Paginated<Flag>, DomainError>;

    /// Insert a new flag and return the assigned `FlagId`.
    async fn save(&self, flag: &Flag) -> Result<FlagId, DomainError>;

    /// Resolve a flag as either approved or rejected.
    ///
    /// Records the resolving moderator and the resolution outcome.
    /// Returns `DomainError::NotFound` if the flag does not exist.
    async fn resolve(
        &self,
        id: FlagId,
        resolution: FlagResolution,
        resolved_by: UserId,
    ) -> Result<(), DomainError>;
}

/// Write and read boundary for audit log entries.
///
/// The composition root wires this to `PgAuditRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait AuditRepository: Send + Sync + 'static {
    /// Record a moderation action in the audit log.
    ///
    /// This method should never return a hard error to the caller — audit log
    /// write failures are logged and swallowed so they do not interrupt the
    /// primary moderation action.
    async fn record(&self, entry: &AuditEntry) -> Result<(), DomainError>;

    /// Most recent `limit` audit entries site-wide (for the admin dashboard feed).
    async fn find_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, DomainError>;

    /// Paginated audit entries filtered by the acting moderator.
    async fn find_by_actor(&self, actor_id: UserId, page: Page) -> Result<Paginated<AuditEntry>, DomainError>;

    /// Paginated audit entries filtered by the target entity UUID.
    async fn find_by_target(&self, target_id: uuid::Uuid, page: Page) -> Result<Paginated<AuditEntry>, DomainError>;

    /// All audit entries site-wide, newest first. Used by the Janitor audit log page.
    async fn find_all(&self, page: Page) -> Result<Paginated<AuditEntry>, DomainError>;

    /// Audit entries for a specific board (all actions whose `target_type` links to
    /// the given board). Used by Board Owner and Volunteer audit log pages.
    ///
    /// Scoped by board via a JSON detail field lookup — slower than an indexed column
    /// but avoids a schema change. For v1.2 a dedicated `board_id` column is planned.
    async fn find_by_board(&self, board_id: crate::models::BoardId, page: Page) -> Result<Paginated<AuditEntry>, DomainError>;
}

/// Persistence boundary for moderator/admin `User` accounts.
///
/// The composition root wires this to `PgUserRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait UserRepository: Send + Sync + 'static {
    /// Fetch a user by their UUID.
    ///
    /// Returns `DomainError::NotFound` if no user with the given id exists.
    async fn find_by_id(&self, id: UserId) -> Result<User, DomainError>;

    /// Fetch a user by their username (for login).
    ///
    /// Returns `DomainError::NotFound` if no user with the given username exists.
    async fn find_by_username(&self, username: &str) -> Result<User, DomainError>;

    /// Paginated list of all user accounts for the admin user management panel.
    async fn find_all(&self, page: Page) -> Result<Paginated<User>, DomainError>;

    /// Insert or update a user record.
    async fn save(&self, user: &User) -> Result<(), DomainError>;

    /// Deactivate a user account (soft delete — preserves audit log references).
    ///
    /// Returns `DomainError::NotFound` if the user does not exist.
    async fn deactivate(&self, id: UserId) -> Result<(), DomainError>;

    /// Returns the `BoardId`s owned by the given user (from the `board_owners` join table).
    async fn find_owned_boards(&self, user_id: UserId) -> Result<Vec<BoardId>, DomainError>;

    /// Returns the `BoardId`s the given user is assigned as a volunteer (from `board_volunteers`).
    async fn find_volunteer_boards(&self, user_id: UserId) -> Result<Vec<BoardId>, DomainError>;

    /// Assign the given user as an owner of the given board.
    async fn add_board_owner(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError>;

    /// Remove the given user as an owner of the given board.
    async fn remove_board_owner(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError>;

    /// Assign the given user as a volunteer on the given board.
    async fn add_volunteer(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError>;

    /// Remove the given user as a volunteer on the given board.
    async fn remove_volunteer(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError>;
}

// ─── Media Ports ─────────────────────────────────────────────────────────────

/// Raw (unprocessed) media uploaded by a poster.
///
/// Passed into `MediaProcessor::process`. The `mime` field is validated
/// against the board's `allowed_mimes` list before processing begins.
#[derive(Debug, Clone)]
pub struct RawMedia {
    /// The original filename as provided by the uploader.
    pub filename: String,
    /// The MIME type as determined by sniffing (not from the Content-Type header alone).
    pub mime: Mime,
    /// The raw file bytes.
    pub data: Bytes,
}

/// Processed media ready for storage, returned by `MediaProcessor::process`.
#[derive(Debug, Clone)]
pub struct ProcessedMedia {
    /// Deterministic storage key: `{board_slug}/{thread_id}/{post_uuid}.{ext}`.
    pub original_key: MediaKey,
    /// MIME-validated and normalised original bytes.
    pub original_data: Bytes,
    /// Thumbnail storage key. `None` if no thumbnail was generated (e.g. unsupported MIME).
    pub thumbnail_key: Option<MediaKey>,
    /// Compressed thumbnail bytes. `None` if no thumbnail was generated.
    pub thumbnail_data: Option<Bytes>,
    /// SHA-256 of `original_data` bytes, used for duplicate detection.
    pub hash: ContentHash,
    /// Size of the original file in kilobytes.
    pub size_kb: u32,
}

/// Object storage boundary for media files.
///
/// The composition root wires this to `S3MediaStorage` (`media-s3`) or
/// `LocalFsMediaStorage` (`media-local`) based on the active feature flag.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait MediaStorage: Send + Sync + 'static {
    /// Store `data` bytes at `key` with the given `content_type`.
    ///
    /// Returns `DomainError::Internal` if the storage backend returns an error.
    async fn store(
        &self,
        key: &MediaKey,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), DomainError>;

    /// Generate a URL for the object at `key` valid for at least `ttl`.
    ///
    /// For S3, this is a pre-signed URL. For local filesystem, this is a
    /// static public path (the `ttl` argument is ignored).
    async fn get_url(&self, key: &MediaKey, ttl: Duration) -> Result<String, DomainError>;

    /// Delete the object at `key`.
    ///
    /// Returns `Ok(())` even if the object does not exist (idempotent delete).
    async fn delete(&self, key: &MediaKey) -> Result<(), DomainError>;
}

/// Media processing boundary: validate, EXIF-strip, thumbnail-generate.
///
/// The composition root selects the correct processor based on active features:
/// - `ImageMediaProcessor` (always compiled) — images only
/// - `VideoMediaProcessor` (`video` feature) — images + video keyframe extraction
/// - `FullMediaProcessor` (`video` + `documents` features) — images + video + PDFs
///
/// # Invariant
/// EXIF stripping is always performed — it is not a `BoardConfig` toggle.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait MediaProcessor: Send + Sync + 'static {
    /// Process raw uploaded media.
    ///
    /// Steps (unconditional):
    /// 1. Validate MIME type (returns `DomainError::Validation` if unsupported)
    /// 2. Strip EXIF metadata
    /// 3. Generate a thumbnail PNG at 320px width
    /// 4. Compute SHA-256 content hash of the original bytes
    ///
    /// Returns `DomainError::MediaProcessing` if thumbnail generation fails.
    async fn process(&self, input: RawMedia) -> Result<ProcessedMedia, DomainError>;

    /// Returns `true` if this processor can handle the given MIME type.
    ///
    /// Used by `PostService` to decide whether to attempt processing before
    /// calling `process()`.
    fn accepts(&self, mime: &Mime) -> bool;
}

// ─── Auth Port ───────────────────────────────────────────────────────────────

/// Authentication boundary: token lifecycle and password hashing.
///
/// The composition root wires this to `JwtAuthProvider` (`auth-jwt`).
/// Future: `CookieAuthProvider` (v1.1), `OidcAuthProvider` (v2.0).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait AuthProvider: Send + Sync + 'static {
    /// Create a signed token encoding the given claims.
    ///
    /// Returns `DomainError::Auth` on signing failure (should not occur in normal operation).
    async fn create_token(&self, claims: &Claims) -> Result<Token, DomainError>;

    /// Verify and decode a token.
    ///
    /// Returns `DomainError::Auth` if the token is invalid, expired, or tampered with.
    async fn verify_token(&self, token: &Token) -> Result<Claims, DomainError>;

    /// Hash a plaintext password using argon2id.
    ///
    /// Returns `DomainError::Internal` on hashing failure (should not occur in normal operation).
    async fn hash_password(&self, password: &str) -> Result<PasswordHash, DomainError>;

    /// Verify a plaintext password against a stored argon2id hash.
    ///
    /// Returns `DomainError::Auth` if the password does not match the stored hash.
    async fn verify_password(
        &self,
        password: &str,
        hash: &PasswordHash,
    ) -> Result<(), DomainError>;

    /// Revoke a previously issued token, preventing future `verify_token` calls from
    /// succeeding.
    ///
    /// This is a no-op for stateless implementations (`JwtAuthProvider`) where tokens
    /// cannot be invalidated before their expiry. Cookie session implementations
    /// (`CookieAuthProvider`) implement this by deleting the session row.
    ///
    /// Callers should not treat a no-op as an error — the intent (token is no longer
    /// valid) is honoured for stateful providers and best-effort for stateless ones.
    async fn revoke_token(&self, token: &Token) -> Result<(), DomainError> {
        let _ = token;
        Ok(())
    }
}

// ─── Session Repository Port ──────────────────────────────────────────────────

/// A stored session row in the `user_sessions` table.
#[derive(Debug, Clone)]
pub struct Session {
    /// Opaque session ID (stored in the cookie).
    pub session_id:  String,
    /// The user this session belongs to.
    pub user_id:     crate::models::UserId,
    /// Claims encoded at login time. Refreshed on role changes.
    pub claims_json: String,
    /// When this session expires.
    pub expires_at:  chrono::DateTime<chrono::Utc>,
}

/// Persistent backing store for cookie sessions.
///
/// The composition root wires this to `PgSessionRepository` (v1.1+).
/// For single-instance development use `InMemorySessionRepository`.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait SessionRepository: Send + Sync + 'static {
    /// Persist a new session row.
    async fn save(&self, session: &Session) -> Result<(), DomainError>;

    /// Retrieve a session by its opaque ID.
    ///
    /// Returns `DomainError::Auth` if the session does not exist or has expired.
    async fn find_by_id(&self, session_id: &str) -> Result<Session, DomainError>;

    /// Delete a session (logout / revocation).
    ///
    /// Silently succeeds if the session does not exist.
    async fn delete(&self, session_id: &str) -> Result<(), DomainError>;

    /// Delete all sessions for a given user (used on deactivation or forced logout).
    async fn delete_for_user(&self, user_id: crate::models::UserId) -> Result<(), DomainError>;

    /// Delete all sessions whose `expires_at` is in the past.
    ///
    /// Called periodically by a maintenance task. Implementations may be a no-op
    /// if the store handles TTL-based expiry natively (e.g. Redis).
    async fn purge_expired(&self) -> Result<(), DomainError>;
}

// ─── Rate Limiter Port ────────────────────────────────────────────────────────

/// The key used to identify a rate-limited requester.
///
/// Rate limiting is per-IP, per-board. A poster on `/b/` and `/tech/` are
/// tracked independently.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RateLimitKey {
    /// The hashed IP of the requester.
    pub ip_hash: IpHash,
    /// The board the request is for.
    pub board_id: BoardId,
}

/// The result of a rate limit check.
#[derive(Debug, Clone)]
pub enum RateLimitStatus {
    /// The request is within the allowed rate.
    Allowed {
        /// Number of posts remaining in the current window.
        remaining: u32,
    },
    /// The rate limit is exceeded.
    Exceeded {
        /// Number of seconds the caller should wait before retrying.
        retry_after_secs: u32,
    },
}

/// Rate limiting boundary for per-IP, per-board post creation.
///
/// The composition root wires this to `RedisRateLimiter` (`redis`) or
/// `NoopRateLimiter` in dev/test environments.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait RateLimiter: Send + Sync + 'static {
    /// Check the current rate limit status for `key` without incrementing the counter.
    ///
    /// Returns `Allowed` with the remaining count, or `Exceeded` with the retry-after delay.
    async fn check(&self, key: &RateLimitKey) -> Result<RateLimitStatus, DomainError>;

    /// Increment the counter for `key` within the given sliding window.
    ///
    /// Creates the key with TTL `window_secs` if it does not exist.
    async fn increment(&self, key: &RateLimitKey, window_secs: u32) -> Result<(), DomainError>;

    /// Reset the counter for `key` to zero.
    ///
    /// Used in tests and for manual mod actions (e.g. clearing a rate limit after a ban).
    async fn reset(&self, key: &RateLimitKey) -> Result<(), DomainError>;
}

// ─── StaffRequest Repository Port ────────────────────────────────────────────

/// Persistence boundary for staff escalation requests.
///
/// Wired to `PgStaffRequestRepository` in v1.1. The port is defined here so
/// `StaffRequestService` can be written and tested before the adapter exists.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait StaffRequestRepository: Send + Sync + 'static {
    /// Persist a new staff request.
    ///
    /// The caller is responsible for constructing the `StaffRequest` with a
    /// fresh `StaffRequestId` and `status = Pending`.
    async fn save(&self, request: &StaffRequest) -> Result<(), DomainError>;

    /// Retrieve a staff request by its ID.
    ///
    /// Returns `DomainError::NotFound` if no request exists with this ID.
    async fn find_by_id(&self, id: StaffRequestId) -> Result<StaffRequest, DomainError>;

    /// All requests submitted by a given user, ordered by `created_at DESC`.
    async fn find_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<StaffRequest>, DomainError>;

    /// All requests with `status = Pending`, ordered by `created_at ASC`.
    ///
    /// Used to populate the admin and board-owner request queues.
    async fn find_pending(&self) -> Result<Vec<StaffRequest>, DomainError>;

    /// All pending requests of type `BecomeVolunteer` targeting `board_id`.
    ///
    /// Used by board owners to see volunteer requests for their boards.
    async fn find_pending_for_board(
        &self,
        slug: &crate::models::Slug,
    ) -> Result<Vec<StaffRequest>, DomainError>;

    /// Update the status, reviewer, and optional note on an existing request.
    ///
    /// Returns `DomainError::NotFound` if `id` does not exist.
    async fn update_status(
        &self,
        id:          StaffRequestId,
        status:      StaffRequestStatus,
        reviewed_by: UserId,
        note:        Option<String>,
    ) -> Result<(), DomainError>;
}

// ─── StaffMessageRepository ───────────────────────────────────────────────────

/// Persistence boundary for internal staff messages.
///
/// Text-only messages between staff accounts. Admins can message any staff;
/// board owners can message their volunteers. Messages expire after 14 days.
///
/// The composition root wires this to `PgStaffMessageRepository` (feature: `db-postgres`).
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait StaffMessageRepository: Send + Sync + 'static {
    /// Fetch all messages addressed to `user_id`, newest first.
    async fn find_for_user(
        &self,
        user_id: UserId,
        page: Page,
    ) -> Result<Paginated<crate::models::StaffMessage>, DomainError>;

    /// Count unread messages for `user_id`. Used to populate the nav badge.
    async fn count_unread(&self, user_id: UserId) -> Result<u32, DomainError>;

    /// Persist a new message. Returns the assigned `StaffMessageId`.
    async fn save(
        &self,
        message: &crate::models::StaffMessage,
    ) -> Result<crate::models::StaffMessageId, DomainError>;

    /// Mark a message as read by setting `read_at = now()`.
    ///
    /// Silently succeeds if the message was already read.
    async fn mark_read(
        &self,
        id: crate::models::StaffMessageId,
    ) -> Result<(), DomainError>;

    /// Delete all messages older than `older_than_days` days.
    ///
    /// Returns the number of messages deleted. Called by a periodic maintenance task
    /// or on-demand from an admin endpoint. Use `14` for the standard expiry window.
    async fn delete_expired(&self, older_than_days: u32) -> Result<u32, DomainError>;
}
