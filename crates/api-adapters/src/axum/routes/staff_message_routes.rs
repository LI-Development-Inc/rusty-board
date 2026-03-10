//! Staff messaging routes — all require at least `StaffUser` role.
//!
//! Routes:
//! - `GET  /staff/messages`         — paginated inbox
//! - `POST /staff/messages`         — send a new message
//! - `POST /staff/messages/:id/read` — mark message as read
//! - `POST /staff/messages/purge`   — admin-only: delete messages older than 14 days

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::axum::handlers::staff_message_handlers::{
    self, StaffMessageState,
};
use services::staff_message::StaffMessageService;

/// Mount all staff message routes under a shared `StaffMessageState`.
pub fn staff_message_routes<MR>(
    svc: Arc<StaffMessageService<MR>>,
) -> Router
where
    MR: domains::ports::StaffMessageRepository + 'static,
{
    let state = StaffMessageState { svc };

    Router::new()
        .route("/staff/messages",           get(staff_message_handlers::inbox::<MR>)
                                               .post(staff_message_handlers::send_message::<MR>))
        .route("/staff/messages/new",        get(staff_message_handlers::compose_page))
        .route("/staff/messages/{id}/read", post(staff_message_handlers::mark_read::<MR>))
        .route("/staff/messages/purge",     post(staff_message_handlers::purge_expired::<MR>))
        .with_state(state)
}
