//! Board owner handlers: dashboard for users who own a board.
//!
//! These are separate from `board_handlers` because they render HTML templates
//! rather than returning JSON, and require board-ownership authorisation.
//!
//! Routes (all board-scoped, run through the board-config middleware):
//!   `GET  /board/:slug/dashboard` — board owner dashboard (HTML)
//!
//! The board-config middleware guarantees that `ExtractedBoardConfig` is present
//! in extensions before these handlers run.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::axum::middleware::{
    auth::AuthenticatedUser,
    board_config::ExtractedBoardConfig,
};
use crate::axum::templates::BoardOwnerDashboardTemplate;
use domains::ports::StaffRequestRepository;
use services::board::BoardRepo;
use services::staff_request::StaffRequestService;

/// `GET /board/:slug/dashboard` — render the board-owner dashboard.
///
/// Requires the authenticated user to own the board or be an admin/moderator.
/// Returns `403 Forbidden` if the user lacks the required permission.
pub async fn board_owner_dashboard<BR: BoardRepo>(
    State(_board_svc): State<Arc<BR>>,
    AuthenticatedUser(current): AuthenticatedUser,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
) -> Response {
    // Permission check: only board owners and admins may view this page
    if !current.can_manage_board_config(board_ctx.board_id) {
        return (StatusCode::FORBIDDEN, "board ownership required").into_response();
    }

    BoardOwnerDashboardTemplate {
        board:  board_ctx.board,
        config: board_ctx.config,
    }.into_response()
}

/// Combined state for the board-owner top-level dashboard.
///
/// Needs both the board service (load owned boards) and the staff request service
/// (load pending volunteer requests for owned boards).
pub struct BoardOwnerDashboardState<BR, UR, RR>
where
    BR: BoardRepo + 'static,
    UR: domains::ports::UserRepository + 'static,
    RR: StaffRequestRepository + 'static,
{
    /// Board service — resolves owned board IDs to `Board` records.
    pub board_svc:   Arc<BR>,
    /// Staff request service — loads pending volunteer requests for owned boards.
    pub request_svc: Arc<StaffRequestService<RR, UR>>,
}

impl<BR, UR, RR> BoardOwnerDashboardState<BR, UR, RR>
where
    BR: BoardRepo + 'static,
    UR: domains::ports::UserRepository + 'static,
    RR: StaffRequestRepository + 'static,
{
    /// Construct a `BoardOwnerDashboardState` from both required services.
    pub fn new(board_svc: Arc<BR>, request_svc: Arc<StaffRequestService<RR, UR>>) -> Self {
        Self { board_svc, request_svc }
    }
}

impl<BR, UR, RR> Clone for BoardOwnerDashboardState<BR, UR, RR>
where
    BR: BoardRepo + 'static,
    UR: domains::ports::UserRepository + 'static,
    RR: StaffRequestRepository + 'static,
{
    fn clone(&self) -> Self {
        Self {
            board_svc:   Arc::clone(&self.board_svc),
            request_svc: Arc::clone(&self.request_svc),
        }
    }
}

/// `GET /board-owner/dashboard` — top-level dashboard listing all boards owned by the caller.
///
/// Resolves `current_user.owned_boards` into `Board` records and renders them
/// in a summary table. Does not require a board slug in the URL.
pub async fn board_owner_top_dashboard<BR, UR, RR>(
    State(s): State<BoardOwnerDashboardState<BR, UR, RR>>,
    AuthenticatedUser(current): AuthenticatedUser,
) -> Response
where
    BR: BoardRepo,
    UR: domains::ports::UserRepository,
    RR: StaffRequestRepository,
{
    use domains::models::Role;
    if !matches!(current.role, Role::BoardOwner | Role::Admin) {
        return (axum::http::StatusCode::FORBIDDEN, "board owner role required").into_response();
    }

    use crate::axum::templates::{DashboardBoard, DashboardTemplate};

    // Resolve each owned board ID to a Board record. Errors are soft-ignored.
    let mut boards = Vec::new();
    for board_id in &current.owned_boards {
        if let Ok(board) = s.board_svc.get_by_id(*board_id).await {
            boards.push(DashboardBoard {
                board,
                can_manage: true,
                can_manage_volunteers: true,
            });
        }
    }

    // Collect pending volunteer requests across all owned boards.
    let mut pending: Vec<domains::models::StaffRequest> = Vec::new();
    for board_id in &current.owned_boards {
        if let Ok(board) = s.board_svc.get_by_id(*board_id).await {
            let mut reqs = s.request_svc
                .list_pending_for_board(&board.slug)
                .await
                .unwrap_or_default();
            pending.append(&mut reqs);
        }
    }

    DashboardTemplate {
        role_display:     current.role_display(),
        announcements:    vec![],
        boards,
        staff:            Some(vec![]), // TODO v1.1: load volunteers via UserRepository
        recent_logs:      vec![],
        recent_posts:     vec![],
        messages:         vec![],
        unread_count:     0,
        pending_requests: Some(pending),
    }.into_response()
}
