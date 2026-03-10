//! User account routes.
//!
//! `GET  /user/dashboard` — personal dashboard for any authenticated user.
//! `POST /user/requests`  — submit a staff escalation request.

use axum::{routing::{get, post}, Router};
use std::sync::Arc;

use domains::ports::{AuthProvider, StaffRequestRepository};
use services::staff_request::StaffRequestService;
use services::user::UserService;

use crate::axum::handlers::user_handlers::{
    user_dashboard, submit_request, UserDashboardState,
};

/// Mount user routes with both `UserService` and `StaffRequestService`.
pub fn user_routes<UR, AP, RR>(
    user_service:    Arc<UserService<UR, AP>>,
    request_service: Arc<StaffRequestService<RR, UR>>,
) -> Router
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    RR: StaffRequestRepository + 'static,
{
    let state = UserDashboardState {
        user_svc:    user_service,
        request_svc: request_service,
    };

    Router::new()
        .route("/user/dashboard", get(user_dashboard::<UR, AP, RR>))
        .route("/user/requests",  post(submit_request::<UR, AP, RR>))
        .with_state(state)
}
