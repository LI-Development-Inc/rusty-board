//! # rb-api
//! 
//! The web routing and orchestration layer for Rusty-Board.

pub mod handlers;
pub mod middleware;

use actix_web::web;

/// Configures the routes for the imageboard.
/// 
/// # Developer Note
/// We use a scoped configuration to allow the main binary to mount 
/// the API under different paths if needed (e.g., /api/v1/).
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            // The "Board Index" (e.g., /b/)
            .route("/{board}/", web::get().to(handlers::board_index))
            // The "Thread View" (e.g., /b/thread/123)
            .route("/{board}/thread/{thread_id}", web::get().to(handlers::view_thread))
            // The Posting Endpoint
            .route("/{board}/post", web::post().to(handlers::create_post))
    );
}