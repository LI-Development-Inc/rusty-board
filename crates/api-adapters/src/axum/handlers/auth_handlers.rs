//! Authentication handlers: `POST /auth/login`, `POST /auth/refresh`, `GET /auth/logout`.

use axum::{extract::State, http::header, response::IntoResponse, Json};
use std::sync::Arc;

use crate::axum::middleware::auth::AuthenticatedUser;
use crate::common::{
    dtos::{LoginRequest, LoginResponse, RegisterRequest},
    errors::ApiError,
};
use domains::ports::AuthProvider;
use services::user::UserService;

/// `POST /auth/login` — verify credentials, issue a JWT, and set an HttpOnly cookie.
///
/// Accepts `Content-Type: application/json` with `{ "username": "...", "password": "..." }`.
/// On success: returns `200` with a `LoginResponse` body **and** sets the `token` cookie.
/// On failure: returns `401`.
///
/// The cookie approach lets browser-based sessions work without JavaScript
/// needing to manually attach `Authorization` headers on every navigation.
pub async fn login<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    let (token, claims) = user_service
        .login(&req.username, &req.password)
        .await
        .map_err(ApiError::from)?;

    // TTL in seconds for the Max-Age directive (same as JWT expiry).
    let ttl_secs = claims.exp - chrono::Utc::now().timestamp();
    let cookie = format!(
        "token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        token.0,
        ttl_secs.max(0),
    );

    let body = Json(LoginResponse {
        token:      token.0,
        expires_at: claims.exp,
    });

    Ok((
        [(header::SET_COOKIE, cookie)],
        body,
    ))
}

/// `POST /auth/refresh` — accept a still-valid token and return a refreshed one.
pub async fn refresh_token<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    AuthenticatedUser(current_user): AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    let (token, claims) = user_service
        .refresh(current_user.user_id())
        .await
        .map_err(ApiError::from)?;

    let ttl_secs = claims.exp - chrono::Utc::now().timestamp();
    let cookie = format!(
        "token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        token.0,
        ttl_secs.max(0),
    );

    let body = Json(LoginResponse {
        token:      token.0,
        expires_at: claims.exp,
    });

    Ok(([(header::SET_COOKIE, cookie)], body))
}

/// `GET /auth/logout` — clear the `token` cookie and redirect to the overboard.
pub async fn logout() -> impl IntoResponse {
    // Max-Age=0 immediately expires the cookie in the browser.
    let clear_cookie = "token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0";
    (
        [(header::SET_COOKIE, clear_cookie)],
        axum::response::Redirect::to("/overboard"),
    )
}

/// `GET /auth/login` — render the login page (HTML).
pub async fn login_page() -> axum::response::Response {
    use crate::axum::templates::LoginTemplate;
    LoginTemplate { error: None }.into_response()
}

/// `GET /auth/register` — render the registration page.
///
/// Returns 404 (via redirect to login) when open registration is disabled;
/// the gating is done in the route layer via `Settings.open_registration`.
pub async fn register_page() -> axum::response::Response {
    use crate::axum::templates::RegisterTemplate;
    RegisterTemplate { error: None }.into_response()
}

/// `POST /auth/register` — create a new `Role::User` account.
///
/// Accepts `Content-Type: application/json` with `RegisterRequest`.
/// On success: returns 201 and redirects to `/auth/login?registered=1`.
/// On failure: re-renders the registration page with an error message.
pub async fn register<UR, AP>(
    State(user_service): State<Arc<UserService<UR, AP>>>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError>
where
    UR: domains::ports::UserRepository,
    AP: AuthProvider,
{
    if req.password != req.password_confirm {
        return Err(ApiError::Validation {
            message: "Passwords do not match.".to_owned(),
        });
    }

    user_service
        .register(&req.username, &req.password)
        .await
        .map_err(ApiError::from)?;

    Ok(axum::http::StatusCode::CREATED)
}

/// `GET /auth/me` — return the current user's identity for client-side nav rendering.
///
/// Returns `200 { username, role, dashboard_url }` if the token cookie is valid.
/// Returns `401` (empty body) when the user is not logged in. Never redirects.
///
/// Called by the base template JavaScript to decide whether to show `[login]` or
/// `[username | role] [dashboard] [logout]` in the nav.
pub async fn me(
    req: axum::extract::Request,
) -> impl IntoResponse {
    let maybe_user = req.extensions().get::<domains::models::CurrentUser>().cloned();
    match maybe_user {
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
        Some(user) => {
            let dashboard = match user.role {
                domains::models::Role::Admin          => "/admin/dashboard",
                domains::models::Role::Janitor        => "/janitor/dashboard",
                domains::models::Role::BoardOwner     => "/board-owner/dashboard",
                domains::models::Role::BoardVolunteer => "/volunteer/dashboard",
                domains::models::Role::User           => "/user/dashboard",
            };
            let role_label = match user.role {
                domains::models::Role::Admin          => "Admin",
                domains::models::Role::Janitor        => "Janitor",
                domains::models::Role::BoardOwner     => "Board Owner",
                domains::models::Role::BoardVolunteer => "Volunteer",
                domains::models::Role::User           => "User",
            };
            Json(serde_json::json!({
                "username":      user.username,
                "role":          role_label,
                "dashboard_url": dashboard,
            }))
            .into_response()
        }
    }
}
