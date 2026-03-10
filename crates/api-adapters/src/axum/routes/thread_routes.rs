//! Thread routes: board view, catalog, thread — nested under `/board/{slug}`.

use axum::{routing::get, Router};
use std::sync::Arc;

use services::thread::ThreadRepo;

use crate::axum::handlers::thread_handlers;

/// Thread routes nested under a board slug (spec-compliant paths).
///
/// Handlers render HTML pages (Askama templates). The board + config data is
/// already present in `ExtractedBoardConfig` from the preceding middleware.
///
/// The board-config middleware must run before these handlers to inject
/// `ExtractedBoardConfig` (which includes the resolved `BoardId` and `Board`).
pub fn thread_routes<TR: ThreadRepo>(thread_service: Arc<TR>) -> Router {
    Router::new()
        .route("/board/{slug}",          get(thread_handlers::show_board_html::<TR>))
        .route("/board/{slug}/catalog",  get(thread_handlers::show_catalog_html::<TR>))
        .route("/board/{slug}/thread/{id}", get(thread_handlers::show_thread_html::<TR>))
        .with_state(thread_service)
}
