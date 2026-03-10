//! Board volunteer management handlers.
//!
//! Routes (all board-scoped):
//!   `GET    /board/:slug/volunteers`          — list volunteers for a board
//!   `POST   /board/:slug/volunteers`          — add a volunteer by username
//!   `DELETE /board/:slug/volunteers/:user_id` — remove a volunteer

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::axum::middleware::{
    auth::AuthenticatedUser,
    board_config::ExtractedBoardConfig,
};
use crate::common::errors::ApiError;
use domains::models::UserId;
use services::board::BoardRepo;

/// A volunteer entry returned in the list.
#[derive(Debug, Serialize)]
pub struct VolunteerEntry {
    /// The volunteer's user ID.
    pub user_id:     String,
    /// The volunteer's username.
    pub username:    String,
    /// ISO-8601 timestamp of when they were assigned.
    pub assigned_at: String,
}

/// Request body for adding a volunteer.
#[derive(Debug, Deserialize)]
pub struct AddVolunteerRequest {
    /// Username of the user to add as volunteer.
    pub username: String,
}

/// `GET /board/:slug/volunteers` — list all volunteers for this board.
pub async fn list_volunteers<BR: BoardRepo>(
    State(board_svc): State<Arc<BR>>,
    AuthenticatedUser(current): AuthenticatedUser,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
) -> Result<impl IntoResponse, ApiError> {
    if !current.can_manage_board_config(board_ctx.board_id) {
        return Err(ApiError::Forbidden);
    }
    let vols = board_svc.list_volunteers(board_ctx.board_id).await
        .map_err(ApiError::from)?;
    let entries: Vec<VolunteerEntry> = vols.into_iter().map(|(uid, uname, at)| VolunteerEntry {
        user_id:     uid.to_string(),
        username:    uname,
        assigned_at: at.to_rfc3339(),
    }).collect();
    Ok(Json(entries))
}

/// `POST /board/:slug/volunteers` — add a user as volunteer by username.
pub async fn add_volunteer<BR: BoardRepo>(
    State(board_svc): State<Arc<BR>>,
    AuthenticatedUser(current): AuthenticatedUser,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Json(req): Json<AddVolunteerRequest>,
) -> Result<StatusCode, ApiError> {
    if !current.can_manage_board_config(board_ctx.board_id) {
        return Err(ApiError::Forbidden);
    }
    board_svc.add_volunteer_by_username(board_ctx.board_id, &req.username, current.id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::CREATED)
}

/// `DELETE /board/:slug/volunteers/:user_id` — remove a volunteer.
pub async fn remove_volunteer<BR: BoardRepo>(
    State(board_svc): State<Arc<BR>>,
    AuthenticatedUser(current): AuthenticatedUser,
    axum::extract::Extension(board_ctx): axum::extract::Extension<ExtractedBoardConfig>,
    Path((_slug, user_id)): Path<(String, Uuid)>,
) -> Result<StatusCode, ApiError> {
    if !current.can_manage_board_config(board_ctx.board_id) {
        return Err(ApiError::Forbidden);
    }
    board_svc.remove_volunteer(board_ctx.board_id, UserId(user_id))
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
