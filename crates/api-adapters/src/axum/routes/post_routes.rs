//! Post routes: create post/thread.

use axum::{routing::post, Router};
use std::sync::Arc;

use services::post::PostService;

use crate::axum::handlers::post_handlers;

/// Post creation route.
///
/// `POST /board/{slug}/post` — create a post or new thread (multipart)
///
/// The board-config middleware must inject `ExtractedBoardConfig` before these handlers run.
pub fn post_routes<PR, TR, BR, MS, RL, MP>(
    post_service: Arc<PostService<PR, TR, BR, MS, RL, MP>>,
) -> Router
where
    PR: domains::ports::PostRepository + 'static,
    TR: domains::ports::ThreadRepository + 'static,
    BR: domains::ports::BanRepository + 'static,
    MS: domains::ports::MediaStorage + 'static,
    RL: domains::ports::RateLimiter + 'static,
    MP: domains::ports::MediaProcessor + 'static,
{
    Router::new()
        .route(
            "/board/{slug}/post",
            post(post_handlers::create_post::<PR, TR, BR, MS, RL, MP>),
        )
        .with_state(post_service)
}
