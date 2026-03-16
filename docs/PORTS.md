# PORTS.md
# rusty-board — Port Trait Registry

> **Authoritative reference for all port traits. Every external boundary must appear here before any adapter is written. When considering a new external dependency, the first question is: which port does it implement?**

Adding a concrete dependency to any crate without first defining a port and adding it to this document is an architectural violation.

---

## What Is a Port?

A port is an `async` trait defined in `domains/ports.rs` that represents a capability the system needs from the outside world. Port traits:

- Live in `domains` — the innermost, always-compiled crate
- Carry `Send + Sync + 'static` bounds (required for use across async boundaries)
- Use native RPITIT async — no `#[async_trait]` macro
- Return `Result<T, DomainError>` — never adapter-specific error types
- Are never implemented in `domains` or `services` — only in adapter crates
- Are never called directly with concrete types inside service code

---

## Adding a New Port — Required Steps

1. Define the trait in `domains/ports.rs` with full signatures and `///` doc comments on every method
2. Add an entry to this document with purpose, users, adapters, and feature flag
3. Add a `Noop<PortName>` test stub in `tests/fixtures/`
4. Add `#[automock]` attribute for `mockall` mock generation
5. Add the port as a generic bound on any service that uses it
6. Implement the concrete adapter in the appropriate adapter crate
7. Add the `#[cfg(feature = "...")]` wiring branch in `composition.rs`
8. Add adapter contract tests in `tests/adapter_contracts/`

Steps 6–8 must not begin before steps 1–5 are complete and reviewed.

---

## Active Ports (v1.0)

---

### `BoardRepository`

**Purpose**: CRUD and pagination for `Board` entities. Also manages `BoardConfig` persistence.

**Used by**: `BoardService`

**v1.0 adapter**: `PgBoardRepository` (`storage-adapters/src/postgres/repositories/board_repository.rs`)

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqliteBoardRepository` (`db-sqlite`, v1.2), `SurrealBoardRepository` (`db-surrealdb`, v2.0)

```rust
pub trait BoardRepository: Send + Sync + 'static {
    /// Fetch a board by its UUID. Returns `DomainError::NotFound` if absent.
    async fn find_by_id(&self, id: BoardId) -> Result<Board, DomainError>;

    /// Fetch a board by its slug. Returns `DomainError::NotFound` if absent.
    async fn find_by_slug(&self, slug: &Slug) -> Result<Board, DomainError>;

    /// Paginated list of all boards ordered by creation date.
    async fn find_all(&self, page: Page) -> Result<Paginated<Board>, DomainError>;

    /// Insert (if new) or update (if existing) a board record.
    async fn save(&self, board: &Board) -> Result<(), DomainError>;

    /// Delete a board and cascade to all child records.
    async fn delete(&self, id: BoardId) -> Result<(), DomainError>;

    /// Fetch the BoardConfig for a board. Returns `DomainError::NotFound` if absent.
    async fn find_config(&self, board_id: BoardId) -> Result<BoardConfig, DomainError>;

    /// Persist an updated BoardConfig for a board.
    async fn save_config(&self, board_id: BoardId, config: &BoardConfig) -> Result<(), DomainError>;
}
```

---

### `ThreadRepository`

**Purpose**: CRUD, bump, sticky/close, and prune operations for `Thread` entities.

**Used by**: `ThreadService`, `PostService`

**v1.0 adapter**: `PgThreadRepository`

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqliteThreadRepository` (`db-sqlite`, v1.2)

```rust
pub trait ThreadRepository: Send + Sync + 'static {
    /// Fetch a thread by UUID. Returns `DomainError::NotFound` if absent.
    async fn find_by_id(&self, id: ThreadId) -> Result<Thread, DomainError>;

    /// Paginated thread list for a board, ordered by bumped_at DESC.
    /// Sticky threads appear first regardless of bump time.
    async fn find_by_board(&self, board_id: BoardId, page: Page) -> Result<Paginated<Thread>, DomainError>;

    /// All threads for catalog view (no pagination; returns only OP post summary).
    /// Catalog view — all threads for a board ordered by `sticky DESC, bumped_at DESC`.
    ///
    /// Returns `ThreadSummary` rows enriched with OP post header fields
    /// (`op_name`, `op_tripcode`, `op_created_at`, `op_post_number`, `op_ip_hash`)
    /// so the board index and catalog templates can render a full post header
    /// without an additional per-thread query. Added v1.1-ux.
    async fn find_catalog(&self, board_id: BoardId) -> Result<Vec<ThreadSummary>, DomainError>;

    /// Set cycle mode on a thread (v1.2). `true` = prune oldest unpinned reply on new post past bump limit.
    async fn set_cycle(&self, id: ThreadId, cycle: bool) -> Result<(), DomainError>;

    /// Insert a new thread. Returns the assigned ThreadId.
    async fn save(&self, thread: &Thread) -> Result<ThreadId, DomainError>;

    /// Update bumped_at and increment reply_count atomically.
    async fn bump(&self, id: ThreadId, bumped_at: DateTime<Utc>) -> Result<(), DomainError>;

    /// Set op_post_id after the OP post is created.
    async fn set_op_post(&self, id: ThreadId, op_post_id: PostId) -> Result<(), DomainError>;

    /// Toggle the sticky flag.
    async fn set_sticky(&self, id: ThreadId, sticky: bool) -> Result<(), DomainError>;

    /// Toggle the closed flag.
    async fn set_closed(&self, id: ThreadId, closed: bool) -> Result<(), DomainError>;

    /// Count threads on a board (for prune threshold check).
    async fn count_by_board(&self, board_id: BoardId) -> Result<u32, DomainError>;

    /// Delete the oldest (by bumped_at) non-sticky threads until count <= keep.
    /// Returns the number of threads deleted.
    async fn prune_oldest(&self, board_id: BoardId, keep: u32) -> Result<u32, DomainError>;

    /// Delete a thread and all its posts (cascade).
    async fn delete(&self, id: ThreadId) -> Result<(), DomainError>;
}
```

---

### `PostRepository`

**Purpose**: CRUD for `Post` entities. Supports IP hash lookups (ban enforcement), recent content hash lookups (spam duplicate detection), and full-text search.

**Used by**: `PostService`, `ModerationService`, search handler

**v1.0 adapter**: `PgPostRepository`

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqlitePostRepository` (`db-sqlite`, v1.2)

```rust
pub trait PostRepository: Send + Sync + 'static {
    /// Fetch a post by UUID. Returns `DomainError::NotFound` if absent.
    async fn find_by_id(&self, id: PostId) -> Result<Post, DomainError>;

    /// Paginated posts in a thread, ordered by timestamp ASC.
    async fn find_by_thread(&self, thread_id: ThreadId, page: Page) -> Result<Paginated<Post>, DomainError>;

    /// All posts by a given IP hash across all boards. Used in moderation.
    async fn find_by_ip_hash(&self, ip_hash: &IpHash) -> Result<Vec<Post>, DomainError>;

    /// ContentHash values of the most recent `limit` posts on a board.
    /// Used for duplicate content detection in spam heuristics.
    async fn find_recent_hashes(&self, board_id: BoardId, limit: u32) -> Result<Vec<ContentHash>, DomainError>;

    /// Insert a new post. Returns the assigned PostId and board-scoped post number.
    async fn save(&self, post: &Post) -> Result<(PostId, u64), DomainError>;

    /// Delete a single post. Does not cascade to thread (thread deletion is via ThreadRepository).
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
    async fn save_attachments(&self, attachments: &[Attachment]) -> Result<(), DomainError>;

    /// Fetch all attachments for a batch of post IDs, returned grouped by post_id.
    async fn find_attachments_by_post_ids(
        &self,
        post_ids: &[PostId],
    ) -> Result<HashMap<PostId, Vec<Attachment>>, DomainError>;

    /// Recent posts across all boards for the overboard view.
    async fn find_overboard(&self, page: Page) -> Result<Paginated<OverboardPost>, DomainError>;

    /// Full-text search for posts on a single board. Added v1.1.
    ///
    /// Only called when `board_config.search_enabled` is true.
    /// `query` is passed to `plainto_tsquery` — safe against injection. Results ordered
    /// by `ts_rank` descending. Returns an empty page when no results match.
    async fn search_fulltext(
        &self,
        board_id: BoardId,
        query: &str,
        page: Page,
    ) -> Result<Paginated<Post>, DomainError>;

    /// All posts in a thread ordered by post_number ASC, up to 500 rows. Added v1.2-ux.
    ///
    /// Used by the thread view which shows every post without pagination.
    /// The 500-row cap matches the maximum bump limit — threads never exceed this size.
    async fn find_all_by_thread(&self, thread_id: ThreadId) -> Result<Vec<Post>, DomainError>;

    /// Resolve a board-scoped post number to the ThreadId that contains it. Added v1.2-ux.
    ///
    /// Used by the `GET /board/{slug}/post/{N}` redirect handler which resolves
    /// cross-board `>>>/{slug}/{N}` links. Returns `None` when no post with that
    /// number exists on the board.
    async fn find_thread_id_by_post_number(
        &self,
        board_id: BoardId,
        post_number: u64,
    ) -> Result<Option<ThreadId>, DomainError>;

    /// Set the pinned flag on a post (v1.2). Pinned posts are excluded from cycle pruning.
    async fn set_pinned(&self, id: PostId, pinned: bool) -> Result<(), DomainError>;

    /// Return the oldest non-OP non-pinned reply ID in a thread. Used by cycle pruning.
    async fn find_oldest_unpinned_reply(&self, thread_id: ThreadId) -> Result<Option<PostId>, DomainError>;

    /// SHA-256 deduplication lookup (v1.2). Reuse existing keys for identical files.
    async fn find_attachment_by_hash(&self, hash: &ContentHash) -> Result<Option<Attachment>, DomainError>;

    /// Delete a single post by ID (used by cycle pruning; does not cascade).
    async fn delete_by_id(&self, id: PostId) -> Result<(), DomainError>;
}
```

---

### `BanRepository`

**Purpose**: Ban issuance, lookup, and expiry. Checked on every post creation.

**Used by**: `PostService` (active ban check), `ModerationService` (issue bans)

**v1.0 adapter**: `PgBanRepository`

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqliteBanRepository` (`db-sqlite`, v1.2)

```rust
pub trait BanRepository: Send + Sync + 'static {
    /// Returns the active (non-expired) ban for this IP hash, if any.
    /// Returns `Ok(None)` if no active ban exists.
    async fn find_active_by_ip(&self, ip_hash: &IpHash) -> Result<Option<Ban>, DomainError>;

    /// Insert a new ban record. Returns the assigned BanId.
    async fn save(&self, ban: &Ban) -> Result<BanId, DomainError>;

    /// Mark a ban as expired (immediate effect; sets expires_at to now()).
    async fn expire(&self, id: BanId) -> Result<(), DomainError>;

    /// Paginated list of all bans (active and expired) for moderator review.
    async fn find_all(&self, page: Page) -> Result<Paginated<Ban>, DomainError>;
}
```

---

### `FlagRepository`

**Purpose**: Report/flag submission and moderation queue management.

**Used by**: `ModerationService`

**v1.0 adapter**: `PgFlagRepository`

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqliteFlagRepository` (`db-sqlite`, v1.2)

```rust
pub trait FlagRepository: Send + Sync + 'static {
    /// Fetch a flag by UUID. Returns `DomainError::NotFound` if absent.
    async fn find_by_id(&self, id: FlagId) -> Result<Flag, DomainError>;

    /// Paginated list of pending flags for the moderation queue.
    async fn find_pending(&self, page: Page) -> Result<Paginated<Flag>, DomainError>;

    /// Insert a new flag. Returns the assigned FlagId.
    async fn save(&self, flag: &Flag) -> Result<FlagId, DomainError>;

    /// Resolve a flag (approve or reject). Records resolver and timestamp.
    async fn resolve(
        &self,
        id: FlagId,
        resolution: FlagResolution,
        resolved_by: UserId,
    ) -> Result<(), DomainError>;
}
```

---

### `AuditRepository`

**Purpose**: Write and read audit log entries for all moderation actions.

**Used by**: `ModerationService`

**v1.0 adapter**: `PgAuditRepository`

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqliteAuditRepository` (`db-sqlite`, v1.2)

```rust
pub trait AuditRepository: Send + Sync + 'static {
    /// Record a moderation action. Should never fail; log and swallow on error.
    async fn record(&self, entry: &AuditEntry) -> Result<(), DomainError>;

    /// Most recent audit entries site-wide (for admin dashboard).
    async fn find_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, DomainError>;

    /// Paginated audit entries filtered by the actor (moderator who acted).
    async fn find_by_actor(&self, actor_id: UserId, page: Page) -> Result<Paginated<AuditEntry>, DomainError>;

    /// Paginated audit entries filtered by the target entity (post, thread, user).
    async fn find_by_target(&self, target_id: Uuid, page: Page) -> Result<Paginated<AuditEntry>, DomainError>;
}
```

---

### `UserRepository`

**Purpose**: Persistence for moderator/admin user accounts. Supports authentication lookup and board ownership retrieval.

**Used by**: `UserService`, `ModerationService` (actor lookup for audit), auth middleware (owned board IDs)

**v1.0 adapter**: `PgUserRepository`

**Feature flag**: `db-postgres`

**Planned future adapters**: `SqliteUserRepository` (`db-sqlite`, v1.2)

```rust
pub trait UserRepository: Send + Sync + 'static {
    /// Fetch a user by UUID. Returns `DomainError::NotFound` if absent.
    async fn find_by_id(&self, id: UserId) -> Result<User, DomainError>;

    /// Fetch a user by username for login. Returns `DomainError::NotFound` if absent.
    async fn find_by_username(&self, username: &str) -> Result<User, DomainError>;

    /// Paginated list of all user accounts (for admin user management).
    async fn find_all(&self, page: Page) -> Result<Paginated<User>, DomainError>;

    /// Insert or update a user record.
    async fn save(&self, user: &User) -> Result<(), DomainError>;

    /// Deactivate a user account (soft delete — preserves audit log references).
    async fn deactivate(&self, id: UserId) -> Result<(), DomainError>;

    /// Returns the board IDs this user owns (from board_owners join table).
    async fn find_owned_boards(&self, user_id: UserId) -> Result<Vec<BoardId>, DomainError>;

    /// Assign a user as owner of a board.
    async fn add_board_owner(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError>;

    /// Remove a user as owner of a board.
    async fn remove_board_owner(&self, board_id: BoardId, user_id: UserId) -> Result<(), DomainError>;
}
```

---

### `MediaStorage`

**Purpose**: Store, retrieve, and delete media files. Generate access URLs.

**Used by**: `PostService` (via composition — PostService calls MediaProcessor, then stores the output)

**v1.0 adapters**:
- `S3MediaStorage` (`storage-adapters/src/media/s3.rs`, feature: `media-s3`)
- `LocalFsMediaStorage` (`storage-adapters/src/media/local_fs.rs`, feature: `media-local`)

**Planned future adapters**: `R2MediaStorage` (`media-r2`, v1.2), `BackblazeMediaStorage` (`media-backblaze`, v1.2), `IpfsMediaStorage` (`media-ipfs`, v2.0)

```rust
pub trait MediaStorage: Send + Sync + 'static {
    /// Store bytes at the given key with the specified content type.
    async fn store(
        &self,
        key: &MediaKey,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), DomainError>;

    /// Generate an access URL for the given key valid for the specified TTL.
    /// S3: generates a presigned URL expiring after `ttl`.
    /// Local filesystem: returns a static public path (TTL is ignored).
    async fn get_url(&self, key: &MediaKey, ttl: Duration) -> Result<String, DomainError>;

    /// Delete the stored object at the given key.
    async fn delete(&self, key: &MediaKey) -> Result<(), DomainError>;
}
```

---

### `MediaProcessor`

**Purpose**: Validate MIME type, strip EXIF metadata, generate thumbnails. Returns normalized media ready for storage.

**Used by**: `PostService`

**v1.0 adapters** (in `storage-adapters/src/media/`):
- `ImageMediaProcessor` — images only, always compiled
- `VideoMediaProcessor` — images + video (`video` feature)
- `FullMediaProcessor` — images + video + documents (`video` + `documents` features)

The composition root selects the correct concrete type based on active features. The service sees only the `MediaProcessor` trait.

```rust
pub trait MediaProcessor: Send + Sync + 'static {
    /// Process raw uploaded media:
    /// 1. Validate MIME type
    /// 2. Strip EXIF (images always; included in video keyframe extraction)
    /// 3. Generate thumbnail PNG at 320px width
    /// 4. Compute SHA-256 content hash of original bytes
    ///
    /// Returns `DomainError::Validation` if MIME is not supported by this processor.
    /// Returns `DomainError::MediaProcessing` if thumbnail generation fails.
    async fn process(&self, input: RawMedia) -> Result<ProcessedMedia, DomainError>;

    /// Returns true if this processor can handle the given MIME type.
    fn accepts(&self, mime: &Mime) -> bool;
}

pub struct RawMedia {
    pub filename: String,
    pub mime:     Mime,
    pub data:     Bytes,
}

pub struct ProcessedMedia {
    pub original_key:    MediaKey,     // deterministic key: {board}/{thread}/{post}/{uuid}.{ext}
    pub original_data:   Bytes,        // MIME-validated original bytes
    pub thumbnail_key:   Option<MediaKey>,  // None if MIME type has no thumbnail support
    pub thumbnail_data:  Option<Bytes>,
    pub hash:            ContentHash,  // SHA-256 of original_data
    pub size_kb:         u32,
}
```

---

### `AuthProvider`

**Purpose**: Token lifecycle management and password hashing. The single external boundary for authentication operations.

**Used by**: `UserService` (login: hash verify + token create), auth middleware (token verify), logout handler (revoke)

**v1.0 adapter**: `JwtAuthProvider` (`auth-adapters/src/jwt_bearer/mod.rs`, feature: `auth-jwt`)

**v1.1 adapter**: `CookieAuthProvider` (`auth-adapters/src/cookie_session/mod.rs`, feature: `auth-cookie`)

**Planned future adapters**: `OidcAuthProvider` (`auth-oidc`, v2.0)

**Port swap validated**: v1.1 — `CookieAuthProvider<InMemorySessionRepository>` passes all 12 unit tests including roundtrip, revocation, expiry, CSRF, and password hashing.

```rust
pub trait AuthProvider: Send + Sync + 'static {
    /// Create a signed token encoding the given claims.
    /// Returns `DomainError::Auth` on signing failure.
    async fn create_token(&self, claims: &Claims) -> Result<Token, DomainError>;

    /// Verify and decode a token. Returns `DomainError::Auth` if invalid or expired.
    async fn verify_token(&self, token: &Token) -> Result<Claims, DomainError>;

    /// Hash a plaintext password using argon2id.
    /// Returns `DomainError::Internal` on hashing failure (should not occur).
    async fn hash_password(&self, password: &str) -> Result<PasswordHash, DomainError>;

    /// Verify a plaintext password against a stored hash.
    /// Returns `DomainError::Auth` if the password does not match.
    async fn verify_password(
        &self,
        password: &str,
        hash: &PasswordHash,
    ) -> Result<(), DomainError>;

    /// Revoke a previously issued token. Added v1.1.
    ///
    /// **Default impl**: no-op. Stateless providers (`JwtAuthProvider`) cannot revoke
    /// before expiry — they inherit this no-op. Stateful providers (`CookieAuthProvider`)
    /// override this to delete the session row, giving immediate revocation.
    ///
    /// Callers must not treat a no-op as an error.
    async fn revoke_token(&self, token: &Token) -> Result<(), DomainError> {
        let _ = token;
        Ok(())
    }
}
```

---

### `RateLimiter`

**Purpose**: Per-IP, per-board post rate limiting. Checked inside `PostService` when `board_config.rate_limit_enabled` is true.

**Used by**: `PostService`

**v1.0 adapter**: `RedisRateLimiter` (`storage-adapters/src/redis/mod.rs`, feature: `redis`)

**Test / dev adapter**: `NoopRateLimiter` — always returns `Allowed`. Provided in `tests/fixtures/`. Also usable in single-instance deployments without Redis.

**v1.1 adapter**: `InMemoryRateLimiter` (`storage-adapters/src/in_memory/rate_limiter.rs`) — DashMap sliding window, no Redis, single-instance only. Ships with `--no-redis` scenario support.

```rust
pub trait RateLimiter: Send + Sync + 'static {
    /// Check current rate limit status for this key without incrementing.
    /// Returns `Allowed` with remaining count, or `Exceeded` with retry-after.
    async fn check(&self, key: &RateLimitKey) -> Result<RateLimitStatus, DomainError>;

    /// Increment the counter for this key within the given window.
    /// Creates the key if it does not exist.
    async fn increment(&self, key: &RateLimitKey, window_secs: u32) -> Result<(), DomainError>;

    /// Reset the counter for this key (used in tests and manual mod actions).
    async fn reset(&self, key: &RateLimitKey) -> Result<(), DomainError>;
}

pub struct RateLimitKey {
    pub ip_hash:  IpHash,
    pub board_id: BoardId,
}

pub enum RateLimitStatus {
    Allowed  { remaining: u32 },
    Exceeded { retry_after_secs: u32 },
}
```

---

## Active Ports (v1.1)

---

### `SessionRepository` (v1.1) ✅

**Purpose**: Persistent backing store for server-side cookie sessions. Used exclusively by `CookieAuthProvider` to create, validate, and revoke sessions.

**Context**: `JwtAuthProvider` does not use this port — JWT is stateless. The port is only wired when `auth-cookie` feature is active.

**Used by**: `CookieAuthProvider<SR: SessionRepository>`

**v1.1 adapter (dev/test)**: `InMemorySessionRepository` (`storage-adapters/src/in_memory/session_repository.rs`) — DashMap-backed, no DB, single-instance.

**v1.1 adapter (production)**: `PgSessionRepository` (`storage-adapters/src/postgres/repositories/session_repository.rs`) — stores sessions in `user_sessions` (migration 016). Wired in `composition.rs` under `auth-cookie` feature.

**Planned future adapters**: `SqliteSessionRepository` (`db-sqlite`, v1.2), `RedisSessionRepository` (v1.2 option for multi-instance).

```rust
pub struct Session {
    /// Opaque session ID stored in the cookie.
    pub session_id:  String,
    /// The user this session belongs to.
    pub user_id:     UserId,
    /// Serialised `Claims` struct. Cached to avoid a user table join per request.
    pub claims_json: String,
    /// When this session expires.
    pub expires_at:  DateTime<Utc>,
}

pub trait SessionRepository: Send + Sync + 'static {
    /// Persist a new session row.
    async fn save(&self, session: &Session) -> Result<(), DomainError>;

    /// Retrieve a session by its opaque ID.
    /// Returns `DomainError::Auth` if absent or expired.
    async fn find_by_id(&self, session_id: &str) -> Result<Session, DomainError>;

    /// Delete a session (logout / revocation). Silently succeeds if absent.
    async fn delete(&self, session_id: &str) -> Result<(), DomainError>;

    /// Delete all sessions for a given user (deactivation or forced logout).
    async fn delete_for_user(&self, user_id: UserId) -> Result<(), DomainError>;

    /// Delete all expired sessions. Called by a periodic maintenance task.
    /// May be a no-op on stores with native TTL expiry (e.g. Redis).
    async fn purge_expired(&self) -> Result<(), DomainError>;
}
```

---

## Planned Ports (Not Yet in `domains/ports.rs`)

These ports are documented here so the interface design happens before implementation pressure. Fields in `BoardConfig` that reference these capabilities already exist. When the version milestone is reached, the port definition in this document becomes the starting point for `domains/ports.rs`.

---

### `StaffRequestRepository` (v1.1) ✅

**Purpose**: Persist and query staff promotion requests submitted by `Role::User` accounts. Covers the full lifecycle: submit, list pending, approve, deny.

**Status**: Active — in `domains/ports.rs`, implemented in `StaffRequestService`.

**Used by**: `StaffRequestService`

**v1.1 adapter**: `PgStaffRequestRepository` (`storage-adapters/src/postgres/repositories/staff_request_repository.rs`) — wired in composition.rs, replaces `NoopStaffRequestRepository`.

**Planned future adapters**: `PgStaffRequestRepository` (`db-postgres`), `SqliteStaffRequestRepository` (`db-sqlite`, v1.2)

```rust
pub trait StaffRequestRepository: Send + Sync + 'static {
    /// Insert a new request. Returns the assigned StaffRequestId.
    async fn save(&self, request: &StaffRequest) -> Result<StaffRequestId, DomainError>;

    /// All pending requests, newest first (for admin queue).
    async fn find_pending(&self, page: Page) -> Result<Paginated<StaffRequest>, DomainError>;

    /// Pending requests targeting a specific board (for board owner queue).
    async fn find_pending_for_board(&self, slug: &Slug, page: Page) -> Result<Paginated<StaffRequest>, DomainError>;

    /// All requests submitted by a user (for User dashboard history).
    async fn find_by_user(&self, user_id: UserId, page: Page) -> Result<Paginated<StaffRequest>, DomainError>;

    /// Fetch a single request by ID.
    async fn find_by_id(&self, id: StaffRequestId) -> Result<StaffRequest, DomainError>;

    /// Mark a request approved or denied, recording the reviewer and optional note.
    async fn resolve(
        &self,
        id: StaffRequestId,
        status: StaffRequestStatus,
        reviewed_by: UserId,
        note: Option<String>,
    ) -> Result<(), DomainError>;
}
```

---

### `StaffMessageRepository` (v1.1) ✅

**Status**: Active — port defined in `domains/ports.rs`, `PgStaffMessageRepository` implemented. `StaffMessageService` pending.

**Purpose**: Internal staff-only text messaging. Admins can message any staff account; board owners can message their volunteers. Messages expire after 14 days. Primary use case: announcements.

**Context**: New `staff_messages` table added in v1.1 migration. No attachment support — body text only.

**Planned adapters**: `PgStaffMessageRepository` (`db-postgres`), `SqliteStaffMessageRepository` (`db-sqlite`, v1.2)

```rust
pub trait StaffMessageRepository: Send + Sync + 'static {
    /// Fetch all messages addressed to a user, newest first.
    async fn find_for_user(&self, user_id: UserId, page: Page) -> Result<Paginated<StaffMessage>, DomainError>;

    /// Fetch unread count for a user (for nav badge).
    async fn count_unread(&self, user_id: UserId) -> Result<u32, DomainError>;

    /// Insert a new message. Returns the assigned message ID.
    async fn save(&self, message: &StaffMessage) -> Result<StaffMessageId, DomainError>;

    /// Mark a message as read. Sets read_at to now().
    async fn mark_read(&self, id: StaffMessageId) -> Result<(), DomainError>;

    /// Delete messages older than 14 days (run as a periodic task).
    async fn delete_expired(&self) -> Result<u32, DomainError>;
}
```

---

### `CaptchaVerifier` (v1.1)

**Purpose**: Verify CAPTCHA challenge tokens submitted with posts.

**Context**: `board_config.captcha_required` already exists. When true, `PostService::create_post` will call this port before accepting a post.

**Planned adapters**: `HCaptchaCaptchaVerifier` (`spam-hcaptcha`), `ReCaptchaCaptchaVerifier` (`spam-recaptcha`)

```rust
// Interface draft for v1.1 — not yet in domains/ports.rs
pub trait CaptchaVerifier: Send + Sync + 'static {
    /// Verify the CAPTCHA response token for the given user IP.
    /// Returns `DomainError::Validation` if verification fails.
    async fn verify(&self, token: &str, ip_hash: &IpHash) -> Result<(), DomainError>;
}
```

---

### `SearchIndex` (v1.2)

**Purpose**: Index posts for full-text search and execute queries.

**Context**: `board_config.search_enabled` already exists. When true, `PostService` will index new posts, and a new search endpoint will query the index.

**Planned adapters**: `MeiliSearchIndex` (`search-meilisearch`), `PgFullTextIndex` (`search-postgres-fts`)

```rust
// Interface draft for v1.2 — not yet in domains/ports.rs (v1.1 ships basic FTS via PostRepository::search_fulltext)
pub trait SearchIndex: Send + Sync + 'static {
    /// Index a post for search. Called after successful post insertion.
    async fn index_post(&self, post: &Post) -> Result<(), DomainError>;

    /// Search posts matching the query on a specific board.
    async fn search(
        &self,
        query: &str,
        board_id: BoardId,
        page: Page,
    ) -> Result<Paginated<PostId>, DomainError>;

    /// Remove a post from the search index (on deletion).
    async fn delete_post(&self, id: PostId) -> Result<(), DomainError>;
}
```

---

### `DnsblChecker` (v1.2)

**Purpose**: Check whether a posting IP appears on a DNS blocklist.

**Context**: Will be a new `BoardConfig` field `dnsbl_enabled` added in v1.2. When true, `PostService` will check the DNSBL before accepting a post.

**Planned adapters**: `SpamhausDnsblChecker`, `BarracudaDnsblChecker`

```rust
// Interface draft for v1.2 — not yet in domains/ports.rs
pub trait DnsblChecker: Send + Sync + 'static {
    /// Returns true if the IP is listed on a blocklist.
    /// Returns false on lookup failure (fail open to avoid blocking legitimate users).
    async fn is_listed(&self, ip: &IpHash) -> Result<bool, DomainError>;
}
```

---

### `FederationSync` (v2.0)

**Purpose**: Publish posts/threads to remote instances via ActivityPub and receive incoming activities.

**Context**: `board_config.federation_enabled` already exists.

**Planned adapters**: `ActivityPubFederationSync` (`federation-activitypub`)

```rust
// Interface draft for v2.0 — not yet in domains/ports.rs
pub trait FederationSync: Send + Sync + 'static {
    /// Publish a new post as an ActivityPub Create activity.
    async fn publish_post(&self, post: &Post, board: &Board) -> Result<(), DomainError>;

    /// Process an incoming ActivityPub activity payload.
    async fn receive_activity(&self, payload: &[u8]) -> Result<(), DomainError>;
}
```

---

## Port × Adapter Matrix

✅ = shipped and tested | 🚧 = in progress | *(planned)* = not yet started

| Port | v1.0 Adapter | v1.1 | v1.2 | v2.0 |
|------|-------------|------|------|------|
| `BoardRepository` | `PgBoardRepository` ✅ | — | `SqliteBoardRepository` | `SurrealBoardRepository` |
| `ThreadRepository` | `PgThreadRepository` ✅ | — | `SqliteThreadRepository` | — |
| `PostRepository` | `PgPostRepository` ✅ | `search_fulltext` ✅, `find_all_by_thread` ✅, `find_thread_id_by_post_number` ✅ | `SqlitePostRepository` | — |
| `BanRepository` | `PgBanRepository` ✅ | — | `SqliteBanRepository` | — |
| `FlagRepository` | `PgFlagRepository` ✅ | — | `SqliteFlagRepository` | — |
| `AuditRepository` | `PgAuditRepository` ✅ | audit log pages ✅ (`find_all`, `find_by_board` added) | `SqliteAuditRepository` | — |
| `UserRepository` | `PgUserRepository` ✅ | — | `SqliteUserRepository` | — |
| `MediaStorage` | `S3MediaStorage` ✅, `LocalFsMediaStorage` ✅ | — | `R2MediaStorage`, `BackblazeMediaStorage` | `IpfsMediaStorage` |
| `MediaProcessor` | `ImageMediaProcessor` ✅ (+ `Video`, + `Full`) | — | — | — |
| `AuthProvider` | `JwtAuthProvider` ✅ | `CookieAuthProvider` ✅ | — | `OidcAuthProvider` |
| `RateLimiter` | `RedisRateLimiter` ✅, `NoopRateLimiter` ✅ | `InMemoryRateLimiter` ✅ | — | — |
| `SessionRepository` | — | `InMemorySessionRepository` ✅, `PgSessionRepository` ✅ | `SqliteSessionRepository` | — |
| `StaffRequestRepository` | — | `PgStaffRequestRepository` ✅ | `SqliteStaffRequestRepository` | — |
| `CaptchaVerifier` | — | `HCaptchaCaptchaVerifier` *(planned)*, `ReCaptchaCaptchaVerifier` *(planned)* | — | — |
| `StaffMessageRepository` | — | `PgStaffMessageRepository` ✅, `StaffMessageService` ✅ | `SqliteStaffMessageRepository` | — |
| `SearchIndex` | — | `PostRepository::search_fulltext` ✅ (basic, not a port) | `MeiliSearchIndex`, `PgFullTextIndex` | — |
| `DnsblChecker` | — | — | `SpamhausDnsblChecker` | — |
| `FederationSync` | — | — | — | `ActivityPubFederationSync` |

---

## `ArchiveRepository` (v1.2)

**Purpose**: Persist threads that are pruned when a board exceeds `max_threads`, so they remain accessible instead of being deleted.

**Gated by**: `board_config.archive_enabled`. When false, `NoopArchiveRepository` is used and threads are hard-deleted as before.

```rust
pub trait ArchiveRepository: Send + Sync + 'static {
    async fn archive_thread(&self, thread: &Thread) -> Result<(), DomainError>;
    async fn find_archived(&self, board_id: BoardId, page: Page) -> Result<Paginated<Thread>, DomainError>;
}
```

**v1.2 adapter**: `PgArchiveRepository` — inserts into `archived_threads` table (migration 015) with `ON CONFLICT DO NOTHING` for idempotency.
**No-op adapter**: `NoopArchiveRepository` — `archive_thread` silently succeeds; `find_archived` returns empty page.
