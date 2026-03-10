//! Post handlers: create post/thread.

use axum::{
    extract::{Multipart, Path, State},
    response::IntoResponse,
};
use bytes::Bytes;
use mime::Mime;
use std::str::FromStr;
use std::sync::Arc;

use crate::axum::middleware::board_config::ExtractedBoardConfig;
use crate::common::errors::ApiError;
use domains::models::ThreadId;
use domains::ports::{BanRepository, MediaProcessor, MediaStorage, RateLimiter, RawMedia};
use services::post::{PostDraft, PostService};
use services::common::utils::hash_ip;

/// `POST /board/:slug/post` — create a new thread or reply.
///
/// Accepts multipart/form-data with fields:
/// - `thread_id` (optional Uuid) — if present, creates a reply; else starts a new thread
/// - `name` (optional) — poster name
/// - `email` (optional) — 'sage' to disable bump
/// - `body` — post body text
/// - `files` (0..N file parts) — attachments
///
/// The real IP is extracted from the peer address (set by reverse proxy middleware),
/// immediately SHA-256 hashed with a daily salt, and never stored raw.
pub async fn create_post<PR, TR, BR, MS, RL, MP>(
    State(post_service): State<Arc<PostService<PR, TR, BR, MS, RL, MP>>>,
    axum::extract::ConnectInfo(peer_addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    // Optional — present if a valid JWT was supplied; absent for anonymous users.
    current_user: Option<axum::extract::Extension<domains::models::CurrentUser>>,
    Path(_slug): Path<String>,
    mut multipart: Multipart,
) -> Result<axum::response::Response, ApiError>
where
    PR: domains::ports::PostRepository,
    TR: domains::ports::ThreadRepository,
    BR: BanRepository,
    MS: MediaStorage,
    RL: RateLimiter,
    MP: MediaProcessor,
{
    let raw_ip = peer_addr.ip().to_string();
    // INVARIANT: IP is hashed immediately; raw value is never stored.
    // Daily salt is derived from the UTC date — rotates at midnight without persistence.
    let daily_salt = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let ip_hash = hash_ip(&raw_ip, &daily_salt);

    let is_staff = current_user.is_some();
    let poster_role = current_user.as_ref().map(|ext| ext.0.role.clone());

    let mut draft = PostDraft {
        board_id:    board_ctx.board_id,
        thread_id:   None,
        body:        String::new(),
        name:        None,
        email:       None,
        ip_hash,
        files:       Vec::new(),
        is_staff,
        poster_role,
    };

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart error: {e}")))?
    {
        let field_name = field.name().unwrap_or("").to_owned();
        match field_name.as_str() {
            "thread_id" => {
                let val = field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
                if !val.is_empty() {
                    let id = uuid::Uuid::from_str(&val)
                        .map_err(|_| ApiError::BadRequest("invalid thread_id UUID".to_owned()))?;
                    draft.thread_id = Some(ThreadId(id));
                }
            }
            "body" => {
                draft.body = field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
            }
            "name" => {
                let val = field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
                if !val.is_empty() { draft.name = Some(val); }
            }
            "email" => {
                let val = field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
                if !val.is_empty() { draft.email = Some(val); }
            }
            "files" => {
                let content_type = field
                    .content_type()
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| "application/octet-stream".to_owned());
                let filename = field
                    .file_name()
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| "file".to_owned());
                let data: Bytes = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
                // Skip empty file fields — browsers submit an empty "files" part
                // when no file is selected; treating it as an attachment causes a
                // mime-type validation error.
                if data.is_empty() { continue; }
                let mime = Mime::from_str(&content_type).unwrap_or(mime::APPLICATION_OCTET_STREAM);
                draft.files.push(RawMedia { filename, mime, data });
            }
            _ => {
                // Ignore unknown fields
                let _ = field.bytes().await;
            }
        }
    }

    let board_slug = board_ctx.board.slug.as_str().to_owned();
    let result = post_service
        .create_post(draft, &board_ctx.config)
        .await
        .map_err(ApiError::from)?;

    let thread_id = result.thread.id;
    let post_num  = result.post.post_number;

    // Browser form submissions get a redirect to the new post.
    // API clients should use the board's thread/post read endpoints.
    let redirect_url = format!(
        "/board/{}/thread/{}#post-{}",
        board_slug, thread_id, post_num
    );
    // TODO v1.1.1: add `X-Post-Number: {post_num}` response header so the thread.html
    // JavaScript can store the number in localStorage for (You) tracking without needing
    // to parse the Location/redirect URL.  Use `axum::response::AppendHeaders` to attach
    // it alongside the Redirect, e.g.:
    //   use axum::response::AppendHeaders;
    //   return Ok((AppendHeaders([("X-Post-Number", post_num.to_string())]),
    //              Redirect::to(&redirect_url)).into_response());
    Ok(axum::response::Redirect::to(&redirect_url).into_response())
}
