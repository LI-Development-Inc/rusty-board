//! Overboard handler: recent posts across all boards.

use axum::{
    extract::{Query, State},
    response::IntoResponse,
};
use std::sync::Arc;
use sha2::{Digest, Sha256};

use crate::axum::templates::{OverboardPostDisplay, OverboardTemplate};
use crate::common::{
    dtos::PaginationQuery,
    errors::ApiError,
};
use domains::models::{OverboardPost, Page, PostId};

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

    let boards = state
        .board_service
        .list_boards(Page::new(1))
        .await
        .map_err(ApiError::from)?
        .items;

    let paginated = state
        .post_service
        .list_overboard(page)
        .await
        .map_err(ApiError::from)?;

    // Bulk-load attachments for all overboard posts in one query.
    let post_ids: Vec<PostId> = paginated.items.iter().map(|p| p.id).collect();
    let total_pages = paginated.total_pages() as u32;
    let mut attachments_map = state
        .post_service
        .find_post_attachments(&post_ids)
        .await
        .map_err(ApiError::from)?;

    let recent_posts: Vec<OverboardPostDisplay> = paginated.items.into_iter().map(|post| {
        let attachments = attachments_map.remove(&post.id).unwrap_or_default();

        // Poster ID: SHA-256(ip_hash + "/" + thread_id), first 4 bytes as hex.
        let mut hasher = Sha256::new();
        hasher.update(post.ip_hash.0.as_bytes());
        hasher.update(b"/");
        hasher.update(post.thread_id.0.to_string().as_bytes());
        let poster_id = hex::encode(&hasher.finalize()[..4]);

        let tripcode_level = post.tripcode.as_deref().map(|t| {
            if t.starts_with("!!!") { "super" }
            else if t.starts_with("!!") { "secure" }
            else { "insecure" }
        });

        let ip_hash_short = post.ip_hash.0.chars().take(10).collect();

        OverboardPostDisplay { post, attachments, poster_id, tripcode_level, ip_hash_short }
    }).collect();

    let tmpl = OverboardTemplate {
        boards,
        recent_posts,
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
    async fn list_overboard(&self, page: Page) -> Result<domains::models::Paginated<OverboardPost>, services::post::PostError>;
    /// Bulk-fetch attachments for a slice of post IDs. Used by the overboard view.
    async fn find_post_attachments(
        &self,
        post_ids: &[PostId],
    ) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, services::post::PostError>;
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
    async fn find_post_attachments(
        &self,
        post_ids: &[PostId],
    ) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, services::post::PostError> {
        self.find_post_attachments(post_ids).await
    }
}
