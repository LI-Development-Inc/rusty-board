//! Askama template type declarations.
//!
//! All templates are compile-time checked by Askama against the HTML files in
//! `templates/`. This module declares the Rust structs that map to each template.
//!
//! `IntoResponse` is implemented manually for each template struct. This was
//! previously provided automatically by the `askama_axum` crate (now deprecated)
//! and by askama's `with-axum` feature (removed in 0.15).

use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use domains::models::{Board, BoardConfig, OverboardPost, Post, Thread, ThreadSummary, User};

/// A post bundled with its per-thread poster ID badge for template rendering.
///
/// The `poster_id` is the first 8 hex characters of SHA-256(`ip_hash + "/" + thread_id`).
/// It is stable per poster per thread — the same IP always gets the same short ID within
/// a thread, but different IDs in different threads (preventing cross-thread tracking).
#[derive(Debug, Clone)]
pub struct PostDisplay {
    /// The underlying post data.
    pub post: Post,
    /// Short 8-char hex string used as the coloured poster ID badge.
    pub poster_id: String,
    /// Media attachments uploaded with this post.
    pub attachments: Vec<domains::models::Attachment>,
    /// If this post has a capcode, the role display string (e.g. `"Admin"`, `"Board Owner"`).
    /// `None` for regular posts and tripcode posts.
    pub capcode_role: Option<String>,
    /// CSS class suffix for the capcode, e.g. `"admin"` or `"board-owner"`.
    /// `None` when `capcode_role` is `None`.
    pub capcode_css: Option<String>,
    /// Tripcode security level for CSS styling: `"insecure"`, `"secure"`, or `"super"`.
    /// `None` when the post has no tripcode (or has a capcode instead).
    pub tripcode_level: Option<&'static str>,
    /// First 10 characters of the IP hash for truncated mod display.
    pub ip_hash_short: String,
}

/// Render a template to an HTML response, returning 500 on render failure.
fn render_template(tmpl: impl Template) -> Response {
    match tmpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "template render error");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Base layout values available to all templates.
#[derive(Debug)]
pub struct BaseContext {
    /// Site display name injected into the base layout.
    pub site_name: String,
}

/// Template for the board view (`board.html`) — list of threads.
/// A thread summary enriched with display-time computed fields for the board index.
#[derive(Debug, Clone)]
pub struct BoardThreadDisplay {
    pub thread:         domains::models::ThreadSummary,
    pub poster_id:      String,
    pub tripcode_level: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "board.html")]
pub struct BoardTemplate {
    pub board:        Board,
    pub config:       BoardConfig,
    pub threads:      Vec<BoardThreadDisplay>,
    pub total_pages:  u32,
    pub current_page: u32,
}

impl IntoResponse for BoardTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the catalog view (`catalog.html`) — grid of thread thumbnails.
#[derive(Template)]
#[template(path = "catalog.html")]
pub struct CatalogTemplate {
    /// The board whose threads are being displayed in catalog format.
    pub board:   Board,
    /// Thread summaries (OP excerpt + thumbnail + reply count) for the grid.
    pub threads: Vec<ThreadSummary>,
    /// Board config needed to render the new-thread form correctly.
    pub config:  domains::models::BoardConfig,
}

impl IntoResponse for CatalogTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for a thread view (`thread.html`) — all posts in a thread.
#[derive(Template)]
#[template(path = "thread.html")]
pub struct ThreadTemplate {
    /// The board this thread belongs to.
    pub board:       Board,
    /// The thread being rendered.
    pub thread:      Thread,
    /// All posts in the thread with per-thread poster ID badges and attachments.
    /// Threads show every post up to the bump limit — no pagination.
    pub posts:       Vec<PostDisplay>,
    /// Whether the thread is closed to new replies.
    pub is_closed:   bool,
    /// Role string for the viewing staff member, e.g. `"janitor"`.
    /// `None` for anonymous visitors and registered users without moderation rights.
    pub viewer_role: Option<String>,
}

impl IntoResponse for ThreadTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// An overboard post bundled with its media attachments for template rendering.
#[derive(Debug, Clone)]
pub struct OverboardPostDisplay {
    /// The post data with board context.
    pub post: OverboardPost,
    /// Media attachments for this post (may be empty).
    pub attachments: Vec<domains::models::Attachment>,
    /// Short 8-char hex poster ID (SHA-256 of ip_hash + thread_id).
    pub poster_id: String,
    /// Tripcode security level: `"insecure"`, `"secure"`, `"super"`, or `None`.
    pub tripcode_level: Option<&'static str>,
    /// First 10 chars of ip_hash for truncated mod display.
    pub ip_hash_short: String,
}

/// Template for the overboard view (`overboard.html`) — recent posts across all boards.
#[derive(Template)]
#[template(path = "overboard.html")]
pub struct OverboardTemplate {
    /// All boards, used to render the board list sidebar.
    pub boards:       Vec<Board>,
    /// Recent posts across all boards on the current page, with attachments.
    pub recent_posts: Vec<OverboardPostDisplay>,
    /// The page number currently being rendered (1-indexed).
    pub current_page: u32,
    /// Total number of pages in the recent-posts feed.
    pub total_pages:  u32,
}

impl IntoResponse for OverboardTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the login page (`login.html`).
#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    /// An optional error message to display above the login form (e.g. "Invalid credentials").
    pub error: Option<String>,
}

impl IntoResponse for LoginTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

// ─── Unified Dashboard ────────────────────────────────────────────────────────
//
// All roles share one template (`dashboard.html`) and one context struct.
// Each section is populated based on the user's role + owned_boards + volunteer_boards.
// Empty vecs and None fields hide sections; the template never branches on role name.

/// A board row in the dashboard Boards table.
#[derive(Debug, Clone)]
pub struct DashboardBoard {
    /// The board record.
    pub board: Board,
    /// Show `[manage]` link to `/board/:slug/dashboard`.
    pub can_manage: bool,
    /// Show volunteer management controls on the per-board dashboard.
    pub can_manage_volunteers: bool,
}

/// A user row in the dashboard Staff table.
#[derive(Debug, Clone)]
pub struct DashboardUser {
    /// The staff account.
    pub user: User,
    /// Show `[message]` link.
    pub can_message: bool,
    /// Show `[deactivate]` button (admin only).
    pub can_deactivate: bool,
}

/// Unified dashboard template — rendered for every role at their respective URL.
///
/// Per-route handlers build this context and return it; the single `dashboard.html`
/// template renders all sections, hiding those with no content.
#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    // ── Header ────────────────────────────────────────────────────────────────
    /// Display name for the current user's role: "Admin", "Janitor", etc.
    pub role_display: &'static str,

    // ── Announcements ─────────────────────────────────────────────────────────
    /// Staff messages addressed to this user (newest first). Empty = section hidden.
    pub announcements: Vec<domains::models::StaffMessage>,

    // ── Boards ────────────────────────────────────────────────────────────────
    /// Boards visible to this user, scoped by role + board membership.
    pub boards: Vec<DashboardBoard>,

    // ── Staff ─────────────────────────────────────────────────────────────────
    /// Staff accounts visible to this user. `None` = section absent.
    /// Admin: all accounts. Janitor: all (message only). BoardOwner: their volunteers.
    pub staff: Option<Vec<DashboardUser>>,

    // ── Recent Logs ───────────────────────────────────────────────────────────
    /// Last 10 audit entries scoped to this user's authority. Empty = section hidden.
    pub recent_logs: Vec<domains::models::AuditEntry>,

    // ── Recent Posts ──────────────────────────────────────────────────────────
    /// Recent posts scoped to this user's authority. Empty = section hidden.
    /// Uses `OverboardPost` so templates have board context (slug, thread ID) without a join.
    pub recent_posts: Vec<OverboardPost>,

    // ── Messages ──────────────────────────────────────────────────────────────
    /// Last 3 messages for inbox preview.
    pub messages: Vec<domains::models::StaffMessage>,
    /// Unread count for the nav badge.
    pub unread_count: u32,

    // ── Pending Requests ──────────────────────────────────────────────────────
    /// Pending staff requests. `None` = section absent (only Admin and BoardOwner see this).
    pub pending_requests: Option<Vec<domains::models::StaffRequest>>,
}

impl IntoResponse for DashboardTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the mod flags page (`mod_flags.html`).
#[derive(Template)]
#[template(path = "mod_flags.html")]
pub struct FlagsPageTemplate {
    /// Pending flags to display on this page.
    pub flags:       Vec<domains::models::Flag>,
    /// Current page number (1-indexed).
    pub page:        u32,
    /// Total number of pages.
    pub total_pages: u32,
}

impl IntoResponse for FlagsPageTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the mod bans page (`mod_bans.html`).
#[derive(Template)]
#[template(path = "mod_bans.html")]
pub struct BansPageTemplate {
    /// Active bans to display on this page.
    pub bans:        Vec<domains::models::Ban>,
    /// Current page number (1-indexed).
    pub page:        u32,
    /// Total number of pages.
    pub total_pages: u32,
}

impl IntoResponse for BansPageTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the board owner per-board dashboard (`board_owner_dashboard.html`).
///
/// This is NOT the unified dashboard — it is the deep-dive config surface for a
/// single board, reached via "Manage →" from the Boards table.
#[derive(Template)]
#[template(path = "board_owner_dashboard.html")]
pub struct BoardOwnerDashboardTemplate {
    /// The board this owner is managing.
    pub board:  Board,
    /// Current runtime configuration for the board.
    pub config: BoardConfig,
}

impl IntoResponse for BoardOwnerDashboardTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the registration page (`register.html`).
#[derive(Template)]
#[template(path = "register.html")]
pub struct RegisterTemplate {
    /// An optional error message to display above the form.
    pub error: Option<String>,
}

impl IntoResponse for RegisterTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

/// Template for the user dashboard (`user_dashboard.html`).
#[derive(Template)]
#[template(path = "user_dashboard.html")]
pub struct UserDashboardTemplate {
    /// Display name of the authenticated user.
    pub username:        String,
    /// When the account was created (formatted for display).
    pub joined_at:       String,
    /// This user's own staff requests (all statuses).
    pub pending_requests: Vec<domains::models::StaffRequest>,
}

impl IntoResponse for UserDashboardTemplate {
    fn into_response(self) -> Response { render_template(self) }
}

// ─── Audit Log Page ───────────────────────────────────────────────────────────

/// Template context for `/janitor/logs`, `/board-owner/logs`, `/volunteer/logs`.
///
/// Shows a paginated full audit log, optionally filtered to a board.
/// The `role_label` drives the page heading (e.g. "Janitor Audit Log").
#[derive(askama::Template)]
#[template(path = "audit_log.html")]
pub struct AuditLogTemplate {
    /// Heading label, e.g. "Janitor" or "Board Owner".
    pub role_label:  String,
    /// Paginated audit entries for this page.
    pub entries:     crate::common::pagination::PageResponse<domains::models::AuditEntry>,
    /// Currently authenticated user for the nav bar.
    pub current_user: domains::models::CurrentUser,
}

impl axum::response::IntoResponse for AuditLogTemplate {
    fn into_response(self) -> axum::response::Response { render_template(self) }
}

// ─── Staff Inbox Page ─────────────────────────────────────────────────────────

/// Template context for `GET /staff/messages` — rendered as HTML inbox.
#[derive(askama::Template)]
#[template(path = "staff_inbox.html")]
pub struct StaffInboxTemplate {
    pub messages:     Vec<domains::models::StaffMessage>,
    pub current_page: u32,
    pub total_pages:  u32,
    pub total:        u64,
}

impl axum::response::IntoResponse for StaffInboxTemplate {
    fn into_response(self) -> axum::response::Response { render_template(self) }
}

// ─── Staff Compose Page ───────────────────────────────────────────────────────

/// Template context for `GET /staff/messages/new` — compose a new message.
#[derive(askama::Template)]
#[template(path = "staff_compose.html")]
pub struct StaffComposeTemplate {
    /// Pre-filled recipient UUID (from `?to=` query param), or empty string.
    pub to_user_id: String,
}

impl axum::response::IntoResponse for StaffComposeTemplate {
    fn into_response(self) -> axum::response::Response { render_template(self) }
}
