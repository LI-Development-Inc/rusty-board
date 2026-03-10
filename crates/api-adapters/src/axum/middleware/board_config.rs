//! Board config loader middleware.
//!
//! For any request with a `:slug` path segment, loads and caches the `BoardConfig`
//! for that board and inserts it into request extensions. Handlers can then access
//! the config without a separate DB round-trip.
//!
//! # Usage
//!
//! Wrap board-scoped route groups that include a `:slug` segment:
//!
//! ```rust,ignore
//! thread_routes(thread_svc.clone())
//!     .route_layer(middleware::from_fn_with_state(
//!         BoardConfigState { source: board_source.clone(), cache: cache.clone() },
//!         board_config_middleware,
//!     ))
//! ```
//!
//! Handlers then extract the config via:
//!
//! ```rust,ignore
//! Extension(board_ctx): Extension<ExtractedBoardConfig>
//! ```

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use domains::models::{Board, BoardConfig, BoardId, Slug};
use storage_adapters::cache::BoardConfigCache;

/// The board config + board ID injected into request extensions by the middleware.
///
/// Handlers access this via `Extension<ExtractedBoardConfig>`.
#[derive(Clone)]
pub struct ExtractedBoardConfig {
    /// Resolved UUID of the board matching the `:slug` path parameter.
    pub board_id: BoardId,
    /// Per-board behavioural configuration, loaded from DB and cached.
    pub config:   BoardConfig,
    /// Board slug (the URL segment, e.g. `"g"` from `/board/g/`).
    pub slug:     Slug,
    /// Full board metadata (title, rules, etc.) for template rendering.
    pub board:    Board,
}

/// Shared state injected into the middleware via `from_fn_with_state`.
#[derive(Clone)]
pub struct BoardConfigState {
    /// Source of truth for board slug → ID + config lookups (type-erased).
    pub source: Arc<dyn BoardConfigSource>,
    /// In-process TTL cache — avoids a DB round-trip on every request.
    pub cache:  Arc<BoardConfigCache>,
}

/// Minimal trait the middleware needs from the board repository.
///
/// Implemented as a blanket over `services::board::BoardRepo` in the composition root.
/// Kept minimal here to avoid pulling the full `BoardRepo` trait into middleware code.
#[async_trait::async_trait]
pub trait BoardConfigSource: Send + Sync + 'static {
    /// Fetch board, board ID and its config by slug. Returns `None` if the board doesn't exist.
    async fn config_by_slug(
        &self,
        slug: &Slug,
    ) -> Result<Option<(Board, BoardId, BoardConfig)>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Blanket implementation of `BoardConfigSource` for any type that implements
/// `services::board::BoardRepo`.
///
/// This keeps `BoardConfigSource` isolated in the middleware layer while allowing
/// the composition root to pass any `BoardRepo` implementation as a `BoardConfigSource`.
#[async_trait::async_trait]
impl<T: services::board::BoardRepo> BoardConfigSource for T {
    async fn config_by_slug(
        &self,
        slug: &Slug,
    ) -> Result<Option<(Board, BoardId, BoardConfig)>, Box<dyn std::error::Error + Send + Sync>> {
        use services::board::BoardError;

        // First look up the board to get its ID
        let board = match self.get_by_slug(slug.as_str()).await {
            Ok(b) => b,
            Err(BoardError::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(Box::new(e)),
        };

        // Then fetch its config
        let config = self.get_config(board.id).await.map_err(Box::new)?;
        let board_id = board.id;
        Ok(Some((board, board_id, config)))
    }
}

/// Axum middleware that resolves a board slug to `(BoardId, BoardConfig)` and inserts
/// an `ExtractedBoardConfig` into request extensions.
///
/// Returns:
/// - `404 Not Found`            — no board matches the slug
/// - `503 Service Unavailable`  — repository call failed
pub async fn board_config_middleware(
    State(state): State<BoardConfigState>,
    mut req: Request,
    next: Next,
) -> Response {
    // Parse the slug from the URI path.
    // Expected path structure: `/board/<slug>[/...]`
    //
    // Guard: only run for paths that begin with exactly "/board/" — not "/board-owner/"
    // or any other prefix that shares the "board" segment.
    let path = req.uri().path();
    if !path.starts_with("/board/") {
        return next.run(req).await;
    }

    let slug_str: Option<String> = {
        let mut parts = path.splitn(4, '/').filter(|s| !s.is_empty());
        parts.next(); // skip "board"
        parts.next().map(str::to_owned)
    };

    let slug_str = match slug_str {
        Some(s) => s,
        None => return next.run(req).await, // no slug → pass through
    };

    let slug = match Slug::new(slug_str.clone()) {
        Ok(s)  => s,
        Err(_) => return (StatusCode::NOT_FOUND, "invalid board slug").into_response(),
    };

    // Try in-process cache first
    if let Some((board, board_id, config)) = state.cache.get_by_slug(&slug) {
        req.extensions_mut().insert(ExtractedBoardConfig { board_id, config, slug: slug.clone(), board });
        return next.run(req).await;
    }

    // Cache miss → hit the repository
    match state.source.config_by_slug(&slug).await {
        Ok(Some((board, board_id, config))) => {
            state.cache.set_by_slug(slug.clone(), board.clone(), board_id, config.clone());
            req.extensions_mut().insert(ExtractedBoardConfig { board_id, config, slug, board });
            next.run(req).await
        }
        Ok(None) => {
            (StatusCode::NOT_FOUND, format!("board '{}' not found", slug_str)).into_response()
        }
        Err(e) => {
            tracing::error!(slug = %slug_str, error = %e, "board_config_middleware: repository error");
            (StatusCode::SERVICE_UNAVAILABLE, "failed to load board configuration").into_response()
        }
    }
}
