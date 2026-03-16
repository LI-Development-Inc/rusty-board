//! Board routes: public read + admin CRUD for boards and their configs.

use axum::{
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

use services::board::BoardRepo;
use domains::ports::PostRepository;

use crate::axum::handlers::board_handlers;
use crate::axum::handlers::board_handlers::{ArchiveState, SearchState};

/// Public board routes — no auth required.
pub fn board_public_routes<BR, PR, AR>(
    board_service: Arc<BR>,
    post_repo: PR,
    archive_repo: Arc<AR>,
) -> Router
where
    BR: BoardRepo,
    PR: PostRepository + Clone,
    AR: domains::ports::ArchiveRepository + Clone,
{
    let search_state = SearchState {
        board_svc: Arc::clone(&board_service),
        post_repo,
    };
    let archive_state = ArchiveState {
        board_svc:    Arc::clone(&board_service),
        archive_repo,
    };

    Router::new()
        .route("/boards", get(board_handlers::list_boards::<BR>))
        .route("/boards/{slug}", get(board_handlers::show_board::<BR>))
        .with_state(board_service)
        .merge(
            Router::new()
                .route("/boards/{slug}/search", get(board_handlers::search_board::<BR, PR>))
                .with_state(search_state),
        )
        .merge(
            Router::new()
                .route("/board/{slug}/archive", get(board_handlers::show_archive::<BR, AR>))
                .with_state(archive_state),
        )
}

/// Admin board management routes — require `Admin` role.
pub fn board_admin_routes<BR: BoardRepo>(board_service: Arc<BR>) -> Router {
    Router::new()
        .route("/admin/boards", post(board_handlers::create_board::<BR>))
        .route(
            "/admin/boards/{id}",
            put(board_handlers::update_board::<BR>)
                .delete(board_handlers::delete_board::<BR>),
        )
        .route(
            "/admin/boards/{id}/config",
            get(board_handlers::get_board_config_by_id::<BR>)
                .put(board_handlers::update_board_config_by_id::<BR>),
        )
        .with_state(board_service)
}
