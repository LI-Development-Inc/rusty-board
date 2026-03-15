//! Admin handlers: users CRUD, board owner assignment, audit log.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::common::{
    dtos::{AddBoardOwnerRequest, CreateUserRequest, PaginationQuery},
    errors::ApiError,
    pagination::PageResponse,
};
use crate::axum::middleware::auth::AdminUser;
use domains::models::{BoardId, Page, User, UserId};
use domains::ports::{AuthProvider, StaffRequestRepository};
use services::staff_request::StaffRequestService;
use services::user::UserService;

/// `GET /admin/users` — list all user accounts.
pub async fn list_users<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    _admin: AdminUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<PageResponse<User>>, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    let page = Page::new(q.page);
    let result = user_service.list_users(page).await.map_err(ApiError::from)?;
    Ok(Json(result.into()))
}

/// `POST /admin/users` — create a new moderator or admin account.
pub async fn create_user<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    _admin: AdminUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    let user = user_service
        .create_user(&req.username, &req.password, req.role.into())
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(user)))
}

/// `POST /admin/users/:id/deactivate` — deactivate a user account.
pub async fn deactivate_user<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    user_service.deactivate(UserId(id)).await.map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /admin/boards/:id/owners` — add a board owner.
pub async fn add_board_owner<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    _admin: AdminUser,
    Path(board_id): Path<Uuid>,
    Json(req): Json<AddBoardOwnerRequest>,
) -> Result<StatusCode, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    user_service
        .add_board_owner(BoardId(board_id), UserId(req.user_id))
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /admin/boards/:id/owners/:user_id` — remove a board owner.
pub async fn remove_board_owner<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    _admin: AdminUser,
    Path((board_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    user_service
        .remove_board_owner(BoardId(board_id), UserId(user_id))
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /admin/dashboard` — admin dashboard (HTML).
///
/// Shows site overview: boards, users, recent audit log.
/// In v1.0 the boards list and audit log are empty — only users are shown.
/// v1.1 will wire a composite state that includes all three services.
/// Combined state for the admin dashboard: user and board services.
///
/// `Clone` is implemented manually because the type parameters do not need to be
/// `Clone` themselves — only the `Arc` wrappers do.
pub struct AdminDashboardState<UR, AP, BR, RR>
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    BR: services::board::BoardRepo + 'static,
    RR: StaffRequestRepository + 'static,
{
    /// User service used to list staff accounts.
    pub user_svc:    Arc<UserService<UR, AP>>,
    /// Board service used to populate the board management table.
    pub board_svc:   Arc<BR>,
    /// Staff request service — populates the pending requests queue.
    pub request_svc: Arc<StaffRequestService<RR, UR>>,
}

impl<UR, AP, BR, RR> Clone for AdminDashboardState<UR, AP, BR, RR>
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    BR: services::board::BoardRepo + 'static,
    RR: StaffRequestRepository + 'static,
{
    fn clone(&self) -> Self {
        Self {
            user_svc:    Arc::clone(&self.user_svc),
            board_svc:   Arc::clone(&self.board_svc),
            request_svc: Arc::clone(&self.request_svc),
        }
    }
}

/// `GET /admin/dashboard` — render the admin dashboard (boards, users, audit log).
pub async fn admin_dashboard<UR, AP, BR, RR>(
    State(s): State<AdminDashboardState<UR, AP, BR, RR>>,
    admin: AdminUser,
) -> axum::response::Response
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
    BR: services::board::BoardRepo,
    RR: StaffRequestRepository,
{
    use axum::response::IntoResponse;
    use crate::axum::templates::{DashboardTemplate, DashboardBoard, DashboardUser};

    let all_boards = match s.board_svc.list_boards(domains::models::Page::new(1)).await {
        Ok(p) => p.items,
        Err(_) => vec![],
    };
    let all_users = match s.user_svc.list_users(domains::models::Page::new(1)).await {
        Ok(p) => p.items,
        Err(_) => vec![],
    };

    let boards = all_boards.iter().map(|b| DashboardBoard {
        board: b.clone(),
        can_manage: true,
        can_manage_volunteers: true,
    }).collect();

    let staff = Some(all_users.iter().map(|u| DashboardUser {
        user: u.clone(),
        can_message: u.id != admin.0.id,
        can_deactivate: u.id != admin.0.id,
    }).collect());

    DashboardTemplate {
        role_display:      admin.0.role_display(),
        announcements:     vec![],
        boards,
        staff,
        recent_logs:       vec![],
        recent_posts:      vec![],
        messages:          vec![],
        unread_count:      0,
        pending_requests:  Some(s.request_svc.list_pending().await.unwrap_or_default()),
    }.into_response()
}


/// `GET /admin/audit` — full audit log, paginated.
///
/// v1.0 note: returns empty list; v1.1 will wire audit repo to admin routes.
pub async fn list_audit_log<UR, AP>(
    State(_user_svc): State<Arc<UserService<UR, AP>>>,
    _admin: AdminUser,
    Query(_q): Query<PaginationQuery>,
) -> Result<Json<crate::common::pagination::PageResponse<domains::models::AuditEntry>>, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    use domains::models::{Page, Paginated};
    let empty: Paginated<domains::models::AuditEntry> =
        Paginated::new(vec![], 0, Page::new(1), 15);
    Ok(Json(empty.into()))
}

// ─── Staff Request Approve / Deny ─────────────────────────────────────────────

/// Request body for `POST /admin/requests/{id}/deny`.
#[derive(Debug, serde::Deserialize)]
pub struct DenyRequestBody {
    /// Optional reviewer note shown to the requester.
    pub note: Option<String>,
}

/// `POST /admin/requests/{id}/approve` — approve a pending staff request.
///
/// Permission is enforced by `StaffRequestService::assert_can_review`:
/// - Admin: can approve any request type.
/// - BoardOwner: can approve `become_volunteer` requests targeting their boards.
/// - All others: 403 Forbidden.
///
/// Shared by both the admin router and the board-owner router; the same route
/// path is used so the dashboard JS does not need to know the caller's role.
pub async fn approve_request<RR, UR>(
    State(request_svc): State<Arc<StaffRequestService<RR, UR>>>,
    crate::axum::middleware::auth::AnyModerationUser(reviewer): crate::axum::middleware::auth::AnyModerationUser,
    Path(id): Path<uuid::Uuid>,
) -> Result<axum::http::StatusCode, ApiError>
where
    RR: domains::ports::StaffRequestRepository,
    UR: domains::ports::UserRepository,
{
    let request_id = domains::models::StaffRequestId(id);
    request_svc
        .approve(request_id, &reviewer, None)
        .await
        .map_err(ApiError::from)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /admin/requests/{id}/deny` — deny a pending staff request.
///
/// Same permission rules as `approve_request`. Accepts an optional JSON body
/// `{ "note": "reason" }` which is stored as the review note.
pub async fn deny_request<RR, UR>(
    State(request_svc): State<Arc<StaffRequestService<RR, UR>>>,
    crate::axum::middleware::auth::AnyModerationUser(reviewer): crate::axum::middleware::auth::AnyModerationUser,
    Path(id): Path<uuid::Uuid>,
    body: Option<Json<DenyRequestBody>>,
) -> Result<axum::http::StatusCode, ApiError>
where
    RR: domains::ports::StaffRequestRepository,
    UR: domains::ports::UserRepository,
{
    let request_id = domains::models::StaffRequestId(id);
    let note = body.and_then(|b| b.note.clone());
    request_svc
        .deny(request_id, &reviewer, note)
        .await
        .map_err(ApiError::from)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
