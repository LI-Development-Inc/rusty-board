//! Integration tests for admin HTTP endpoints.
//!
//! Covers: user management CRUD, board owner assignment/removal, audit log,
//! and the admin dashboard. All tests use hand-rolled stubs for `UserRepository`
//! and `AuthProvider`; no real database or JWT stack is needed.

use api_adapters::axum::routes::admin_routes::admin_routes;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use domains::{errors::DomainError, models::*, ports::*};
use services::board::{BoardError, BoardRepo};
use services::staff_request::StaffRequestService;
use services::user::UserService;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Stubs ────────────────────────────────────────────────────────────────────

struct OkUserRepo;

impl OkUserRepo {
    fn stub_user(id: UserId) -> User {
        User {
            id,
            username:      "testmod".to_owned(),
            password_hash: PasswordHash::new("hashed"),
            role:          Role::Janitor,
            is_active:     true,
            created_at:    Utc::now(),
        }
    }
}

#[async_trait::async_trait]
impl UserRepository for OkUserRepo {
    async fn find_by_id(&self, id: UserId) -> Result<User, DomainError> {
        Ok(Self::stub_user(id))
    }
    async fn find_by_username(&self, _: &str) -> Result<User, DomainError> {
        Ok(Self::stub_user(UserId(Uuid::new_v4())))
    }
    async fn find_all(&self, page: Page) -> Result<Paginated<User>, DomainError> {
        Ok(Paginated::new(vec![Self::stub_user(UserId(Uuid::new_v4()))], 1, page, 15))
    }
    async fn save(&self, _: &User) -> Result<(), DomainError> { Ok(()) }
    async fn deactivate(&self, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn find_owned_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn find_volunteer_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn add_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn add_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
}

struct OkAuth;

#[async_trait::async_trait]
impl AuthProvider for OkAuth {
    async fn create_token(&self, claims: &Claims) -> Result<Token, DomainError> {
        Ok(Token::new(format!("fake.{}", claims.user_id.0)))
    }
    async fn verify_token(&self, _: &Token) -> Result<Claims, DomainError> {
        Err(DomainError::auth())
    }
    async fn hash_password(&self, p: &str) -> Result<PasswordHash, DomainError> {
        Ok(PasswordHash::new(format!("hashed:{p}")))
    }
    async fn verify_password(&self, _: &str, _: &PasswordHash) -> Result<(), DomainError> {
        Ok(())
    }
}

// ─── Board stub ──────────────────────────────────────────────────────────────

/// Minimal `BoardRepo` stub — always returns an empty board list.
///
/// Used by `admin_routes` (which now requires a board service for the dashboard)
/// in tests that exercise user-management endpoints and don't care about boards.
#[derive(Clone)]
struct NoBoardRepo;

#[async_trait::async_trait]
impl BoardRepo for NoBoardRepo {
    async fn create_board(&self, _slug: &str, _title: &str, _rules: &str) -> Result<domains::models::Board, services::board::BoardError> {
        unimplemented!("not needed in admin tests")
    }
    async fn get_by_slug(&self, _slug: &str) -> Result<domains::models::Board, services::board::BoardError> {
        unimplemented!()
    }
    async fn get_by_id(&self, _id: domains::models::BoardId) -> Result<domains::models::Board, services::board::BoardError> {
        unimplemented!()
    }
    async fn update_board(&self, _id: domains::models::BoardId, _title: Option<&str>, _rules: Option<&str>) -> Result<domains::models::Board, services::board::BoardError> {
        unimplemented!()
    }
    async fn delete_board(&self, _id: domains::models::BoardId) -> Result<(), services::board::BoardError> {
        unimplemented!()
    }
    async fn list_boards(&self, _page: domains::models::Page) -> Result<domains::models::Paginated<domains::models::Board>, services::board::BoardError> {
        Ok(domains::models::Paginated::new(vec![], 0, _page, 15))
    }
    async fn get_config(&self, _id: domains::models::BoardId) -> Result<domains::models::BoardConfig, services::board::BoardError> {
        unimplemented!()
    }
    async fn update_config(&self, _id: domains::models::BoardId, _config: domains::models::BoardConfig) -> Result<domains::models::BoardConfig, services::board::BoardError> {
        unimplemented!()
    }
    async fn list_volunteers(&self, _: BoardId) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> { Ok(vec![]) }
    async fn add_volunteer_by_username(&self, _: BoardId, _: &str, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
}

// ─── Staff request stub ───────────────────────────────────────────────────────

struct NopRequestRepo;

#[async_trait::async_trait]
impl StaffRequestRepository for NopRequestRepo {
    async fn save(&self, _: &StaffRequest) -> Result<(), DomainError> { Ok(()) }
    async fn find_by_id(&self, id: StaffRequestId) -> Result<StaffRequest, DomainError> {
        Err(DomainError::not_found(id.to_string()))
    }
    async fn find_by_user(&self, _: UserId) -> Result<Vec<StaffRequest>, DomainError> { Ok(vec![]) }
    async fn find_pending(&self) -> Result<Vec<StaffRequest>, DomainError> { Ok(vec![]) }
    async fn find_pending_for_board(&self, _: &Slug) -> Result<Vec<StaffRequest>, DomainError> { Ok(vec![]) }
    async fn update_status(
        &self, id: StaffRequestId, _: StaffRequestStatus, _: UserId, _: Option<String>
    ) -> Result<(), DomainError> {
        Err(DomainError::not_found(id.to_string()))
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn app() -> axum::Router {
    let svc        = Arc::new(UserService::new(OkUserRepo, OkAuth, 3600));
    let board_svc  = Arc::new(NoBoardRepo);
    let request_svc = Arc::new(StaffRequestService::new(NopRequestRepo, OkUserRepo));
    admin_routes(svc, board_svc, request_svc)
}

fn with_admin(mut req: Request<Body>) -> Request<Body> {
    req.extensions_mut().insert(CurrentUser::from_claims(Claims {
        user_id:      UserId(Uuid::new_v4()),
        username: "testuser".into(),
        role:         Role::Admin,
        owned_boards: vec![],
        volunteer_boards: vec![],
        exp:          (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    }));
    req
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn post_empty(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

// ─── User management ─────────────────────────────────────────────────────────

#[tokio::test]
async fn list_users_returns_200_with_users() {
    let resp = app().oneshot(with_admin(get("/admin/users"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"], 1);
}

#[tokio::test]
async fn list_users_returns_401_without_auth() {
    let resp = app().oneshot(get("/admin/users")).await.unwrap();
    assert!(resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_user_returns_201() {
    let body = r#"{"username":"newmod","password":"strongpassword123","role":"janitor"}"#;
    let resp = app()
        .oneshot(with_admin(post_json("/admin/users", body)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_user_returns_422_on_invalid_body() {
    // Missing `password` field.
    let resp = app()
        .oneshot(with_admin(post_json(
            "/admin/users",
            r#"{"username":"newmod","role":"janitor"}"#,
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn deactivate_user_returns_204() {
    let user_id = Uuid::new_v4();
    let resp = app()
        .oneshot(with_admin(post_empty(&format!("/admin/users/{user_id}/deactivate"))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// ─── Board owner management ───────────────────────────────────────────────────

#[tokio::test]
async fn add_board_owner_returns_204() {
    let board_id = Uuid::new_v4();
    let user_id  = Uuid::new_v4();
    let body = format!(r#"{{"user_id":"{user_id}"}}"#);
    let resp = app()
        .oneshot(with_admin(post_json(
            &format!("/admin/boards/{board_id}/owners"),
            &body,
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn remove_board_owner_returns_204() {
    let board_id = Uuid::new_v4();
    let user_id  = Uuid::new_v4();
    let resp = app()
        .oneshot(with_admin(delete(&format!(
            "/admin/boards/{board_id}/owners/{user_id}"
        ))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// ─── Audit log ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_audit_log_returns_200_with_empty_list() {
    // v1.0: stub returns empty list; v1.1 will wire the real audit repo.
    let resp = app()
        .oneshot(with_admin(get("/admin/audit")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"], 0);
}

// ─── Admin dashboard ─────────────────────────────────────────────────────────

#[tokio::test]
async fn admin_dashboard_returns_200_html() {
    let resp = app()
        .oneshot(with_admin(
            Request::builder()
                .method(Method::GET)
                .uri("/admin/dashboard")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected HTML response, got: {ct}");
}

#[tokio::test]
async fn admin_dashboard_returns_401_without_auth() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/admin/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN);
}
