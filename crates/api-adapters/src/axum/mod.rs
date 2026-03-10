//! Axum HTTP transport layer (`web-axum` feature).
//!
//! The single entry point is `build_router()` which constructs the complete
//! `axum::Router` with all routes, middleware layers, and injected services.
//! Called once in `composition.rs`.

pub mod error;
pub mod handlers;
pub mod health;
pub mod metrics;
pub mod middleware;
pub mod routes;
pub mod templates;

use std::sync::Arc;

pub use error::into_response_ext::IntoApiResponse;

/// Application state injected into all Axum handlers.
///
/// Wraps all services in `Arc` for cheap cloning across requests.
/// Services are generic over their port types, so `AppState` uses trait objects
/// only where necessary (the concrete types are monomorphized in `composition.rs`
/// and the `Arc<AppState<...>>` carries concrete type parameters).
#[derive(Clone)]
pub struct AppState<BS, PS, TS, MS, US>
where
    BS: Send + Sync + 'static,
    PS: Send + Sync + 'static,
    TS: Send + Sync + 'static,
    MS: Send + Sync + 'static,
    US: Send + Sync + 'static,
{
    /// Board CRUD and config service.
    pub board_service:      Arc<BS>,
    /// Post creation, media dispatch, and board-rule enforcement service.
    pub post_service:       Arc<PS>,
    /// Thread creation, sticky/close, and prune service.
    pub thread_service:     Arc<TS>,
    /// Moderation service: bans, flags, deletions, and audit log.
    pub moderation_service: Arc<MS>,
    /// User account and authentication service.
    pub user_service:       Arc<US>,
    /// In-process TTL cache for `BoardConfig` — avoids a DB round-trip per request.
    pub board_config_cache: Arc<storage_adapters::cache::BoardConfigCache>,
    /// JWT authentication provider used by auth middleware and token refresh.
    pub auth_provider:      Arc<dyn domains::ports::AuthProvider>,
}

impl<BS, PS, TS, MS, US> AppState<BS, PS, TS, MS, US>
where
    BS: Send + Sync + 'static,
    PS: Send + Sync + 'static,
    TS: Send + Sync + 'static,
    MS: Send + Sync + 'static,
    US: Send + Sync + 'static,
{
    /// Construct a new `AppState`.
    pub fn new(
        board_service:      BS,
        post_service:       PS,
        thread_service:     TS,
        moderation_service: MS,
        user_service:       US,
        board_config_cache: storage_adapters::cache::BoardConfigCache,
        auth_provider:      Arc<dyn domains::ports::AuthProvider>,
    ) -> Self {
        Self {
            board_service:      Arc::new(board_service),
            post_service:       Arc::new(post_service),
            thread_service:     Arc::new(thread_service),
            moderation_service: Arc::new(moderation_service),
            user_service:       Arc::new(user_service),
            board_config_cache: Arc::new(board_config_cache),
            auth_provider,
        }
    }
}
