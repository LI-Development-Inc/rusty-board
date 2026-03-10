//! Board handlers: list, show, create, update, delete, config.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::common::{
    dtos::{BoardConfigUpdate, BoardCreate, BoardUpdate, PaginationQuery},
    errors::ApiError,
    pagination::PageResponse,
};
use crate::axum::middleware::auth::{AdminUser, AuthenticatedUser};
use domains::models::Page;

/// `GET /boards` — list all boards, paginated.
pub async fn list_boards<BR>(
    State(board_service): State<Arc<BR>>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<PageResponse<domains::models::Board>>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let page = Page::new(q.page);
    let result = board_service.list_boards(page).await
        .map_err(ApiError::from)?;
    Ok(Json(result.into()))
}

/// `GET /board/:slug` — show board metadata (the handler feeds the template).
pub async fn show_board<BR>(
    State(board_service): State<Arc<BR>>,
    Path(slug): Path<String>,
) -> Result<Json<domains::models::Board>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let board = board_service.get_by_slug(&slug).await
        .map_err(ApiError::from)?;
    Ok(Json(board))
}

/// `POST /admin/boards` — create a board (admin only).
pub async fn create_board<BR>(
    State(board_service): State<Arc<BR>>,
    _admin: AdminUser,
    Json(req): Json<BoardCreate>,
) -> Result<(StatusCode, Json<domains::models::Board>), ApiError>
where
    BR: services::board::BoardRepo,
{
    let board = board_service.create_board(&req.slug, &req.title, &req.rules).await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(board)))
}

/// `PUT /admin/boards/:id` — update board title/rules (admin only).
pub async fn update_board<BR>(
    State(board_service): State<Arc<BR>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
    Json(req): Json<BoardUpdate>,
) -> Result<Json<domains::models::Board>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let board = board_service
        .update_board(domains::models::BoardId(id), req.title.as_deref(), req.rules.as_deref())
        .await
        .map_err(ApiError::from)?;
    Ok(Json(board))
}

/// `DELETE /admin/boards/:id` — delete a board (admin only).
pub async fn delete_board<BR>(
    State(board_service): State<Arc<BR>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError>
where
    BR: services::board::BoardRepo,
{
    board_service.delete_board(domains::models::BoardId(id)).await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /admin/boards/:id/config` — get board config by board UUID (admin).
pub async fn get_board_config_by_id<BR>(
    State(board_service): State<Arc<BR>>,
    _user: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<domains::models::BoardConfig>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let config = board_service.get_config(domains::models::BoardId(id)).await.map_err(ApiError::from)?;
    Ok(Json(config))
}

/// `PUT /admin/boards/:id/config` — update board config by board UUID (admin).
pub async fn update_board_config_by_id<BR>(
    State(board_service): State<Arc<BR>>,
    _user: AdminUser,
    Path(id): Path<Uuid>,
    Json(update): Json<BoardConfigUpdate>,
) -> Result<Json<domains::models::BoardConfig>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let current = board_service.get_config(domains::models::BoardId(id)).await.map_err(ApiError::from)?;
    let updated = update.apply_to(current);
    let saved = board_service.update_config(domains::models::BoardId(id), updated).await.map_err(ApiError::from)?;
    Ok(Json(saved))
}
/// `GET /board/:slug/config` — get board config (board owner or above).
pub async fn get_board_config<BR>(
    State(board_service): State<Arc<BR>>,
    _user: AuthenticatedUser,
    Path(slug): Path<String>,
) -> Result<Json<domains::models::BoardConfig>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let board = board_service.get_by_slug(&slug).await.map_err(ApiError::from)?;
    let config = board_service.get_config(board.id).await.map_err(ApiError::from)?;
    Ok(Json(config))
}

/// `PUT /board/:slug/config` — update board config (board owner or above).
pub async fn update_board_config<BR>(
    State(board_service): State<Arc<BR>>,
    _user: AuthenticatedUser,
    Path(slug): Path<String>,
    Json(update): Json<BoardConfigUpdate>,
) -> Result<Json<domains::models::BoardConfig>, ApiError>
where
    BR: services::board::BoardRepo,
{
    let board = board_service.get_by_slug(&slug).await.map_err(ApiError::from)?;
    let current = board_service.get_config(board.id).await.map_err(ApiError::from)?;
    let updated = update.apply_to(current);
    let saved = board_service.update_config(board.id, updated).await.map_err(ApiError::from)?;
    Ok(Json(saved))
}

// ─── Search ──────────────────────────────────────────────────────────────────

use domains::ports::PostRepository;

/// Combined state for the search handler: board lookup + FTS query.
pub struct SearchState<BR, PR> {
    pub board_svc: Arc<BR>,
    /// Repository held directly — `PgPostRepository` is `Clone` (wraps `PgPool`).
    pub post_repo: PR,
}

impl<BR, PR: Clone> Clone for SearchState<BR, PR> {
    fn clone(&self) -> Self {
        Self {
            board_svc: Arc::clone(&self.board_svc),
            post_repo: self.post_repo.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
/// Query parameters for `GET /boards/:slug/search`.
pub struct SearchQuery {
    /// Full-text query string.
    pub q: String,
    /// Page number (1-based). Defaults to 1.
    #[serde(default = "default_page")]
    pub page: u32,
}

fn default_page() -> u32 { 1 }

/// `GET /boards/:slug/search?q=...` — full-text search for posts on a board.
///
/// Returns `404` if the board does not exist.
/// Returns `403` if `board_config.search_enabled` is false.
/// Returns `400` if `q` is empty or missing.
pub async fn search_board<BR, PR>(
    State(s): State<SearchState<BR, PR>>,
    Path(slug): Path<String>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<crate::common::pagination::PageResponse<domains::models::Post>>, crate::common::errors::ApiError>
where
    BR: services::board::BoardRepo,
    PR: PostRepository,
{
    if params.q.trim().is_empty() {
        return Err(crate::common::errors::ApiError::BadRequest("search query `q` must not be empty".into()));
    }

    let board = s.board_svc.get_by_slug(&slug).await
        .map_err(crate::common::errors::ApiError::from)?;

    let config = s.board_svc.get_config(board.id).await
        .map_err(crate::common::errors::ApiError::from)?;

    if !config.search_enabled {
        return Err(crate::common::errors::ApiError::Forbidden);
    }

    let page = domains::models::Page::new(params.page);
    let results = s.post_repo
        .search_fulltext(board.id, &params.q, page)
        .await
        .map_err(crate::common::errors::ApiError::from)?;

    Ok(Json(crate::common::pagination::PageResponse::from(results)))
}
