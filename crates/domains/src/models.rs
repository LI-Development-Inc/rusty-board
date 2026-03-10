//! Domain models, value objects, and `BoardConfig`.
//!
//! This module is the authoritative definition of every entity in the system.
//! All types here are pure data — no I/O, no framework dependencies.
//!
//! # Organisation
//! - **Value objects**: newtype wrappers providing type safety for IDs, slugs, etc.
//! - **Domain models**: the primary entities (`Board`, `Thread`, `Post`, etc.)
//! - **BoardConfig**: the runtime behaviour surface — the only path from dashboard UI
//!   to service logic changes.
//! - **Supporting enums**: `Role`, `FlagResolution`
//!
//! # Serde
//! All types that cross the API or storage boundary derive `Serialize`/`Deserialize`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::errors::ValidationError;

// ─── Slug validation constant ────────────────────────────────────────────────

// INVARIANT: slug must match this pattern everywhere — in value object validation,
// in the DB CHECK constraint, and in URL routing. Change here if the rule changes.
#[allow(dead_code)]
const SLUG_PATTERN: &str = "^[a-z0-9_-]{1,16}$";

// ─── Value Objects ───────────────────────────────────────────────────────────

/// A board's unique identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoardId(pub Uuid);

impl BoardId {
    /// Create a new random `BoardId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BoardId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BoardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A thread's unique identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub Uuid);

impl ThreadId {
    /// Create a new random `ThreadId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A post's unique identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostId(pub Uuid);

impl PostId {
    /// Create a new random `PostId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PostId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PostId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A user's unique identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
    /// Create a new random `UserId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A ban's unique identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BanId(pub Uuid);

impl BanId {
    /// Create a new random `BanId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A flag (report) unique identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagId(pub Uuid);

impl FlagId {
    /// Create a new random `FlagId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FlagId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for FlagId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// SHA-256 hash of `raw_ip + daily_salt`. Raw IP addresses are never stored.
///
/// The salt rotates on restart (controlled by `Settings.ip_salt_rotation_secs`).
/// Once the salt has rotated, stored hashes cannot be correlated back to any IP.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IpHash(pub String);

impl IpHash {
    /// Wrap a pre-computed hex string as an `IpHash`.
    pub fn new(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    /// Return the inner hex string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IpHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A storage key identifying a media object (e.g. `tech/1a2b/3c4d/uuid.jpg`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaKey(pub String);

impl MediaKey {
    /// Create a `MediaKey` from a path string.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl std::fmt::Display for MediaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// SHA-256 hash of the original file bytes. Used for duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub String);

impl ContentHash {
    /// Wrap a pre-computed hex string as a `ContentHash`.
    pub fn new(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    /// Return the inner hex string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated board slug matching `^[a-z0-9_-]{1,16}$`.
///
/// Slugs appear in URLs and are the human-readable identifier for a board.
/// They are validated at construction; once constructed, a `Slug` is always valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Slug(String);

impl Slug {
    /// Validate and wrap a slug string.
    ///
    /// Returns `ValidationError::InvalidSlug` if the value does not match
    /// `^[a-z0-9_-]{1,16}$`.
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let v = value.into();
        // INVARIANT: must stay in sync with SLUG_PATTERN and the DB CHECK constraint
        let valid = !v.is_empty()
            && v.len() <= 16
            && v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
        if valid {
            Ok(Self(v))
        } else {
            Err(ValidationError::InvalidSlug { value: v })
        }
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// File size in kilobytes. Used in `BoardConfig` and `Attachment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileSizeKb(pub u32);

impl FileSizeKb {
    /// Create a `FileSizeKb` from a kilobyte value.
    pub fn new(kb: u32) -> Self {
        Self(kb)
    }
}

/// A pagination page number (1-indexed).
///
/// `Page` is validated to be ≥ 1 at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page(pub u32);

impl Page {
    /// Default page size used for paginated queries.
    pub const DEFAULT_PAGE_SIZE: u32 = 15;

    /// Construct a `Page`. Clamps to 1 if 0 is passed.
    pub fn new(n: u32) -> Self {
        Self(n.max(1))
    }

    /// Return the 0-based offset for use in SQL `OFFSET` clauses.
    pub fn offset(self, page_size: u32) -> u32 {
        (self.0 - 1) * page_size
    }
}

impl Default for Page {
    fn default() -> Self {
        Self(1)
    }
}

/// A page of results with total count metadata.
///
/// Used as the return type for all paginated repository methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// The total number of items across all pages.
    pub total: u64,
    /// The current page number (1-indexed).
    pub page: Page,
    /// Number of items per page.
    pub page_size: u32,
}

impl<T> Paginated<T> {
    /// Construct a `Paginated` wrapper.
    pub fn new(items: Vec<T>, total: u64, page: Page, page_size: u32) -> Self {
        Self { items, total, page, page_size }
    }

    /// Construct an empty `Paginated` — zero items, zero total.
    ///
    /// Used when a user has no boards and the log page should render empty
    /// rather than erroring.
    pub fn empty(page: Page, page_size: u32) -> Self {
        Self { items: vec![], total: 0, page, page_size }
    }

    /// True if there is a next page.
    pub fn has_next(&self) -> bool {
        let fetched = (self.page.0 as u64 - 1) * self.page_size as u64 + self.items.len() as u64;
        fetched < self.total
    }

    /// True if there is a previous page.
    pub fn has_prev(&self) -> bool {
        self.page.0 > 1
    }

    /// Total number of pages.
    pub fn total_pages(&self) -> u64 {
        if self.page_size == 0 {
            return 0;
        }
        self.total.div_ceil(self.page_size as u64)
    }
}

/// A signed authentication token (JWT bearer token in v1.0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token(pub String);

impl Token {
    /// Wrap a raw token string.
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An argon2id password hash stored as a PHC string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordHash(pub String);

impl PasswordHash {
    /// Wrap a PHC-format hash string.
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }
}

/// The claims encoded inside an authentication token.
///
/// These are the facts the system knows about the authenticated user for the
/// duration of their token's validity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// The authenticated user's identifier.
    pub user_id: UserId,
    /// The authenticated user's display name (included so handlers can render it without a DB call).
    pub username: String,
    /// The user's role at the time of token issuance.
    pub role: Role,
    /// The boards this user owns at the time of token issuance.
    pub owned_boards: Vec<BoardId>,
    /// The boards this user is assigned as a volunteer at the time of token issuance.
    pub volunteer_boards: Vec<BoardId>,
    /// Unix timestamp (seconds) at which this token expires.
    pub exp: i64,
}

/// The outcome of resolving a flag (report).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlagResolution {
    /// The flag was approved — the reported content is indeed a violation.
    Approved,
    /// The flag was rejected — the report was not actioned.
    Rejected,
}

// ─── Role ────────────────────────────────────────────────────────────────────

/// The privilege level of a moderator/admin user account.
///
/// Anonymous posters have no `Role` — they are identified only by `IpHash`.
/// `Role::User` is the lowest registered tier — no moderation powers, can submit staff requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Registered account. No moderation powers. Can submit staff requests.
    User,
    /// Board volunteer — can moderate their assigned boards only.
    BoardVolunteer,
    /// Owns a specific board — manages its config, volunteers, and local bans.
    BoardOwner,
    /// Global janitor — can moderate all boards site-wide.
    Janitor,
    /// Full site access: board CRUD, user management, all moderation.
    Admin,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User           => f.write_str("user"),
            Role::BoardVolunteer => f.write_str("board_volunteer"),
            Role::BoardOwner     => f.write_str("board_owner"),
            Role::Janitor        => f.write_str("janitor"),
            Role::Admin          => f.write_str("admin"),
        }
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user"            => Ok(Role::User),
            "board_volunteer" => Ok(Role::BoardVolunteer),
            "board_owner"     => Ok(Role::BoardOwner),
            "janitor"         => Ok(Role::Janitor),
            "admin"           => Ok(Role::Admin),
            other             => Err(format!("unknown role: {other}")),
        }
    }
}

// ─── Domain Models ───────────────────────────────────────────────────────────

/// A board — the top-level container for threads.
///
/// Boards are identified by a short slug (e.g. `tech`, `art`) that appears in URLs.
/// Each board has exactly one associated `BoardConfig` row in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    /// Unique identifier.
    pub id: BoardId,
    /// URL-safe identifier matching `^[a-z0-9_-]{1,16}$`.
    pub slug: Slug,
    /// Human-readable display name (1–64 characters).
    pub title: String,
    /// Markdown rules text shown at the top of the board.
    pub rules: String,
    /// When this board was created.
    pub created_at: DateTime<Utc>,
}

/// A thread — a collection of posts initiated by an OP post.
///
/// Threads are associated with exactly one board and belong to the board until
/// pruned. The `bumped_at` timestamp controls thread ordering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Unique identifier.
    pub id: ThreadId,
    /// The board this thread belongs to.
    pub board_id: BoardId,
    /// The ID of the opening post. `None` until the OP post is inserted.
    pub op_post_id: Option<PostId>,
    /// Number of reply posts (excluding the OP).
    pub reply_count: u32,
    /// The last time this thread was bumped (controls sort order).
    pub bumped_at: DateTime<Utc>,
    /// Sticky threads appear above non-sticky threads regardless of bump time.
    pub sticky: bool,
    /// Closed threads do not accept new posts.
    pub closed: bool,
    // TODO v1.2: cycle mode — when `true`, the oldest unpinned post is pruned when a new
    // reply would exceed the bump limit instead of the thread expiring.
    // Requires migration adding `cycle BOOLEAN NOT NULL DEFAULT FALSE` to `threads`
    // and a `set_cycle(id, bool)` method on `ThreadRepository`.
    // pub cycle: bool,
    /// When this thread was created.
    pub created_at: DateTime<Utc>,
}

/// A summary of a thread used for catalog views.
///
/// Contains only enough information to render the catalog grid tile —
/// the OP text preview and thumbnail key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    /// Thread identifier.
    pub thread_id: ThreadId,
    /// Board identifier.
    pub board_id: BoardId,
    /// The opening post body (may be truncated for display).
    pub op_body: String,
    /// The first attachment thumbnail key, if any.
    pub thumbnail_key: Option<MediaKey>,
    /// Number of replies.
    pub reply_count: u32,
    /// Whether the thread is sticky.
    pub sticky: bool,
    /// Whether the thread is closed.
    pub closed: bool,
    /// Last bump time.
    pub bumped_at: DateTime<Utc>,
}

/// A post — the atomic unit of content in rusty-board.
///
/// Posts belong to a thread and are ordered by creation time within the thread.
/// Anonymous posts have no `name` or `tripcode`. The raw IP is never stored —
/// only its daily-salted SHA-256 hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    /// Unique identifier.
    pub id: PostId,
    /// The thread this post belongs to.
    pub thread_id: ThreadId,
    /// The post body text (may be empty if the post contains only attachments).
    pub body: String,
    /// Daily-salted SHA-256 of the poster's IP address. Never the raw IP.
    pub ip_hash: IpHash,
    /// Poster's display name. `None` if posted anonymously.
    pub name: Option<String>,
    /// Tripcode derived from a password hash. `None` unless tripcodes are enabled.
    pub tripcode: Option<String>,
    /// Email field. `Some("sage")` disables thread bump; otherwise unused.
    pub email: Option<String>,
    /// When this post was created.
    pub created_at: DateTime<Utc>,
    /// Board-scoped sequential number (1-indexed). Used in `No.N` display.
    pub post_number: u64,
    // TODO v1.2: pinned — when `true` in a cycle thread, this post is never pruned.
    // Requires migration adding `pinned BOOLEAN NOT NULL DEFAULT FALSE` to `posts`
    // and a `set_pinned(id, bool)` method on `PostRepository`.
    // pub pinned: bool,
}

/// A lightweight post entry for the overboard view, enriched with board context.
///
/// The overboard lists recent posts across all boards. Unlike `Post`, this type
/// carries the `board_slug` so the template can build a link to the parent thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverboardPost {
    /// Unique post identifier.
    pub id: PostId,
    /// The thread this post belongs to.
    pub thread_id: ThreadId,
    /// Slug of the board the thread belongs to (e.g. `"b"` or `"tech"`).
    pub board_slug: String,
    /// Post body text.
    pub body: String,
    /// Poster display name, or `None` if anonymous.
    pub name: Option<String>,
    /// When this post was created.
    pub created_at: DateTime<Utc>,
    /// Board-scoped sequential post number, same as `Post::post_number`.
    pub post_number: u64,
}

/// A media attachment associated with a post.
///
/// Attachments are stored in the configured `MediaStorage` backend. The database
/// stores only keys and metadata; raw bytes live in S3 or the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Unique identifier.
    pub id: Uuid,
    /// The post this attachment belongs to.
    pub post_id: PostId,
    /// The original filename as uploaded by the poster.
    pub filename: String,
    /// The validated MIME type of the attachment.
    pub mime: String,
    /// SHA-256 of the original file bytes (for deduplication in v1.2).
    pub hash: ContentHash,
    /// File size in kilobytes.
    pub size_kb: u32,
    /// Storage key for the original file.
    pub media_key: MediaKey,
    /// Storage key for the generated thumbnail. `None` if not applicable.
    pub thumbnail_key: Option<MediaKey>,
    /// Whether this attachment is marked as a spoiler (blurred until clicked).
    pub spoiler: bool,
}

/// An IP ban record.
///
/// Bans are enforced by `ip_hash`. A poster who changes their IP is no longer banned.
/// Bans may be permanent (`expires_at = None`) or time-limited.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ban {
    /// Unique identifier.
    pub id: BanId,
    /// The banned IP hash.
    pub ip_hash: IpHash,
    /// The user account that issued this ban.
    pub banned_by: UserId,
    /// Reason displayed to the banned poster on their next post attempt.
    pub reason: String,
    /// When this ban expires. `None` = permanent ban.
    pub expires_at: Option<DateTime<Utc>>,
    /// When this ban was issued.
    pub created_at: DateTime<Utc>,
}

/// A flag (report) submitted by a visitor against a post.
///
/// Flags appear in the moderator's flag queue until resolved (approved or rejected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flag {
    /// Unique identifier.
    pub id: FlagId,
    /// The post being reported.
    pub post_id: PostId,
    /// The reason provided by the reporter.
    pub reason: String,
    /// The IP hash of the reporter.
    pub reporter_ip_hash: IpHash,
    /// Current status of the flag.
    pub status: FlagStatus,
    /// The moderator who resolved this flag, if resolved.
    pub resolved_by: Option<UserId>,
    /// When this flag was submitted.
    pub created_at: DateTime<Utc>,
}

/// Status of a moderation flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlagStatus {
    /// Awaiting moderator review.
    Pending,
    /// Approved — content actioned.
    Approved,
    /// Rejected — report not valid.
    Rejected,
}

impl std::fmt::Display for FlagStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlagStatus::Pending  => f.write_str("pending"),
            FlagStatus::Approved => f.write_str("approved"),
            FlagStatus::Rejected => f.write_str("rejected"),
        }
    }
}

impl FromStr for FlagStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending"  => Ok(FlagStatus::Pending),
            "approved" => Ok(FlagStatus::Approved),
            "rejected" => Ok(FlagStatus::Rejected),
            other      => Err(format!("unknown flag status: {other}")),
        }
    }
}

/// A moderator or admin user account.
///
/// Anonymous posters are not `User`s — they have no account. Only moderators,
/// janitors, and admins have `User` records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier.
    pub id: UserId,
    /// Login username (3–32 characters, alphanumeric + underscore).
    pub username: String,
    /// Argon2id PHC-format password hash.
    pub password_hash: PasswordHash,
    /// The privilege level of this account.
    pub role: Role,
    /// Inactive accounts cannot log in. Deactivation is a soft delete.
    pub is_active: bool,
    /// When this account was created.
    pub created_at: DateTime<Utc>,
}

/// An audit log entry recording a moderation action.
///
/// Every privileged action (delete post, ban IP, resolve flag, etc.) writes
/// one `AuditEntry`. Entries are never deleted. The audit log is append-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier.
    pub id: Uuid,
    /// The moderator who performed the action. `None` for anonymous actions.
    pub actor_id: Option<UserId>,
    /// The IP hash of the actor when no account is involved.
    pub actor_ip_hash: Option<IpHash>,
    /// The action that was performed.
    pub action: AuditAction,
    /// The target entity UUID (post, thread, user, board, etc.).
    pub target_id: Option<Uuid>,
    /// The kind of entity targeted.
    pub target_type: Option<String>,
    /// Action-specific detail payload (JSON).
    pub details: Option<serde_json::Value>,
    /// When this action occurred.
    pub created_at: DateTime<Utc>,
}

/// The kind of moderation action recorded in an audit log entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// A post was deleted by a moderator or admin.
    DeletePost,
    /// A thread (and all its posts) was deleted by a moderator or admin.
    DeleteThread,
    /// A thread was stickied or un-stickied.
    StickyThread,
    /// A thread was closed to new replies, or re-opened.
    CloseThread,
    /// An IP hash was banned.
    BanIp,
    /// An active ban was manually expired before its scheduled expiry.
    ExpireBan,
    /// A content flag was resolved (approved or rejected).
    ResolveFlag,
    /// A board's `BoardConfig` was updated via the dashboard.
    UpdateBoardConfig,
    /// A new board was created.
    CreateBoard,
    /// A board was deleted.
    DeleteBoard,
    /// A new staff user account was created.
    CreateUser,
    /// A staff user account was deactivated (soft delete).
    DeactivateUser,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuditAction::DeletePost        => "delete_post",
            AuditAction::DeleteThread      => "delete_thread",
            AuditAction::StickyThread      => "sticky_thread",
            AuditAction::CloseThread       => "close_thread",
            AuditAction::BanIp             => "ban_ip",
            AuditAction::ExpireBan         => "expire_ban",
            AuditAction::ResolveFlag       => "resolve_flag",
            AuditAction::UpdateBoardConfig => "update_board_config",
            AuditAction::CreateBoard       => "create_board",
            AuditAction::DeleteBoard       => "delete_board",
            AuditAction::CreateUser        => "create_user",
            AuditAction::DeactivateUser    => "deactivate_user",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for AuditAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "delete_post"        => Ok(AuditAction::DeletePost),
            "delete_thread"      => Ok(AuditAction::DeleteThread),
            "sticky_thread"      => Ok(AuditAction::StickyThread),
            "close_thread"       => Ok(AuditAction::CloseThread),
            "ban_ip"             => Ok(AuditAction::BanIp),
            "expire_ban"         => Ok(AuditAction::ExpireBan),
            "resolve_flag"       => Ok(AuditAction::ResolveFlag),
            "update_board_config" => Ok(AuditAction::UpdateBoardConfig),
            "create_board"       => Ok(AuditAction::CreateBoard),
            "delete_board"       => Ok(AuditAction::DeleteBoard),
            "create_user"        => Ok(AuditAction::CreateUser),
            "deactivate_user"    => Ok(AuditAction::DeactivateUser),
            other => Err(format!("unknown AuditAction: {other}")),
        }
    }
}

// ─── BoardConfig ─────────────────────────────────────────────────────────────

/// The runtime behaviour surface for a board.
///
/// One row exists in `board_configs` per board. Operators change these fields
/// through the admin or board-owner dashboard; no redeployment is required.
///
/// # INVARIANT
/// `BoardConfig` is the **only** path from a dashboard control to service
/// behaviour. Services branch on its fields. They never read environment
/// variables, feature flags, or global state.
///
/// # Caching
/// `BoardConfig` is cached in-process with a 60-second TTL in a
/// `DashMap<BoardId, (BoardConfig, Instant)>`. The cache is invalidated
/// immediately on `PUT /board/:slug/config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardConfig {
    // ── Content rules ──────────────────────────────────────────────────────
    /// Replies past this count no longer bump the thread. Default: 500.
    pub bump_limit: u32,
    /// Maximum number of file attachments per post. Default: 4.
    pub max_files: u8,
    /// Maximum file size per attachment in kilobytes. Default: 10240 (10 MB).
    pub max_file_size: FileSizeKb,
    /// MIME types allowed as attachments. Default: jpeg, png, gif, webp.
    pub allowed_mimes: Vec<String>,
    /// Maximum post body length in characters. Default: 4000.
    pub max_post_length: u32,

    // ── Rate limiting ──────────────────────────────────────────────────────
    /// Whether IP-based rate limiting is enforced on this board. Default: true.
    pub rate_limit_enabled: bool,
    /// Rolling window length for rate limiting in seconds. Default: 60.
    pub rate_limit_window_secs: u32,
    /// Maximum posts per IP within the rate-limit window. Default: 3.
    pub rate_limit_posts: u32,

    // ── Spam filtering ─────────────────────────────────────────────────────
    /// Whether spam heuristics are run on new posts. Default: true.
    pub spam_filter_enabled: bool,
    /// A post with a spam score ≥ this threshold is rejected. Default: 0.75.
    pub spam_score_threshold: f32,
    /// Reject posts whose content hash matches a recent post on this board. Default: true.
    pub duplicate_check: bool,
    /// Domains whose URLs are blocked in post bodies. Empty list = no blacklist. Default: [].
    ///
    /// Any post body containing a URL whose hostname matches an entry in this list
    /// is rejected with `PostError::SpamDetected`. Matching is case-insensitive and
    /// checks the full hostname (e.g. `"spam.example.com"` matches that host only,
    /// not `"example.com"`).
    pub link_blacklist: Vec<String>,
    /// Minimum seconds that must pass between posts from the same name+tripcode
    /// combination on this board. `0` disables the check. Default: 0.
    ///
    /// Prevents a single identity from flooding a board without triggering the
    /// IP-based rate limiter (e.g. via proxies). Only applied when `forced_anon`
    /// is false and the poster provides a name.
    pub name_rate_limit_window_secs: u32,

    // ── Posting behaviour ──────────────────────────────────────────────────
    /// When true, the name field is ignored and all posts display as "Anonymous". Default: false.
    pub forced_anon: bool,
    /// Allow posters to sage (prevent bump by setting email to "sage"). Default: true.
    pub allow_sage: bool,
    /// Allow tripcode identifiers. Adapter not compiled in v1.0. Default: false.
    // TODO(v1.1): wire CaptchaVerifier port when captcha_required = true
    pub allow_tripcodes: bool,
    /// Require CAPTCHA verification on post creation. Adapter not compiled in v1.0. Default: false.
    // TODO(v1.1): wire CaptchaVerifier port when captcha_required = true
    pub captcha_required: bool,
    /// This board contains adult/NSFW content. Default: false.
    pub nsfw: bool,

    // ── Future capabilities ─────────────────────────────────────────────────
    // Fields are present now so that the schema is stable; the adapters that
    // actually act on these toggles ship in later versions.
    /// Full-text search enabled. Adapter ships in v1.2. Default: false.
    // TODO(v1.2): wire SearchIndex port when search_enabled = true
    pub search_enabled: bool,
    /// Posts are archived before pruning. Adapter ships in v1.2. Default: false.
    // TODO(v1.2): implement archive adapter when archive_enabled = true
    pub archive_enabled: bool,
    /// ActivityPub federation enabled. Adapter ships in v2.0. Default: false.
    // TODO(v2.0): wire FederationSync port when federation_enabled = true
    pub federation_enabled: bool,
}

impl Default for BoardConfig {
    /// Conservative defaults — safe for a new board with no custom configuration.
    fn default() -> Self {
        Self {
            bump_limit:             500,
            max_files:              4,
            max_file_size:          FileSizeKb(10240),
            allowed_mimes:          vec![
                "image/jpeg".to_owned(),
                "image/png".to_owned(),
                "image/gif".to_owned(),
                "image/webp".to_owned(),
            ],
            max_post_length:        4000,
            rate_limit_enabled:     true,
            rate_limit_window_secs: 60,
            rate_limit_posts:       3,
            spam_filter_enabled:         true,
            spam_score_threshold:        0.75,
            duplicate_check:             true,
            link_blacklist:              vec![],
            name_rate_limit_window_secs: 0,
            forced_anon:                 false,
            allow_sage:             true,
            allow_tripcodes:        false,
            captcha_required:       false,
            nsfw:                   false,
            search_enabled:         false,
            archive_enabled:        false,
            federation_enabled:     false,
        }
    }
}

impl BoardConfig {
    /// Check whether a given MIME type string is allowed by this config.
    pub fn allows_mime(&self, mime: &str) -> bool {
        self.allowed_mimes.iter().any(|m| m == mime)
    }

    /// Check whether a file of the given size (in KB) is within the limit.
    pub fn allows_file_size_kb(&self, size_kb: u32) -> bool {
        size_kb <= self.max_file_size.0
    }

    /// Check whether a post body of the given length is within the limit.
    pub fn allows_post_length(&self, len: usize) -> bool {
        len <= self.max_post_length as usize
    }
}

// ─── CurrentUser ─────────────────────────────────────────────────────────────

/// The authenticated user context available in request handlers.
///
/// Populated by the auth middleware from a verified JWT token. Handlers use this
/// to perform permission checks before calling services.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// The authenticated user's identifier.
    pub id: UserId,
    /// The authenticated user's display name (from JWT claim; no DB call needed).
    pub username: String,
    /// The user's role.
    pub role: Role,
    /// The board IDs this user owns (from `board_owners` join table).
    /// Used for board config and volunteer management permission checks.
    pub owned_boards: Vec<BoardId>,
    /// The board IDs this user is assigned as a volunteer (from `board_volunteers` join table).
    /// Used to scope the dashboard Boards/Logs/Posts sections for volunteer users.
    pub volunteer_boards: Vec<BoardId>,
}

impl CurrentUser {
    /// Construct a `CurrentUser` from verified JWT `Claims`.
    ///
    /// Called by the auth middleware after a token has been successfully verified.
    pub fn from_claims(claims: Claims) -> Self {
        Self {
            id:               claims.user_id,
            username:         claims.username,
            role:             claims.role,
            owned_boards:     claims.owned_boards,
            volunteer_boards: claims.volunteer_boards,
        }
    }

    /// Alias for `id` — matches the field name on `Claims` and used in handlers.
    pub fn user_id(&self) -> UserId {
        self.id
    }

    /// Returns `true` if this user has admin privileges.
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Returns `true` if this user has site-wide moderation capability (Janitor or Admin).
    pub fn is_moderator_or_above(&self) -> bool {
        matches!(self.role, Role::Janitor | Role::Admin)
    }

    /// Returns `true` if this user may update config for the given board.
    pub fn can_manage_board_config(&self, board_id: BoardId) -> bool {
        self.is_admin() || self.owned_boards.contains(&board_id)
    }

    /// Returns `true` if this user may perform moderation actions.
    pub fn can_moderate(&self) -> bool {
        matches!(self.role, Role::Janitor | Role::Admin)
    }

    /// Returns `true` if this user may delete posts or issue bans on any board
    /// they have authority over (owned, assigned, or site-wide).
    pub fn can_delete(&self) -> bool {
        matches!(self.role, Role::BoardVolunteer | Role::BoardOwner | Role::Janitor | Role::Admin)
    }

    /// Returns `true` if this user may moderate the given board specifically.
    /// Admins and Janitors may moderate any board.
    /// BoardOwners may moderate boards they own.
    /// BoardVolunteers may moderate boards they are assigned to.
    pub fn can_moderate_board(&self, board_id: BoardId) -> bool {
        self.is_moderator_or_above()
            || self.owned_boards.contains(&board_id)
            || self.volunteer_boards.contains(&board_id)
    }

    /// All board IDs this user has any moderation authority over.
    /// Returns `None` for Admin/Janitor (site-wide — no list needed).
    pub fn scoped_boards(&self) -> Option<Vec<BoardId>> {
        if self.is_moderator_or_above() {
            None // site-wide authority — handler queries all boards
        } else {
            let mut boards = self.owned_boards.clone();
            for b in &self.volunteer_boards {
                if !boards.contains(b) {
                    boards.push(*b);
                }
            }
            Some(boards)
        }
    }

    /// Returns the display name for this user's role.
    pub fn role_display(&self) -> &'static str {
        match self.role {
            Role::Admin          => "Admin",
            Role::Janitor        => "Janitor",
            Role::BoardOwner     => "Board Owner",
            Role::BoardVolunteer => "Board Volunteer",
            Role::User           => "User",
        }
    }
}

// ─── StaffRequest ─────────────────────────────────────────────────────────────

/// The type of staff promotion request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaffRequestType {
    /// Request to create a new board and become its owner.
    BoardCreate,
    /// Request to become a volunteer on an existing board.
    BecomeVolunteer,
    /// Request to become a site-wide janitor.
    BecomeJanitor,
}

impl std::fmt::Display for StaffRequestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StaffRequestType::BoardCreate     => f.write_str("board_create"),
            StaffRequestType::BecomeVolunteer => f.write_str("become_volunteer"),
            StaffRequestType::BecomeJanitor   => f.write_str("become_janitor"),
        }
    }
}

impl std::str::FromStr for StaffRequestType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "board_create"     => Ok(StaffRequestType::BoardCreate),
            "become_volunteer" => Ok(StaffRequestType::BecomeVolunteer),
            "become_janitor"   => Ok(StaffRequestType::BecomeJanitor),
            other              => Err(format!("unknown request type: {other}")),
        }
    }
}

/// The review status of a staff request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaffRequestStatus {
    /// Awaiting review by an admin or board owner.
    Pending,
    /// Request was accepted; the appropriate role/board promotion was applied.
    Approved,
    /// Request was rejected; see `review_note` for the reason.
    Denied,
}

impl std::fmt::Display for StaffRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StaffRequestStatus::Pending  => f.write_str("pending"),
            StaffRequestStatus::Approved => f.write_str("approved"),
            StaffRequestStatus::Denied   => f.write_str("denied"),
        }
    }
}

impl std::str::FromStr for StaffRequestStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending"  => Ok(StaffRequestStatus::Pending),
            "approved" => Ok(StaffRequestStatus::Approved),
            "denied"   => Ok(StaffRequestStatus::Denied),
            other      => Err(format!("unknown request status: {other}")),
        }
    }
}

/// A request submitted by a `Role::User` to join the staff pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffRequest {
    /// Unique identifier for this request.
    pub id:           StaffRequestId,
    /// The user who submitted this request.
    pub from_user_id: UserId,
    /// What kind of promotion is being requested.
    pub request_type: StaffRequestType,
    /// For `BecomeVolunteer`: the slug of the board being requested.
    pub target_slug:  Option<Slug>,
    /// JSON payload — contents depend on `request_type`.
    pub payload:      serde_json::Value,
    /// Current review status.
    pub status:       StaffRequestStatus,
    /// The staff member who approved or denied this request, if reviewed.
    pub reviewed_by:  Option<UserId>,
    /// Optional note from the reviewer shown to the requester.
    pub review_note:  Option<String>,
    /// When the request was submitted.
    pub created_at:   chrono::DateTime<chrono::Utc>,
    /// When the request was last updated (status change, note edit).
    pub updated_at:   chrono::DateTime<chrono::Utc>,
}

// ─── Value object: StaffRequestId ────────────────────────────────────────────

/// Newtype wrapper around UUID for staff request IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StaffRequestId(pub uuid::Uuid);

impl StaffRequestId {
    /// Create a new random `StaffRequestId`.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
    /// Return the underlying UUID.
    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

impl Default for StaffRequestId {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Display for StaffRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_valid_values() {
        assert!(Slug::new("tech").is_ok());
        assert!(Slug::new("a").is_ok());
        assert!(Slug::new("my-board").is_ok());
        assert!(Slug::new("board_1").is_ok());
        assert!(Slug::new("1234567890123456").is_ok()); // exactly 16 chars
    }

    #[test]
    fn slug_invalid_values() {
        assert!(Slug::new("").is_err()); // empty
        assert!(Slug::new("12345678901234567").is_err()); // 17 chars
        assert!(Slug::new("UPPER").is_err()); // uppercase
        assert!(Slug::new("has space").is_err()); // space
        assert!(Slug::new("spe©ial").is_err()); // non-ascii
    }

    #[test]
    fn board_config_default_allows_standard_images() {
        let cfg = BoardConfig::default();
        assert!(cfg.allows_mime("image/jpeg"));
        assert!(cfg.allows_mime("image/png"));
        assert!(cfg.allows_mime("image/gif"));
        assert!(cfg.allows_mime("image/webp"));
        assert!(!cfg.allows_mime("application/pdf"));
        assert!(!cfg.allows_mime("video/mp4"));
    }

    #[test]
    fn board_config_default_conservative() {
        let cfg = BoardConfig::default();
        assert!(cfg.rate_limit_enabled);
        assert!(cfg.spam_filter_enabled);
        assert!(cfg.duplicate_check);
        assert!(!cfg.forced_anon);
        assert!(!cfg.nsfw);
        assert!(!cfg.allow_tripcodes);
        assert!(!cfg.captcha_required);
        assert!(!cfg.search_enabled);
        assert!(!cfg.archive_enabled);
        assert!(!cfg.federation_enabled);
    }

    #[test]
    fn paginated_helpers() {
        let p: Paginated<i32> = Paginated::new(vec![1, 2, 3], 30, Page::new(1), 15);
        assert!(p.has_next());
        assert!(!p.has_prev());
        assert_eq!(p.total_pages(), 2);

        let last: Paginated<i32> = Paginated::new(vec![1], 16, Page::new(2), 15);
        assert!(!last.has_next());
        assert!(last.has_prev());
    }
}

// ─── StaffMessage ─────────────────────────────────────────────────────────────

/// A text-only message between staff accounts.
///
/// Sender rules (enforced at service layer):
///   Admin → any staff
///   BoardOwner → their volunteers + janitors
/// Messages expire after 14 days (periodic cleanup task).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffMessage {
    /// Unique identifier.
    pub id:           StaffMessageId,
    /// The user who sent the message.
    pub from_user_id: UserId,
    /// The user who received the message.
    pub to_user_id:   UserId,
    /// Message body text (1–4000 characters).
    pub body:         String,
    /// When the recipient read the message. `None` means unread.
    pub read_at:      Option<chrono::DateTime<chrono::Utc>>,
    /// When the message was sent.
    pub created_at:   chrono::DateTime<chrono::Utc>,
}

impl StaffMessage {
    /// Returns `true` if the message has not yet been read.
    pub fn is_unread(&self) -> bool {
        self.read_at.is_none()
    }
}

// ─── Value object: StaffMessageId ────────────────────────────────────────────

/// Newtype wrapper around UUID for staff message IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StaffMessageId(pub uuid::Uuid);

impl StaffMessageId {
    /// Create a new random `StaffMessageId`.
    pub fn new() -> Self { Self(uuid::Uuid::new_v4()) }
    /// Return the underlying UUID.
    pub fn as_uuid(&self) -> uuid::Uuid { self.0 }
}

impl Default for StaffMessageId {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Display for StaffMessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
