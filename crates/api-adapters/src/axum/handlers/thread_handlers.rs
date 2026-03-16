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
    templates::{BoardTemplate, BoardThreadDisplay, CatalogTemplate, PostDisplay, ThreadTemplate},
};
use sha2::{Digest, Sha256};
use crate::common::{
    dtos::PaginationQuery,
    errors::ApiError,
    pagination::PageResponse,
};
use domains::models::{Page, Thread, ThreadId, ThreadSummary};

// ── Public HTML views ─────────────────────────────────────────────────────────

/// `GET /board/:slug` — thread index with unified OP post headers.
pub async fn show_board_html<TR: services::thread::ThreadRepo>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError>
{
    let all_threads = thread_service
        .get_catalog(board_ctx.board_id)
        .await
        .map_err(ApiError::from)?;

    const PAGE_SIZE: usize = 15;
    let total     = all_threads.len();
    let page_idx  = (q.page as usize).saturating_sub(1);
    let start     = page_idx * PAGE_SIZE;
    let total_pages = total.div_ceil(PAGE_SIZE).max(1) as u32;

    let threads: Vec<BoardThreadDisplay> = all_threads
        .into_iter()
        .skip(start)
        .take(PAGE_SIZE)
        .map(|t| {
            let mut hasher = Sha256::new();
            hasher.update(t.op_ip_hash.0.as_bytes());
            hasher.update(b"/");
            hasher.update(t.thread_id.0.to_string().as_bytes());
            let poster_id = hex::encode(&hasher.finalize()[..4]);
            let tripcode_level = t.op_tripcode.as_deref().map(|tc| {
                if tc.starts_with("!!!") { "super" }
                else if tc.starts_with("!!") { "secure" }
                else { "insecure" }
            });
            BoardThreadDisplay { thread: t, poster_id, tripcode_level }
        })
        .collect();

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

/// `GET /board/:slug/thread/:id` — thread with all posts, rendered as HTML.
///
/// Shows all posts in the thread (up to bump limit, 500) without pagination.
/// Staff with `can_delete()` receive the mod toolbar via `viewer_role`.
pub async fn show_thread_html<TR: services::thread::ThreadRepo>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Path((_slug, thread_id)): Path<(String, uuid::Uuid)>,
    maybe_user: Option<axum::extract::Extension<domains::models::CurrentUser>>,
) -> Result<impl IntoResponse, ApiError>
{
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

    // Load ALL posts — no pagination. Thread view shows every reply up to bump limit.
    let all_posts = thread_service
        .list_all_posts(ThreadId(thread_id))
        .await
        .map_err(ApiError::from)?;

    let is_closed = thread.closed;
    let thread_id_str = thread_id.to_string();

    let post_ids: Vec<_> = all_posts.iter().map(|p| p.id).collect();
    let mut attachments_map = thread_service
        .find_post_attachments(&post_ids)
        .await
        .map_err(ApiError::from)?;

    let posts: Vec<PostDisplay> = all_posts.into_iter().map(|post| {
        let mut hasher = Sha256::new();
        hasher.update(post.ip_hash.0.as_bytes());
        hasher.update(b"/");
        hasher.update(thread_id_str.as_bytes());
        let hash_bytes = hasher.finalize();
        let poster_id  = hex::encode(&hash_bytes[..4]);
        let attachments = attachments_map.remove(&post.id).unwrap_or_default();
        let capcode_role = post.tripcode.as_deref()
            .and_then(services::common::tripcode::capcode_role_str)
            .map(str::to_owned);
        let capcode_css  = capcode_role.as_deref()
            .map(services::common::tripcode::capcode_css_class);
        let tripcode_level = if capcode_role.is_some() {
            None // capcode, not a tripcode
        } else {
            post.tripcode.as_deref().map(|t| {
                if t.starts_with("!!!") { "super" }
                else if t.starts_with("!!") { "secure" }
                else { "insecure" }
            })
        };
        let ip_hash_short = post.ip_hash.0.chars().take(10).collect();
        PostDisplay { post, poster_id, attachments, capcode_role, capcode_css, tripcode_level, ip_hash_short }
    }).collect();

    let tmpl = ThreadTemplate {
        board:       board_ctx.board,
        thread:      thread.clone(),
        posts,
        is_closed,
        is_cycle:    thread.cycle,
        viewer_role,
    };
    Ok(tmpl)
}

/// `GET /board/:slug/post/:post_number` — redirect to the thread containing this post.
///
/// Resolves cross-board `>>>/{slug}/{N}` links. The post number is board-scoped
/// (the `No.N` counter), not a UUID. Returns 303 to the correct thread anchor,
/// or 404 if no post with that number exists on this board.
pub async fn redirect_to_post<TR: services::thread::ThreadRepo>(
    State(thread_service): State<Arc<TR>>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Path((_slug, post_number)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError>
{
    let thread_id = thread_service
        .find_thread_id_by_post_number(board_ctx.board_id, post_number)
        .await
        .map_err(ApiError::from)?;

    match thread_id {
        Some(tid) => {
            let url = format!(
                "/board/{}/thread/{}#post-{}",
                board_ctx.board.slug, tid, post_number
            );
            Ok(axum::response::Redirect::to(&url).into_response())
        }
        None => Err(ApiError::NotFound(format!("Post #{post_number} not found on this board"))),
    }
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
