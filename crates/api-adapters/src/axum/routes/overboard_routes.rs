//! Overboard route: recent posts across all boards.

use axum::{routing::get, Router};
use std::sync::Arc;

use crate::axum::handlers::overboard_handlers::{
    OverboardPostSource, OverboardState, show_overboard,
};
use services::board::BoardRepo;

/// `GET /overboard` — recent posts across all boards.
pub fn overboard_routes<BR, PR>(board_service: Arc<BR>, post_service: Arc<PR>) -> Router
where
    BR: BoardRepo + 'static,
    PR: OverboardPostSource + 'static,
{
    let state = OverboardState { board_service, post_service };
    Router::new()
        .route("/overboard", get(show_overboard::<BR, PR>))
        .with_state(state)
}
