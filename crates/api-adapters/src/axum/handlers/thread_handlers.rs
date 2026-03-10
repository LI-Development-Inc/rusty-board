//! Thread handlers: board index, catalog, and thread view.
//!
//! Public routes render HTML via Askama templates using the `board` and `config`
//! already resolved by the `board_config_middleware` into `ExtractedBoardConfig`.
//! JSON variants are kept for programmatic/mobile clients.

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

use crate::axum::{
    middleware::board_config::ExtractedBoardConfig,
    templates::{BoardTemplate, CatalogTemplate, PostDisplay, ThreadTemplate},
};
use sha2::{Digest, Sha256};
use crate::common::{
    dtos::PaginationQuery,
    errors::ApiError,
    pagination::PageResponse,
};
use domains::models::{Page, Thread, ThreadId, ThreadSummary};

// ── Public HTML views ─────────────────────────────────────────────────────────

/// `GET /board/:slug` — thread list rendered as HTML with OP body previews.
///
/// Uses `get_catalog` (which returns `ThreadSummary` with OP body) so the board
/// index can show meaningful thread titles. Pagination is applied after the fetch
/// since the catalog returns all threads sorted by bump time.
pub async fn show_board_html<TR: services::thread::ThreadRepo>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError>
{
    // Use catalog (returns ThreadSummary with op_body) rather than raw Thread list.
    let all_threads = thread_service
        .get_catalog(board_ctx.board_id)
        .await
        .map_err(ApiError::from)?;

    // Manual pagination on the in-memory catalog (typically ≤200 threads on a board).
    const PAGE_SIZE: usize = 15;
    let total = all_threads.len();
    let page_idx = (q.page as usize).saturating_sub(1);
    let start = page_idx * PAGE_SIZE;
    let threads: Vec<_> = all_threads.into_iter().skip(start).take(PAGE_SIZE).collect();
    let total_pages = total.div_ceil(PAGE_SIZE).max(1) as u32;

    let tmpl = BoardTemplate {
        board:        board_ctx.board,
        config:       board_ctx.config,
        total_pages,
        threads,
        current_page: q.page,
    };
    Ok(tmpl)
}

/// `GET /board/:slug/catalog` — catalog grid rendered as HTML.
pub async fn show_catalog_html<TR: services::thread::ThreadRepo>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
) -> Result<impl IntoResponse, ApiError>
{
    let threads = thread_service
        .get_catalog(board_ctx.board_id)
        .await
        .map_err(ApiError::from)?;

    let tmpl = CatalogTemplate { board: board_ctx.board, threads, config: board_ctx.config };
    Ok(tmpl)
}

/// `GET /board/:slug/thread/:id` — thread with posts, rendered as HTML.
///
/// Accepts an optional `CurrentUser` extension (populated by the auth middleware when
/// a valid session cookie is present). When the viewer holds a moderation role
/// (`janitor`, `board_owner`, `board_volunteer`, or `admin`) the template receives
/// a non-`None` `viewer_role` that enables the inline moderation toolbar and IP hash
/// display on every post.
pub async fn show_thread_html<TR: services::thread::ThreadRepo>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Path((_slug, thread_id)): Path<(String, uuid::Uuid)>,
    Query(q): Query<PaginationQuery>,
    maybe_user: Option<axum::extract::Extension<domains::models::CurrentUser>>,
) -> Result<impl IntoResponse, ApiError>
{
    // Determine if the viewer has moderation rights (for the mod toolbar).
    let viewer_role: Option<String> = maybe_user
        .and_then(|axum::extract::Extension(u)| {
            if u.can_delete() {
                Some(match u.role {
                    domains::models::Role::Admin          => "admin",
                    domains::models::Role::Janitor        => "janitor",
                    domains::models::Role::BoardOwner     => "board_owner",
                    domains::models::Role::BoardVolunteer => "board_volunteer",
                    _                                     => return None,
                }.to_owned())
            } else {
                None
            }
        });

    let thread = thread_service
        .get_thread(ThreadId(thread_id))
        .await
        .map_err(ApiError::from)?;

    let paginated_posts = thread_service
        .list_posts(ThreadId(thread_id), Page::new(q.page))
        .await
        .map_err(ApiError::from)?;

    let is_closed = thread.closed;
    let thread_id_str = thread_id.to_string();
    let total_pages   = paginated_posts.total_pages() as u32;

    // Build PostDisplay entries — compute per-thread poster ID for each post.
    // poster_id = first 8 hex chars of SHA-256(ip_hash + "/" + thread_id)
    // Stable per poster per thread; different across threads.
    let post_ids: Vec<_> = paginated_posts.items.iter().map(|p| p.id).collect();
    let mut attachments_map = thread_service
        .find_post_attachments(&post_ids)
        .await
        .map_err(ApiError::from)?;

    let posts: Vec<PostDisplay> = paginated_posts.items.into_iter().map(|post| {
        let mut hasher = Sha256::new();
        hasher.update(post.ip_hash.0.as_bytes());
        hasher.update(b"/");
        hasher.update(thread_id_str.as_bytes());
        let hash_bytes = hasher.finalize();
        let poster_id  = hex::encode(&hash_bytes[..4]); // 8 hex chars
        let attachments = attachments_map.remove(&post.id).unwrap_or_default();
        // Compute capcode fields from the stored tripcode value
        let capcode_role = post.tripcode.as_deref()
            .and_then(services::common::tripcode::capcode_role_str)
            .map(str::to_owned);
        let capcode_css  = capcode_role.as_deref()
            .map(services::common::tripcode::capcode_css_class);
        PostDisplay { post, poster_id, attachments, capcode_role, capcode_css }
    }).collect();

    let tmpl = ThreadTemplate {
        board:        board_ctx.board,
        thread,
        total_pages,
        posts,
        current_page: q.page,
        is_closed,
        viewer_role,
    };
    Ok(tmpl)
}

// ── JSON API variants ─────────────────────────────────────────────────────────

/// `GET /board/:slug` — paginated thread list as JSON.
pub async fn list_threads<TR>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<PageResponse<Thread>>, ApiError>
where
    TR: services::thread::ThreadRepo,
{
    let page = Page::new(q.page);
    let result = thread_service
        .list_threads(board_ctx.board_id, page)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result.into()))
}

/// `GET /board/:slug/catalog` — catalog view as JSON.
pub async fn show_catalog<TR>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
) -> Result<Json<Vec<ThreadSummary>>, ApiError>
where
    TR: services::thread::ThreadRepo,
{
    let summaries = thread_service
        .get_catalog(board_ctx.board_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(summaries))
}

/// `GET /board/:slug/thread/:id` — thread view as JSON.
pub async fn show_thread<TR>(
    State(thread_service): State<Arc<TR>>,
    Path((_slug, thread_id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<Thread>, ApiError>
where
    TR: services::thread::ThreadRepo,
{
    let thread = thread_service
        .get_thread(ThreadId(thread_id))
        .await
        .map_err(ApiError::from)?;
    Ok(Json(thread))
}
