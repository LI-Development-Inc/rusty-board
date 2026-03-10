//! Authentication routes: `/auth/login`, `/auth/refresh`, `/auth/logout`.

use axum::{routing::{get, post}, Router};
use std::sync::Arc;

use domains::ports::AuthProvider;
use services::user::UserService;

use crate::axum::handlers::auth_handlers;

/// Mount auth routes.
///
/// `GET  /auth/login`    — render login page (HTML)
/// `POST /auth/login`    — issue a JWT from username + password; sets `token` cookie
/// `POST /auth/refresh`  — extend an existing valid JWT; refreshes `token` cookie
/// `GET  /auth/logout`   — clear `token` cookie and redirect to overboard
/// `GET  /auth/register` — render registration page (only when `open_registration` is true)
/// `POST /auth/register` — create a new `Role::User` account
pub fn auth_routes<UR, AP>(
    user_service: Arc<UserService<UR, AP>>,
    open_registration: bool,
) -> Router
where
    UR: domains::ports::UserRepository + 'static,
    AP: AuthProvider + 'static,
{
    let mut router = Router::new()
        .route(
            "/auth/login",
            get(auth_handlers::login_page)
                .post(auth_handlers::login::<UR, AP>),
        )
        .route("/auth/refresh", post(auth_handlers::refresh_token::<UR, AP>))
        .route("/auth/logout",  get(auth_handlers::logout))
        .route("/auth/me",      get(auth_handlers::me));

    if open_registration {
        router = router.route(
            "/auth/register",
            get(auth_handlers::register_page)
                .post(auth_handlers::register::<UR, AP>),
        );
    }

    router.with_state(user_service)
}
