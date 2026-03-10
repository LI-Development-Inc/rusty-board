//! Staff messaging handlers: inbox, send, mark-read, purge.
//!
//! All routes require at least `StaffUser` (BoardVolunteer or above).

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::common::{dtos::PaginationQuery, errors::ApiError};
use crate::axum::middleware::auth::StaffUser;
use domains::models::StaffMessageId;
use services::staff_message::StaffMessageService;

/// Query params for `GET /staff/messages/new`.
#[derive(Deserialize, Default)]
pub struct ComposeQuery {
    /// Optional pre-filled recipient UUID.
    pub to: Option<String>,
}

// ─── State ────────────────────────────────────────────────────────────────────

/// Shared state for all staff message handlers.
///
/// Holds the `StaffMessageService` behind an `Arc` for cheap cloning across requests.
pub struct StaffMessageState<MR: domains::ports::StaffMessageRepository> {
    /// The staff message service.
    pub svc: Arc<StaffMessageService<MR>>,
}

impl<MR: domains::ports::StaffMessageRepository> Clone for StaffMessageState<MR> {
    fn clone(&self) -> Self { Self { svc: self.svc.clone() } }
}

// ─── DTOs ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /staff/messages`.
#[derive(Deserialize)]
pub struct SendMessageRequest {
    /// UUID of the recipient staff account.
    pub to_user_id: Uuid,
    /// Plain-text message body (1–4 000 characters).
    pub body: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /staff/messages` — HTML inbox for the current staff user.
pub async fn inbox<MR>(
    State(s): State<StaffMessageState<MR>>,
    StaffUser(current): StaffUser,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError>
where
    MR: domains::ports::StaffMessageRepository,
{
    let page = domains::models::Page(q.page);
    let paginated = s.svc.inbox(current.id, page).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let total = paginated.total;
    let total_pages = paginated.total_pages() as u32;
    Ok(crate::axum::templates::StaffInboxTemplate {
        messages:     paginated.items,
        current_page: q.page,
        total_pages,
        total,
    })
}

/// `GET /staff/messages/new` — compose page (HTML form).
///
/// Accepts an optional `?to=<uuid>` query param to pre-fill the recipient field.
pub async fn compose_page(
    _staff: StaffUser,
    Query(q): Query<ComposeQuery>,
) -> impl IntoResponse {
    crate::axum::templates::StaffComposeTemplate {
        to_user_id: q.to.unwrap_or_default(),
    }
}

/// `POST /staff/messages` — send a new message.
///
/// Body must include `to_user_id` (UUID) and `body` (string, 1–4 000 chars).
pub async fn send_message<MR>(
    State(s): State<StaffMessageState<MR>>,
    StaffUser(current): StaffUser,
    Json(req): Json<SendMessageRequest>,
) -> Result<impl IntoResponse, ApiError>
where
    MR: domains::ports::StaffMessageRepository,
{
    let id = s.svc.send(
        &current,
        domains::models::UserId(req.to_user_id),
        req.body,
    ).await.map_err(|e| match e {
        services::staff_message::StaffMessageError::PermissionDenied { reason: _ } =>
            ApiError::Forbidden,
        services::staff_message::StaffMessageError::Validation { reason } =>
            ApiError::BadRequest(reason),
        other => ApiError::Internal(other.to_string()),
    })?;
    Ok(Json(serde_json::json!({ "id": id.to_string() })))
}

/// `POST /staff/messages/:id/read` — mark a message as read.
///
/// Silently succeeds if the message was already read or does not belong to the
/// caller — no information-leakage via 404.
pub async fn mark_read<MR>(
    State(s): State<StaffMessageState<MR>>,
    _staff: StaffUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError>
where
    MR: domains::ports::StaffMessageRepository,
{
    s.svc.mark_read(StaffMessageId(id)).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /staff/messages/purge` — delete messages older than 14 days.
///
/// Admin-only action. Returns the count of deleted messages.
pub async fn purge_expired<MR>(
    State(s): State<StaffMessageState<MR>>,
    crate::axum::middleware::auth::AdminUser(current): crate::axum::middleware::auth::AdminUser,
) -> Result<impl IntoResponse, ApiError>
where
    MR: domains::ports::StaffMessageRepository,
{
    let _ = current;
    let deleted = s.svc.purge_expired(14).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}
