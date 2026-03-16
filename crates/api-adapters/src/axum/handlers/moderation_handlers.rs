//! Moderation handlers: flags, bans, delete, sticky, close.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::common::{
    dtos::{CreateBanRequest, PaginationQuery, ResolveFlagRequest},
    errors::ApiError,
};
use crate::axum::middleware::auth::{AnyAuthenticatedUser, AuthenticatedUser, ModeratorUser};
use domains::models::{BanId, FlagId, IpHash, Page, ThreadId};
use services::board::BoardRepo;
use services::moderation::ModerationService;

/// Body for `POST /mod/threads/:id/sticky` and `POST /mod/threads/:id/close`.
#[derive(Deserialize)]
pub struct SetBoolRequest {
    /// The desired value (`true` to enable, `false` to disable).
    pub value: bool,
}

/// Body for `POST /mod/threads/:id/delete-by-ip`.
#[derive(Deserialize)]
pub struct DeleteByIpRequest {
    /// The ip_hash of the poster whose posts should be deleted from this thread.
    pub ip_hash: String,
}

/// `GET /mod/flags` — list pending flags, paginated.
pub async fn list_flags<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    _mod_user: ModeratorUser,
    Query(q): Query<PaginationQuery>,
    crate::axum::middleware::accept::WantsJson(wants_json): crate::axum::middleware::accept::WantsJson,
) -> Result<axum::response::Response, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    use axum::response::IntoResponse;
    use crate::axum::templates::FlagsPageTemplate;
    let page = Page::new(q.page);
    let result = svc.list_pending_flags(page).await.map_err(ApiError::from)?;

    // Content negotiation: JSON for API clients, HTML for browser requests.
    if wants_json {
        Ok(Json(result).into_response())
    } else {
        Ok(FlagsPageTemplate {
            total_pages: result.total_pages() as u32,
            flags: result.items,
            page: q.page,
        }.into_response())
    }
}

/// `POST /mod/flags/:id/resolve` — approve or reject a flag.
pub async fn resolve_flag<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ResolveFlagRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.resolve_flag(FlagId(id), req.resolution.into(), current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mod/posts/:id/delete` — delete a post and record audit entry.
pub async fn delete_post<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.delete_post(domains::models::PostId(id), current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mod/threads/:id/delete` — delete a thread and all its posts.
pub async fn delete_thread<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.delete_thread(domains::models::ThreadId(id), current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mod/threads/:id/sticky` — set or clear sticky status.
///
/// Body: `{ "value": true }` to sticky, `{ "value": false }` to un-sticky.
pub async fn toggle_sticky<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetBoolRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.set_sticky(domains::models::ThreadId(id), req.value, current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mod/threads/:id/close` — set or clear closed status.
///
/// Body: `{ "value": true }` to close, `{ "value": false }` to re-open.
pub async fn toggle_closed<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetBoolRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.set_closed(domains::models::ThreadId(id), req.value, current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mod/threads/:id/delete-by-ip` — delete all posts by an IP in this thread.
///
/// Body: `{ "ip_hash": "<hex>" }`.
/// Returns `{ "deleted": N }` with the count of removed posts.
pub async fn delete_posts_by_ip<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
    Json(req): Json<DeleteByIpRequest>,
) -> Result<impl IntoResponse, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    let count = svc
        .delete_posts_by_ip_in_thread(IpHash::new(req.ip_hash), ThreadId(id), current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "deleted": count })))
}

/// `POST /mod/threads/:id/cycle` — toggle cycle mode on a thread.
pub async fn toggle_cycle<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetBoolRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.set_cycle(domains::models::ThreadId(id), req.value, current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mod/posts/:id/pin` — pin or unpin a post in a cycle thread.
pub async fn set_post_pinned<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetBoolRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.set_pinned(domains::models::PostId(id), req.value, current.user_id())
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}


/// `POST /mod/bans` — issue an IP ban.
pub async fn create_ban<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Json(req): Json<CreateBanRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.ban_ip(
        IpHash::new(req.ip_hash),
        req.reason,
        req.expires_at,
        current.user_id(),
    )
    .await
    .map_err(ApiError::from)?;
    Ok(StatusCode::CREATED)
}

/// `POST /mod/bans/:id/expire` — immediately expire a ban.
pub async fn expire_ban<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    ModeratorUser(current): ModeratorUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    svc.expire_ban(BanId(id), current.user_id()).await.map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /mod/bans` — list all bans (active + expired), paginated.
///
/// Returns HTML by default; responds with JSON when `Accept: application/json`
/// is set (used by the dashboard ban-count widget).
pub async fn list_bans<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    _mod_user: ModeratorUser,
    crate::axum::middleware::accept::WantsJson(wants_json): crate::axum::middleware::accept::WantsJson,
    Query(q): Query<PaginationQuery>,
) -> Result<axum::response::Response, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    use axum::response::IntoResponse;
    use crate::axum::templates::BansPageTemplate;
    use crate::common::pagination::PageResponse;
    let page = Page::new(q.page);
    let result = svc.list_bans(page).await.map_err(ApiError::from)?;
    if wants_json {
        let resp: PageResponse<domains::models::Ban> = result.into();
        Ok(axum::Json(resp).into_response())
    } else {
        Ok(BansPageTemplate {
            total_pages: result.total_pages() as u32,
            bans: result.items,
            page: q.page,
        }.into_response())
    }
}

/// Request body for `POST /board/:slug/thread/:id/flag`.
#[derive(Debug, serde::Deserialize)]
pub struct CreateFlagRequest {
    /// Human-readable description of the rule violation.
    pub reason: String,
}

/// `POST /board/:slug/thread/:id/flag` — report a post to the moderation queue.
///
/// No auth required — any visitor can report a post.
/// The reporter's IP is hashed immediately with a daily salt; raw IPs are never stored.
pub async fn create_flag<BR, PR, TR, FR, AR, UR>(
    State(svc): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    axum::extract::ConnectInfo(peer_addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Path((_slug, thread_id)): Path<(String, Uuid)>,
    Json(req): Json<CreateFlagRequest>,
) -> Result<StatusCode, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    // Hash reporter IP with daily salt — same approach as create_post
    let raw_ip = peer_addr.ip().to_string();
    let daily_salt = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let ip_hash = services::common::utils::hash_ip(&raw_ip, &daily_salt);

    // Resolve thread → OP post_id so the FK in the flags table is satisfied.
    // The flag route uses the thread UUID; flags reference posts (the FK target).
    let tid = domains::models::ThreadId(thread_id);
    let thread = svc.get_thread(tid).await.map_err(ApiError::from)?;
    let op_post_id = thread.op_post_id.ok_or_else(|| {
        ApiError::NotFound("thread has no opening post".to_owned())
    })?;

    svc.file_flag(op_post_id, req.reason, ip_hash)
        .await
        .map_err(ApiError::from)?;

    Ok(StatusCode::CREATED)
}

/// Combined state for the janitor and volunteer dashboard handlers.
///
/// Both dashboards need the moderation service (audit log) and the board service
/// (board list). `Clone` is implemented manually — `Arc` fields are always cloneable.
pub struct ModerationDashboardState<BR, PR, TR, FR, AR, UR, BS>
where
    BR: domains::ports::BanRepository + 'static,
    PR: domains::ports::PostRepository + 'static,
    TR: domains::ports::ThreadRepository + 'static,
    FR: domains::ports::FlagRepository + 'static,
    AR: domains::ports::AuditRepository + 'static,
    UR: domains::ports::UserRepository + 'static,
    BS: BoardRepo + 'static,
{
    /// Moderation service — provides audit log entries.
    pub mod_svc:   Arc<ModerationService<BR, PR, TR, FR, AR, UR>>,
    /// Board service — resolves board lists for janitor/volunteer views.
    pub board_svc: Arc<BS>,
}

impl<BR, PR, TR, FR, AR, UR, BS> Clone for ModerationDashboardState<BR, PR, TR, FR, AR, UR, BS>
where
    BR: domains::ports::BanRepository + 'static,
    PR: domains::ports::PostRepository + 'static,
    TR: domains::ports::ThreadRepository + 'static,
    FR: domains::ports::FlagRepository + 'static,
    AR: domains::ports::AuditRepository + 'static,
    UR: domains::ports::UserRepository + 'static,
    BS: BoardRepo + 'static,
{
    fn clone(&self) -> Self {
        Self {
            mod_svc:   Arc::clone(&self.mod_svc),
            board_svc: Arc::clone(&self.board_svc),
        }
    }
}

/// `GET /janitor/dashboard` — global janitor dashboard.
pub async fn janitor_dashboard<BR, PR, TR, FR, AR, UR, BS>(
    State(s): State<ModerationDashboardState<BR, PR, TR, FR, AR, UR, BS>>,
    crate::axum::middleware::auth::JanitorStaffUser(current): crate::axum::middleware::auth::JanitorStaffUser,
) -> axum::response::Response
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
    BS: BoardRepo,
{
    use axum::response::IntoResponse;
    use crate::axum::templates::{DashboardBoard, DashboardTemplate};
    use domains::models::Page;

    let recent_logs: Vec<domains::models::AuditEntry> =
        s.mod_svc.recent_audit_entries(10).await.unwrap_or_default();

    // All boards — janitor sees site-wide, no manage controls
    let all_boards: Vec<domains::models::Board> =
        s.board_svc.list_boards(Page::new(1)).await
            .map(|p| p.items)
            .unwrap_or_default();
    let boards = all_boards.into_iter().map(|b| DashboardBoard {
        board: b,
        can_manage: false,
        can_manage_volunteers: false,
    }).collect();

    DashboardTemplate {
        role_display:     current.role_display(),
        announcements:    vec![],
        boards,
        staff:            None,
        recent_logs,
        recent_posts:     vec![],
        messages:         vec![],
        unread_count:     0,
        pending_requests: None,
    }.into_response()
}

/// `GET /volunteer/dashboard` — board volunteer dashboard.
pub async fn volunteer_dashboard<BR, PR, TR, FR, AR, UR, BS>(
    State(s): State<ModerationDashboardState<BR, PR, TR, FR, AR, UR, BS>>,
    AuthenticatedUser(current): AuthenticatedUser,
) -> axum::response::Response
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
    BS: BoardRepo,
{
    use axum::response::IntoResponse;
    use crate::axum::templates::{DashboardBoard, DashboardTemplate};

    let recent_logs: Vec<domains::models::AuditEntry> =
        s.mod_svc.recent_audit_entries(10).await.unwrap_or_default();
    let mut boards = Vec::new();
    for board_id in &current.volunteer_boards {
        if let Ok(board) = s.board_svc.get_by_id(*board_id).await {
            boards.push(DashboardBoard {
                board,
                can_manage: false,
                can_manage_volunteers: false,
            });
        }
    }

    DashboardTemplate {
        role_display:     current.role_display(),
        announcements:    vec![],
        boards,
        staff:            None,
        recent_logs,
        recent_posts:     vec![],
        messages:         vec![],
        unread_count:     0,
        pending_requests: None,
    }.into_response()
}

/// `GET /mod/dashboard` — redirect to the caller's own dashboard based on role.
pub async fn mod_dashboard_redirect(
    AnyAuthenticatedUser(current): AnyAuthenticatedUser,
) -> axum::response::Response {
    use domains::models::Role;
    let url = match current.role {
        Role::Admin          => "/admin/dashboard",
        Role::Janitor        => "/janitor/dashboard",
        Role::BoardOwner     => "/board-owner/dashboard",
        Role::BoardVolunteer => "/volunteer/dashboard",
        Role::User           => "/user/dashboard",
    };
    axum::response::Redirect::to(url).into_response()
}

// ─── Audit Log Pages ──────────────────────────────────────────────────────────

/// `GET /janitor/logs` — full paginated audit log, site-wide.
///
/// Accessible to Janitor and Admin roles. Shows all moderation actions
/// in reverse chronological order with optional pagination.
pub async fn janitor_audit_log<BR, PR, TR, FR, AR, UR>(
    State(s): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    crate::axum::middleware::auth::JanitorStaffUser(current): crate::axum::middleware::auth::JanitorStaffUser,
    Query(q): Query<crate::common::dtos::PaginationQuery>,
) -> Result<axum::response::Response, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    let page    = domains::models::Page(q.page);
    let entries = s.audit_log_all(page).await
        .map_err(ApiError::from)?;

    Ok(crate::axum::templates::AuditLogTemplate {
        role_label:   "Janitor".into(),
        entries:      crate::common::pagination::PageResponse::from(entries),
        current_user: current,
    }.into_response())
}

/// `GET /board-owner/logs` — audit log for all boards owned by the current user.
///
/// Iterates over `current.owned_boards` and returns a merged view of the most
/// relevant board's log. For simplicity, shows the log for the first owned board;
/// a board selector can be added in a future UI iteration.
pub async fn board_owner_audit_log<BR, PR, TR, FR, AR, UR>(
    State(s): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    crate::axum::middleware::auth::BoardOwnerUser(current): crate::axum::middleware::auth::BoardOwnerUser,
    Query(q): Query<crate::common::dtos::PaginationQuery>,
) -> Result<axum::response::Response, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    let page = domains::models::Page(q.page);
    // Use the first owned board; if none, return an empty log page.
    let entries = if let Some(board_id) = current.owned_boards.first().copied() {
        s.audit_log_for_board(board_id, page).await
            .map_err(ApiError::from)?
    } else {
        domains::models::Paginated::empty(page, Page::DEFAULT_PAGE_SIZE)
    };

    Ok(crate::axum::templates::AuditLogTemplate {
        role_label:   "Board Owner".into(),
        entries:      crate::common::pagination::PageResponse::from(entries),
        current_user: current,
    }.into_response())
}

/// `GET /volunteer/logs` — audit log for all boards where the current user is a volunteer.
pub async fn volunteer_audit_log<BR, PR, TR, FR, AR, UR>(
    State(s): State<Arc<ModerationService<BR, PR, TR, FR, AR, UR>>>,
    crate::axum::middleware::auth::VolunteerUser(current): crate::axum::middleware::auth::VolunteerUser,
    Query(q): Query<crate::common::dtos::PaginationQuery>,
) -> Result<axum::response::Response, ApiError>
where
    BR: domains::ports::BanRepository,
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    FR: domains::ports::FlagRepository,
    AR: domains::ports::AuditRepository,
    UR: domains::ports::UserRepository,
{
    let page = domains::models::Page(q.page);
    let entries = if let Some(board_id) = current.volunteer_boards.first().copied() {
        s.audit_log_for_board(board_id, page).await
            .map_err(ApiError::from)?
    } else {
        domains::models::Paginated::empty(page, Page::DEFAULT_PAGE_SIZE)
    };

    Ok(crate::axum::templates::AuditLogTemplate {
        role_label:   "Volunteer".into(),
        entries:      crate::common::pagination::PageResponse::from(entries),
        current_user: current,
    }.into_response())
}
