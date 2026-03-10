//! Data Transfer Objects (DTOs) for API requests and responses.
//!
//! DTOs live in `common/` (not feature-gated) because they represent the
//! canonical request/response shapes regardless of which web framework is active.
//! Handlers in both `axum/` and `actix/` use the same DTOs.
//!
//! DTOs validate shape (types, required fields) but not business rules.
//! Business rule validation happens in services.

use domains::models::{FlagResolution, Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Board DTOs ──────────────────────────────────────────────────────────────

/// Request body for `POST /admin/boards` — create a new board.
#[derive(Debug, Deserialize)]
pub struct BoardCreate {
    /// URL slug matching `^[a-z0-9_-]{1,16}$`.
    pub slug:  String,
    /// Display name (1–64 characters).
    pub title: String,
    /// Optional rules text shown at the top of the board.
    #[serde(default)]
    pub rules: String,
}

/// Request body for `PUT /admin/boards/:id` — update board metadata.
#[derive(Debug, Deserialize)]
pub struct BoardUpdate {
    /// New title. `None` leaves the current title unchanged.
    pub title: Option<String>,
    /// New rules. `None` leaves the current rules unchanged.
    pub rules:  Option<String>,
}

/// Partial update to `BoardConfig` fields.
///
/// All fields are `Option` — only provided fields are applied to the existing config.
#[derive(Debug, Deserialize)]
pub struct BoardConfigUpdate {
    /// New bump limit. `None` leaves the current value unchanged.
    pub bump_limit:             Option<u32>,
    /// New maximum number of file attachments per post. `None` leaves unchanged.
    pub max_files:              Option<u8>,
    /// New maximum file size in kilobytes. `None` leaves unchanged.
    pub max_file_size_kb:       Option<u32>,
    /// New allowed MIME types. `None` leaves unchanged.
    pub allowed_mimes:          Option<Vec<String>>,
    /// New maximum post body length in characters. `None` leaves unchanged.
    pub max_post_length:        Option<u32>,
    /// Enable or disable rate limiting. `None` leaves unchanged.
    pub rate_limit_enabled:     Option<bool>,
    /// Rate limit window duration in seconds. `None` leaves unchanged.
    pub rate_limit_window_secs: Option<u32>,
    /// Maximum posts allowed per IP within the rate limit window. `None` leaves unchanged.
    pub rate_limit_posts:       Option<u32>,
    /// Enable or disable spam filtering. `None` leaves unchanged.
    pub spam_filter_enabled:    Option<bool>,
    /// Spam score threshold (0.0–1.0); posts above this are rejected. `None` leaves unchanged.
    pub spam_score_threshold:   Option<f32>,
    /// Enable or disable duplicate post detection. `None` leaves unchanged.
    pub duplicate_check:        Option<bool>,
    /// Force all posters to be anonymous (ignore the name field). `None` leaves unchanged.
    pub forced_anon:            Option<bool>,
    /// Allow `sage` in the email field to suppress thread bump. `None` leaves unchanged.
    pub allow_sage:             Option<bool>,
    /// Allow tripcodes in the name field. `None` leaves unchanged.
    pub allow_tripcodes:        Option<bool>,
    /// Require CAPTCHA verification before posting. `None` leaves unchanged.
    pub captcha_required:       Option<bool>,
    /// Mark the board as NSFW. `None` leaves unchanged.
    pub nsfw:                   Option<bool>,
}

impl BoardConfigUpdate {
    /// Apply only the provided (non-None) fields to an existing `BoardConfig`.
    pub fn apply_to(self, mut config: domains::models::BoardConfig) -> domains::models::BoardConfig {
        if let Some(v) = self.bump_limit             { config.bump_limit = v; }
        if let Some(v) = self.max_files              { config.max_files = v; }
        if let Some(v) = self.max_file_size_kb       { config.max_file_size = domains::models::FileSizeKb(v); }
        if let Some(v) = self.allowed_mimes          { config.allowed_mimes = v; }
        if let Some(v) = self.max_post_length        { config.max_post_length = v; }
        if let Some(v) = self.rate_limit_enabled     { config.rate_limit_enabled = v; }
        if let Some(v) = self.rate_limit_window_secs { config.rate_limit_window_secs = v; }
        if let Some(v) = self.rate_limit_posts       { config.rate_limit_posts = v; }
        if let Some(v) = self.spam_filter_enabled    { config.spam_filter_enabled = v; }
        if let Some(v) = self.spam_score_threshold   { config.spam_score_threshold = v; }
        if let Some(v) = self.duplicate_check        { config.duplicate_check = v; }
        if let Some(v) = self.forced_anon            { config.forced_anon = v; }
        if let Some(v) = self.allow_sage             { config.allow_sage = v; }
        if let Some(v) = self.allow_tripcodes        { config.allow_tripcodes = v; }
        if let Some(v) = self.captcha_required       { config.captcha_required = v; }
        if let Some(v) = self.nsfw                   { config.nsfw = v; }
        config
    }
}

// ─── Auth DTOs ───────────────────────────────────────────────────────────────

/// Request body for `POST /auth/register` — public self-registration.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// Desired username (3–32 alphanumeric characters and underscores).
    pub username: String,
    /// Password (minimum 12 characters; hashed before storage).
    pub password: String,
    /// Password confirmation — must match `password` exactly.
    pub password_confirm: String,
}

/// Request body for `POST /auth/login`.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// The staff account username.
    pub username: String,
    /// The plaintext password (transmitted over TLS; never stored raw).
    pub password: String,
}

/// Response body for a successful login.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// Signed JWT to include in subsequent requests as `Authorization: Bearer <token>`.
    pub token:      String,
    /// Unix timestamp (seconds) when the token expires.
    pub expires_at: i64,
}

// ─── Post / thread DTOs ──────────────────────────────────────────────────────

/// Pagination query parameters used across list endpoints.
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    /// Page number to retrieve (1-indexed). Defaults to `1`.
    #[serde(default = "default_page")]
    pub page: u32,
}

fn default_page() -> u32 {
    1
}

// ─── Moderation DTOs ─────────────────────────────────────────────────────────

/// Request body for `POST /mod/bans`.
#[derive(Debug, Deserialize)]
pub struct CreateBanRequest {
    /// IP hash to ban. Provided by the moderator after looking up posts by IP.
    pub ip_hash:    String,
    /// Human-readable reason displayed to the banned poster.
    pub reason:     String,
    /// Optional expiry. `None` = permanent ban.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Request body for `POST /mod/flags/:id/resolve`.
#[derive(Debug, Deserialize)]
pub struct ResolveFlagRequest {
    /// The resolution decision to apply to the flag.
    pub resolution: FlagResolutionDto,
}

/// Resolution decision for a content flag — mirrors `domains::models::FlagResolution`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlagResolutionDto {
    /// The flagged content was removed or actioned — flag is approved.
    Approved,
    /// The flagged content was reviewed and found acceptable — flag is rejected.
    Rejected,
}

impl From<FlagResolutionDto> for FlagResolution {
    fn from(d: FlagResolutionDto) -> Self {
        match d {
            FlagResolutionDto::Approved => FlagResolution::Approved,
            FlagResolutionDto::Rejected => FlagResolution::Rejected,
        }
    }
}

/// Request body for `POST /admin/users`.
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    /// Username for the new staff account (3–32 alphanumeric characters).
    pub username: String,
    /// Initial plaintext password (minimum 12 characters; hashed before storage).
    pub password: String,
    /// Role to assign to the new account.
    pub role:     RoleDto,
}

/// Staff role — mirrors `domains::models::Role` for JSON deserialization.
///
/// Accepted values in request bodies: `"admin"`, `"janitor"`, `"board_owner"`, `"board_volunteer"`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleDto {
    /// Full site access: CRUD boards, manage users, moderate everywhere.
    Admin,
    /// Global site-wide moderation on all boards.
    Janitor,
    /// Owns and configures one or more specific boards.
    #[serde(rename = "board_owner")]
    BoardOwner,
    /// Moderates the boards they are assigned to (by a board owner or admin).
    #[serde(rename = "board_volunteer")]
    BoardVolunteer,
}

impl From<RoleDto> for Role {
    fn from(d: RoleDto) -> Self {
        match d {
            RoleDto::Admin          => Role::Admin,
            RoleDto::Janitor        => Role::Janitor,
            RoleDto::BoardOwner     => Role::BoardOwner,
            RoleDto::BoardVolunteer => Role::BoardVolunteer,
        }
    }
}

/// Request body for `POST /board/:slug/thread/:id/flag`.
#[derive(Debug, Deserialize)]
pub struct CreateFlagRequest {
    /// Human-readable reason for the flag, provided by the user reporting the content.
    pub reason: String,
}

/// Request body for `POST /admin/boards/:id/owners`.
#[derive(Debug, Deserialize)]
pub struct AddBoardOwnerRequest {
    /// UUID of the staff user to assign as owner of the board.
    pub user_id: Uuid,
}
