//! Admin routes: user CRUD, board owner assignment, dashboard, board creation, announcements.

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;

use domains::ports::{AuthProvider, StaffRequestRepository};
use services::board::BoardRepo;
use services::staff_request::StaffRequestService;
use services::user::UserService;

use crate::axum::handlers::admin_handlers::{self, AdminDashboardState};

/// Build and return all admin routes.
///
/// Routes are split across three sub-routers by state type, then merged:
///
/// - **Dashboard state** (`AdminDashboardState`) — dashboard view, board creation, announcements
/// - **Request service state** — approve / deny staff requests
/// - **User service state** — user CRUD, board-owner assignment, audit log
///
/// The `msg_service` is injected as an `axum::Extension` so the broadcast handler
/// can reach it without adding a generic parameter to the primary state type.
pub fn admin_routes<UR, AP, BR, RR, MR>(
    user_service:    Arc<UserService<UR, AP>>,
    board_service:   Arc<BR>,
    request_service: Arc<StaffRequestService<RR, UR>>,
    msg_service:     Arc<services::staff_message::StaffMessageService<MR>>,
) -> Router
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    BR: BoardRepo + 'static,
    RR: StaffRequestRepository + 'static,
    MR: domains::ports::StaffMessageRepository + 'static,
{
    let dashboard_state = AdminDashboardState {
        user_svc:    user_service.clone(),
        board_svc:   board_service,
        request_svc: request_service.clone(),
    };

    // ── Routes that use AdminDashboardState ───────────────────────────────────
    let dashboard_router: Router = Router::new()
        .route("/admin/dashboard", get(admin_handlers::admin_dashboard::<UR, AP, BR, RR>))
        .route("/admin/announce",  post(admin_handlers::broadcast_announcement::<UR, AP, BR, RR, MR>))
        .layer(axum::Extension(msg_service))
        .with_state(dashboard_state);

    // ── Routes that use StaffRequestService ──────────────────────────────────
    let request_router: Router = Router::new()
        .route("/admin/requests/{id}/approve", post(admin_handlers::approve_request::<RR, UR>))
        .route("/admin/requests/{id}/deny",    post(admin_handlers::deny_request::<RR, UR>))
        .with_state(request_service);

    // ── Routes that use UserService ───────────────────────────────────────────
    let user_router: Router = Router::new()
        .route(
            "/admin/users",
            get(admin_handlers::list_users::<UR, AP>)
                .post(admin_handlers::create_user::<UR, AP>),
        )
        .route("/admin/users/{id}/deactivate",          post(admin_handlers::deactivate_user::<UR, AP>))
        .route("/admin/boards/{id}/owners",             post(admin_handlers::add_board_owner::<UR, AP>))
        .route("/admin/boards/{id}/owners/{user_id}",   delete(admin_handlers::remove_board_owner::<UR, AP>))
        .route("/admin/audit",                          get(admin_handlers::list_audit_log::<UR, AP>))
        .with_state(user_service);

    Router::new()
        .merge(dashboard_router)
        .merge(request_router)
        .merge(user_router)
}
