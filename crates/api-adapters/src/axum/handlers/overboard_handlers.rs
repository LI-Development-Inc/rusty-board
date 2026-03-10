//! Overboard handler: recent posts across all boards.

use axum::{
    extract::{Query, State},
    response::IntoResponse,
};
use std::sync::Arc;

use crate::axum::templates::OverboardTemplate;
use crate::common::{
    dtos::PaginationQuery,
    errors::ApiError,
};
use domains::models::{OverboardPost, Page};

/// Combined state for the overboard handler.
pub struct OverboardState<BR, PR> {
    /// Board service used to list all boards for the navigation header.
    pub board_service: Arc<BR>,
    /// Post service used to load recent posts across all boards.
    pub post_service:  Arc<PR>,
}

impl<BR, PR> Clone for OverboardState<BR, PR> {
    fn clone(&self) -> Self {
        Self {
            board_service: self.board_service.clone(),
            post_service:  self.post_service.clone(),
        }
    }
}

/// `GET /overboard` — recent posts across all boards, rendered as HTML.
pub async fn show_overboard<BR, PR>(
    State(state): State<OverboardState<BR, PR>>,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError>
where
    BR: services::board::BoardRepo,
    PR: OverboardPostSource,
{
    let page = Page::new(q.page);

    // Load all boards for the navigation header (first page — up to 15)
    let boards = state
        .board_service
        .list_boards(Page::new(1))
        .await
        .map_err(ApiError::from)?
        .items;

    // Load recent posts across all boards
    let paginated = state
        .post_service
        .list_overboard(page)
        .await
        .map_err(ApiError::from)?;

    let total_pages = paginated.total_pages() as u32;
    let tmpl = OverboardTemplate {
        boards,
        recent_posts: paginated.items,
        current_page: q.page,
        total_pages,
    };
    Ok(tmpl)
}

/// Minimal trait for sources that can list posts across all boards.
///
/// Implemented by `PostService` via a blanket impl below.
#[async_trait::async_trait]
pub trait OverboardPostSource: Send + Sync + 'static {
    /// Return a paginated list of recent posts across all boards, ordered by creation date descending.
    async fn list_overboard(&self, page: Page) -> Result<domains::models::Paginated<OverboardPost>, services::post::PostError>;
}

#[async_trait::async_trait]
impl<PR, TR, BR, MS, RL, MP> OverboardPostSource
    for services::post::PostService<PR, TR, BR, MS, RL, MP>
where
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    BR: domains::ports::BanRepository,
    MS: domains::ports::MediaStorage,
    RL: domains::ports::RateLimiter,
    MP: domains::ports::MediaProcessor,
{
    async fn list_overboard(&self, page: Page) -> Result<domains::models::Paginated<OverboardPost>, services::post::PostError> {
        self.list_overboard(page).await
    }
}
