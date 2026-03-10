//! Board owner routes: config management by board owners.
//!
//! Per spec: `GET /board/{slug}/config`, `PUT /board/{slug}/config`.
//! The board-config middleware injects the `ExtractedBoardConfig` which includes the
//! board_id for any routes that need it.

use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;

use domains::ports::{StaffRequestRepository, UserRepository};
use services::board::BoardRepo;
use services::staff_request::StaffRequestService;

use crate::axum::handlers::{
    board_handlers, board_owner_handlers::{self, BoardOwnerDashboardState},
    volunteer_handlers,
};

/// Board owner routes.
///
/// `GET  /board-owner/dashboard`  — top-level dashboard (all owned boards + pending requests)
/// `GET  /board/{slug}/config`    — get board config (JSON)
/// `PUT  /board/{slug}/config`    — update board config
/// `GET  /board/{slug}/dashboard` — per-board owner dashboard (HTML)
pub fn board_owner_routes<BR, UR, RR>(
    board_service:   Arc<BR>,
    request_service: Arc<StaffRequestService<RR, UR>>,
) -> Router
where
    BR: BoardRepo + 'static,
    UR: UserRepository + 'static,
    RR: StaffRequestRepository + 'static,
{
    // Top-level dashboard needs both board_svc and request_svc.
    let dashboard_state = BoardOwnerDashboardState::<BR, UR, RR>::new(
        board_service.clone(),
        request_service,
    );

    Router::new()
        .route(
            "/board-owner/dashboard",
            get(board_owner_handlers::board_owner_top_dashboard::<BR, UR, RR>)
                .with_state(dashboard_state),
        )
        .route(
            "/board/{slug}/config",
            get(board_handlers::get_board_config::<BR>)
                .put(board_handlers::update_board_config::<BR>),
        )
        .route(
            "/board/{slug}/dashboard",
            get(board_owner_handlers::board_owner_dashboard::<BR>),
        )
        .route(
            "/board/{slug}/volunteers",
            get(volunteer_handlers::list_volunteers::<BR>)
                .post(volunteer_handlers::add_volunteer::<BR>),
        )
        .route(
            "/board/{slug}/volunteers/{user_id}",
            axum::routing::delete(volunteer_handlers::remove_volunteer::<BR>),
        )
        .with_state(board_service)
}
