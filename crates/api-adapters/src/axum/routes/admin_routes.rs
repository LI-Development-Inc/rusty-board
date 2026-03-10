//! Admin routes: user CRUD, board owner assignment, dashboard.

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

/// Admin routes — require `Admin` role.
pub fn admin_routes<UR, AP, BR, RR>(
    user_service:    Arc<UserService<UR, AP>>,
    board_service:   Arc<BR>,
    request_service: Arc<StaffRequestService<RR, UR>>,
) -> Router
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    BR: BoardRepo + 'static,
    RR: StaffRequestRepository + 'static,
{
    // Dashboard gets a combined state (needs user list, board list, and request queue).
    let dashboard_state = AdminDashboardState {
        user_svc:    user_service.clone(),
        board_svc:   board_service,
        request_svc: request_service,
    };

    Router::new()
        // Dashboard (combined state)
        .route(
            "/admin/dashboard",
            get(admin_handlers::admin_dashboard::<UR, AP, BR, RR>)
                .with_state(dashboard_state),
        )
        // User management (user service only)
        .route(
            "/admin/users",
            get(admin_handlers::list_users::<UR, AP>)
                .post(admin_handlers::create_user::<UR, AP>),
        )
        .route(
            "/admin/users/{id}/deactivate",
            post(admin_handlers::deactivate_user::<UR, AP>),
        )
        // Board owner management
        .route(
            "/admin/boards/{id}/owners",
            post(admin_handlers::add_board_owner::<UR, AP>),
        )
        .route(
            "/admin/boards/{id}/owners/{user_id}",
            delete(admin_handlers::remove_board_owner::<UR, AP>),
        )
        // Audit log
        .route("/admin/audit", get(admin_handlers::list_audit_log::<UR, AP>))
        .with_state(user_service)
}
