//! User account handlers: dashboard, staff request submission.
//!
//! Routes:
//!   `GET  /user/dashboard`  — render the user dashboard (any authenticated user)
//!   `POST /user/requests`   — submit a staff escalation request

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::axum::middleware::auth::AnyAuthenticatedUser;
use crate::axum::templates::UserDashboardTemplate;
use crate::common::errors::ApiError;
use domains::models::Slug;
use domains::ports::{AuthProvider, StaffRequestRepository};
use services::staff_request::StaffRequestService;
use services::user::UserService;

/// Combined handler state for user routes — needs both services.
///
/// `Clone` is implemented manually so the derive does not require
/// `UR: Clone + AP: Clone + RR: Clone` — the fields are `Arc<...>` which
/// are always `Clone`.
pub struct UserDashboardState<UR, AP, RR>
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    RR: StaffRequestRepository + 'static,
{
    /// User service — provides access to the current user's details.
    pub user_svc:    Arc<UserService<UR, AP>>,
    /// Staff request service — loads and submits requests.
    pub request_svc: Arc<StaffRequestService<RR, UR>>,
}

impl<UR, AP, RR> Clone for UserDashboardState<UR, AP, RR>
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
    RR: StaffRequestRepository + 'static,
{
    fn clone(&self) -> Self {
        Self {
            user_svc:    Arc::clone(&self.user_svc),
            request_svc: Arc::clone(&self.request_svc),
        }
    }
}

/// `GET /user/dashboard` — render the user's personal dashboard.
///
/// Accessible to any authenticated user (`AnyAuthenticatedUser` extractor).
pub async fn user_dashboard<UR, AP, RR>(
    State(state): State<UserDashboardState<UR, AP, RR>>,
    AnyAuthenticatedUser(current): AnyAuthenticatedUser,
) -> Result<axum::response::Response, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
    RR: StaffRequestRepository,
{
    use axum::response::IntoResponse as _;

    let requests = state.request_svc
        .list_by_user(current.id)
        .await
        .map_err(ApiError::from)?;

    Ok(UserDashboardTemplate {
        username:         current.username.clone(),
        joined_at:        "—".to_owned(), // TODO v1.1: store created_at on User
        pending_requests: requests,
    }
    .into_response())
}

/// Request body for `POST /user/requests`.
#[derive(Debug, Deserialize)]
pub struct SubmitRequestBody {
    /// One of: `"board_create"`, `"become_volunteer"`, `"become_janitor"`.
    pub request_type:  String,
    /// Optional notes / pitch from the requester.
    #[serde(default)]
    pub notes:         String,
    /// For `become_volunteer` — the target board's slug.
    pub target_slug:   Option<String>,
    /// For `board_create` — the requester's preferred slug.
    pub preferred_slug:  Option<String>,
    /// For `board_create` — the requester's preferred board title.
    pub preferred_title: Option<String>,
    /// For `board_create` — board rules text.
    #[serde(default)]
    pub rules:           String,
}

/// `POST /user/requests` — submit a staff escalation request.
///
/// Dispatches to the appropriate `StaffRequestService` method based on
/// `request_type`. Returns `201 Created` on success.
pub async fn submit_request<UR, AP, RR>(
    State(state): State<UserDashboardState<UR, AP, RR>>,
    AnyAuthenticatedUser(current): AnyAuthenticatedUser,
    Json(body): Json<SubmitRequestBody>,
) -> Result<StatusCode, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
    RR: StaffRequestRepository,
{
    match body.request_type.as_str() {
        "board_create" => {
            let slug  = body.preferred_slug.as_deref().unwrap_or("");
            let title = body.preferred_title.as_deref().unwrap_or("");
            state.request_svc
                .submit_board_create(current.id, slug, title, &body.rules, &body.notes)
                .await
                .map_err(ApiError::from)?;
        }
        "become_volunteer" => {
            let raw_slug = body.target_slug.ok_or_else(|| {
                ApiError::BadRequest("target_slug is required for become_volunteer".to_owned())
            })?;
            let slug = Slug::new(&raw_slug).map_err(|e| ApiError::BadRequest(e.to_string()))?;
            state.request_svc
                .submit_become_volunteer(current.id, slug, &body.notes)
                .await
                .map_err(ApiError::from)?;
        }
        "become_janitor" => {
            state.request_svc
                .submit_become_janitor(current.id, &body.notes)
                .await
                .map_err(ApiError::from)?;
        }
        other => {
            return Err(ApiError::BadRequest(format!("unknown request_type: {other}")));
        }
    }

    Ok(StatusCode::CREATED)
}
