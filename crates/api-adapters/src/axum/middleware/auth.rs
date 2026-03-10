//! JWT bearer authentication middleware.
//!
//! Accepts a token from two sources (checked in order):
//!   1. `Authorization: Bearer <token>` header (API clients, seed scripts)
//!   2. `token=<value>` HttpOnly cookie (browser sessions after login)
//!
//! Missing or invalid tokens do not reject the request. Handlers that require
//! authentication use `AuthenticatedUser`, `ModeratorUser`, or `AdminUser`
//! extractors which enforce the required role.

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use domains::models::{CurrentUser, Role, Token};
use domains::ports::AuthProvider;
use std::sync::Arc;

/// Extract a raw token string from the request.
///
/// Checks the `Authorization: Bearer` header first, then falls back to the
/// `token` cookie. Returns `None` if neither is present.
fn extract_token(req: &Request) -> Option<String> {
    // 1. Authorization header (API / seed scripts)
    if let Some(bearer) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        return Some(bearer.to_owned());
    }

    // 2. HttpOnly cookie (browser sessions set by POST /auth/login)
    req.headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(str::trim)
                .find(|part| part.starts_with("token="))
                .and_then(|part| part.strip_prefix("token="))
        })
        .map(str::to_owned)
}

/// Soft auth middleware — inserts `CurrentUser` into extensions if token is valid.
/// Missing/invalid tokens do not reject the request; handlers must enforce auth.
pub async fn auth_middleware(
    auth_provider: Arc<dyn AuthProvider>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(token_str) = extract_token(&req) {
        let token = Token::new(token_str);
        if let Ok(claims) = auth_provider.verify_token(&token).await {
            req.extensions_mut().insert(CurrentUser::from_claims(claims));
        }
    }
    next.run(req).await
}

/// Axum extractor — requires authenticated user; returns `401` if absent.
pub struct AuthenticatedUser(pub CurrentUser);

// axum 0.8: FromRequestParts uses RPITIT — plain async fn in impl, no #[async_trait] needed.
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AuthenticatedUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<CurrentUser>().cloned()
            .map(AuthenticatedUser)
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))
    }
}

/// Axum extractor — requires janitor or admin (site-wide moderation); returns `403` if insufficient role.
pub struct JanitorStaffUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for JanitorStaffUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        if u.can_moderate() { Ok(JanitorStaffUser(u)) }
        else { Err((StatusCode::FORBIDDEN, "janitor role required")) }
    }
}

/// Axum extractor — requires admin role; returns `403` if insufficient role.
pub struct AdminUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AdminUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        if u.role == Role::Admin { Ok(AdminUser(u)) }
        else { Err((StatusCode::FORBIDDEN, "admin role required")) }
    }
}

/// Backward-compatibility alias — handlers use this name while being updated.
pub use JanitorStaffUser as ModeratorUser;

/// Axum extractor — any authenticated user with a named role (User and above).
/// Use this for routes accessible to all registered accounts including Role::User.
pub struct AnyAuthenticatedUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AnyAuthenticatedUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        Ok(AnyAuthenticatedUser(u))
    }
}

/// Axum extractor — any authenticated staff member with moderation authority
/// (BoardVolunteer, BoardOwner, Janitor, Admin). Does NOT include Role::User.
/// Use `AnyAuthenticatedUser` for routes accessible to all registered accounts.
pub struct StaffUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for StaffUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        if u.can_delete() { Ok(StaffUser(u)) }
        else { Err((StatusCode::FORBIDDEN, "staff role required")) }
    }
}

/// Axum extractor — any staff who can perform moderation actions on at least some boards.
/// Includes BoardVolunteer, BoardOwner, Janitor, Admin.
pub struct AnyModerationUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AnyModerationUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        if u.can_delete() { Ok(AnyModerationUser(u)) }
        else { Err((StatusCode::FORBIDDEN, "staff moderation role required")) }
    }
}

/// Axum extractor — requires `BoardOwner` or `Admin` role.
///
/// Board owner routes must not be accessible to Janitors or Volunteers,
/// who have different authority scopes.
pub struct BoardOwnerUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for BoardOwnerUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        if matches!(u.role, Role::BoardOwner | Role::Admin) {
            Ok(BoardOwnerUser(u))
        } else {
            Err((StatusCode::FORBIDDEN, "board owner role required"))
        }
    }
}

/// Axum extractor — requires `BoardVolunteer`, `BoardOwner`, or `Admin` role.
///
/// Volunteer routes are accessible to the volunteer themselves as well as
/// board owners and admins (who have a superset of authority).
pub struct VolunteerUser(pub CurrentUser);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for VolunteerUser {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let u = parts.extensions.get::<CurrentUser>().cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "authentication required"))?;
        if matches!(u.role, Role::BoardVolunteer | Role::BoardOwner | Role::Admin) {
            Ok(VolunteerUser(u))
        } else {
            Err((StatusCode::FORBIDDEN, "volunteer role required"))
        }
    }
}
