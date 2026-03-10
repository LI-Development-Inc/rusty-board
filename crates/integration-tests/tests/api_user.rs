//! Integration tests for user account HTTP endpoints.
//!
//! Covers:
//! - `GET  /user/dashboard`  — requires any authenticated user
//! - `POST /user/requests`   — submit a staff escalation request
//!
//! `CurrentUser` is injected directly into request extensions (same technique
//! as `api_moderation.rs`) — no real auth middleware or JWT signing needed.

use api_adapters::axum::routes::user_routes::user_routes;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use domains::{errors::DomainError, models::*, ports::*};
use services::staff_request::StaffRequestService;
use services::user::UserService;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Stubs ────────────────────────────────────────────────────────────────────

struct NopUserRepo;

#[async_trait::async_trait]
impl UserRepository for NopUserRepo {
    async fn find_by_id(&self, _: UserId) -> Result<User, DomainError> {
        Ok(make_user(Role::User))
    }
    async fn find_by_username(&self, _: &str) -> Result<User, DomainError> {
        Ok(make_user(Role::User))
    }
    async fn find_all(&self, page: Page) -> Result<Paginated<User>, DomainError> {
        Ok(Paginated::new(vec![], 0, page, 15))
    }
    async fn save(&self, _: &User) -> Result<(), DomainError> { Ok(()) }
    async fn deactivate(&self, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn find_owned_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn find_volunteer_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn add_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn add_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
}

struct NopAuth;

#[async_trait::async_trait]
impl AuthProvider for NopAuth {
    async fn create_token(&self, _: &Claims) -> Result<Token, DomainError> {
        Err(DomainError::internal("unused"))
    }
    async fn verify_token(&self, _: &Token) -> Result<Claims, DomainError> {
        Err(DomainError::auth())
    }
    async fn hash_password(&self, p: &str) -> Result<PasswordHash, DomainError> {
        Ok(PasswordHash::new(format!("h:{p}")))
    }
    async fn verify_password(&self, _: &str, _: &PasswordHash) -> Result<(), DomainError> {
        Ok(())
    }
}

struct NopRequestRepo;

#[async_trait::async_trait]
impl StaffRequestRepository for NopRequestRepo {
    async fn save(&self, _: &StaffRequest) -> Result<(), DomainError> { Ok(()) }
    async fn find_by_id(&self, id: StaffRequestId) -> Result<StaffRequest, DomainError> {
        Err(DomainError::not_found(id.to_string()))
    }
    async fn find_by_user(&self, _: UserId) -> Result<Vec<StaffRequest>, DomainError> {
        Ok(vec![])
    }
    async fn find_pending(&self) -> Result<Vec<StaffRequest>, DomainError> { Ok(vec![]) }
    async fn find_pending_for_board(&self, _: &Slug) -> Result<Vec<StaffRequest>, DomainError> {
        Ok(vec![])
    }
    async fn update_status(
        &self, id: StaffRequestId, _: StaffRequestStatus, _: UserId, _: Option<String>
    ) -> Result<(), DomainError> {
        Err(DomainError::not_found(id.to_string()))
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_user(role: Role) -> User {
    User {
        id:            UserId(Uuid::new_v4()),
        username:      "alice".to_owned(),
        password_hash: PasswordHash::new("$argon2id$v=19$m=19456,t=2,p=1$x$y"),
        role,
        is_active:     true,
        created_at:    Utc::now(),
    }
}

fn user_app() -> axum::Router {
    let user_svc    = Arc::new(UserService::new(NopUserRepo, NopAuth, 3600));
    let request_svc = Arc::new(StaffRequestService::new(NopRequestRepo, NopUserRepo));
    user_routes(user_svc, request_svc)
}

/// Inject a `CurrentUser` extension directly — no middleware needed.
fn with_user(mut req: Request<Body>, role: Role) -> Request<Body> {
    let current = CurrentUser::from_claims(Claims {
        user_id:          UserId(Uuid::new_v4()),
        username:         "alice".to_owned(),
        role,
        owned_boards:     vec![],
        volunteer_boards: vec![],
        exp:              (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    });
    req.extensions_mut().insert(current);
    req
}

fn html_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::ACCEPT, "text/html")
        .body(Body::empty())
        .unwrap()
}

fn json_post(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn user_dashboard_returns_401_without_auth() {
    let resp = user_app()
        .oneshot(html_get("/user/dashboard"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_dashboard_returns_200_html_for_role_user() {
    let resp = user_app()
        .oneshot(with_user(html_get("/user/dashboard"), Role::User))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected HTML, got: {ct}");
}

#[tokio::test]
async fn user_dashboard_returns_200_for_admin_too() {
    // AnyAuthenticatedUser accepts all roles.
    let resp = user_app()
        .oneshot(with_user(html_get("/user/dashboard"), Role::Admin))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn submit_request_returns_401_without_auth() {
    let resp = user_app()
        .oneshot(json_post("/user/requests", r#"{"request_type":"become_janitor"}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn submit_request_board_create_returns_201() {
    let body = r#"{"request_type":"board_create","preferred_slug":"tech","preferred_title":"Technology","notes":"I want to run this board"}"#;
    let resp = user_app()
        .oneshot(with_user(json_post("/user/requests", body), Role::User))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn submit_request_become_volunteer_returns_201() {
    let body = r#"{"request_type":"become_volunteer","target_slug":"tech","notes":"I am active on this board"}"#;
    let resp = user_app()
        .oneshot(with_user(json_post("/user/requests", body), Role::User))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn submit_request_become_janitor_returns_201() {
    let body = r#"{"request_type":"become_janitor","notes":"I would like to help moderate"}"#;
    let resp = user_app()
        .oneshot(with_user(json_post("/user/requests", body), Role::User))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn submit_request_unknown_type_returns_400() {
    let body = r#"{"request_type":"become_president"}"#;
    let resp = user_app()
        .oneshot(with_user(json_post("/user/requests", body), Role::User))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn submit_volunteer_without_slug_returns_400() {
    let body = r#"{"request_type":"become_volunteer","notes":"no slug given"}"#;
    let resp = user_app()
        .oneshot(with_user(json_post("/user/requests", body), Role::User))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
