//! Integration tests for board-owner config endpoints.
//!
//! Covers: `GET /board/:slug/config` and `PUT /board/:slug/config`.
//!
//! These routes require a board-config middleware extension (`ExtractedBoardConfig`)
//! which is normally injected by the `load_board_config` middleware in production.
//! In tests we inject it directly via request extensions.

use api_adapters::axum::{
    middleware::board_config::ExtractedBoardConfig,
    routes::board_owner_routes::board_owner_routes,
};
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use domains::{errors::DomainError, models::*};
use services::board::{BoardError, BoardRepo};
use services::staff_request::StaffRequestService;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Stub BoardRepo ───────────────────────────────────────────────────────────

struct OkConfigRepo { board: Board }

impl OkConfigRepo {
    fn for_slug(slug: &str) -> Self {
        Self {
            board: Board {
                id:         BoardId(Uuid::new_v4()),
                slug:       Slug::new(slug).unwrap(),
                title:      format!("/{slug}/ — Test"),
                rules:      "".to_owned(),
                created_at: Utc::now(),
            },
        }
    }
}

#[async_trait::async_trait]
impl BoardRepo for OkConfigRepo {
    async fn create_board(&self, _s: &str, _t: &str, _: &str) -> Result<Board, BoardError> {
        Ok(self.board.clone())
    }
    async fn get_by_slug(&self, _: &str) -> Result<Board, BoardError> { Ok(self.board.clone()) }
    async fn get_by_id(&self, _: BoardId) -> Result<Board, BoardError> { Ok(self.board.clone()) }
    async fn update_board(&self, _: BoardId, _: Option<&str>, _: Option<&str>)
        -> Result<Board, BoardError>
    {
        Ok(self.board.clone())
    }
    async fn delete_board(&self, _: BoardId) -> Result<(), BoardError> { Ok(()) }
    async fn list_boards(&self, p: Page) -> Result<Paginated<Board>, BoardError> {
        Ok(Paginated::new(vec![self.board.clone()], 1, p, 15))
    }
    async fn get_config(&self, _: BoardId) -> Result<BoardConfig, BoardError> {
        Ok(BoardConfig::default())
    }
    async fn update_config(&self, _: BoardId, c: BoardConfig) -> Result<BoardConfig, BoardError> {
        Ok(c)
    }
    async fn list_volunteers(&self, _: BoardId) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> { Ok(vec![]) }
    async fn add_volunteer_by_username(&self, _: BoardId, _: &str, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
}

// ─── Stub StaffRequestRepository ─────────────────────────────────────────────

struct NopRequestRepo;

#[async_trait::async_trait]
impl domains::ports::StaffRequestRepository for NopRequestRepo {
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

struct NopUserRepo;

#[async_trait::async_trait]
impl domains::ports::UserRepository for NopUserRepo {
    async fn find_by_id(&self, _: UserId) -> Result<User, DomainError> {
        Err(DomainError::not_found("user"))
    }
    async fn find_by_username(&self, _: &str) -> Result<User, DomainError> {
        Err(DomainError::not_found("user"))
    }
    async fn find_all(&self, p: Page) -> Result<Paginated<User>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
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

fn make_request_svc() -> Arc<StaffRequestService<NopRequestRepo, NopUserRepo>> {
    Arc::new(StaffRequestService::new(NopRequestRepo, NopUserRepo))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Inject a `CurrentUser` (admin) and an `ExtractedBoardConfig` into request extensions.
/// These are normally injected by the auth middleware and board-config middleware
/// respectively; in tests we bypass both.
fn with_board_owner_context(mut req: Request<Body>, board: &Board) -> Request<Body> {
    let board_id = board.id;

    // Admin user who owns this board.
    let user = CurrentUser::from_claims(Claims {
        user_id:      UserId(Uuid::new_v4()),
        username: "testuser".into(),
        role:         Role::Admin,
        owned_boards: vec![board_id],
        volunteer_boards: vec![],
        exp:          (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    });
    req.extensions_mut().insert(user);

    // Board-config context (normally set by the load_board_config middleware).
    let ctx = ExtractedBoardConfig {
        board:    board.clone(),
        board_id,
        config:   BoardConfig::default(),
        slug:     board.slug.clone(),
    };
    req.extensions_mut().insert(ctx);

    req
}

fn get_config(slug: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(format!("/board/{slug}/config"))
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap()
}

fn put_config(slug: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(format!("/board/{slug}/config"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_board_config_returns_200() {
    let repo = OkConfigRepo::for_slug("tech");
    let board = repo.board.clone();
    let app = board_owner_routes(Arc::new(repo), make_request_svc());

    let req = with_board_owner_context(get_config("tech"), &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Default bump limit is 500 per BoardConfig::default().
    assert!(json["bump_limit"].as_u64().is_some());
}

#[tokio::test]
async fn get_board_config_returns_401_without_auth() {
    let repo = OkConfigRepo::for_slug("tech");
    let _board = repo.board.clone();
    let app = board_owner_routes(Arc::new(repo), make_request_svc());

    // No CurrentUser in extensions → AuthenticatedUser extractor returns 401.
    let resp = app.oneshot(get_config("tech")).await.unwrap();
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "expected 401 or 403, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn update_board_config_returns_200_and_new_config() {
    let repo = OkConfigRepo::for_slug("tech");
    let board = repo.board.clone();
    let app = board_owner_routes(Arc::new(repo), make_request_svc());

    let body = r#"{"bump_limit":100,"rate_limit_posts":3}"#;
    let req = with_board_owner_context(put_config("tech", body), &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["bump_limit"], 100);
    assert_eq!(json["rate_limit_posts"], 3);
}

#[tokio::test]
async fn update_board_config_partial_update_leaves_other_fields_unchanged() {
    let repo = OkConfigRepo::for_slug("b");
    let board = repo.board.clone();
    let app = board_owner_routes(Arc::new(repo), make_request_svc());

    // Only set forced_anon; all other fields should remain at their defaults.
    let body = r#"{"forced_anon":true}"#;
    let req = with_board_owner_context(put_config("b", body), &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["forced_anon"], true);
    // Default bump_limit is 500 — it should be unchanged.
    assert_eq!(json["bump_limit"], 500);
}

// ─── Volunteer endpoints ──────────────────────────────────────────────────────

fn with_board_owner_user(mut req: Request<Body>, board: &Board) -> Request<Body> {
    let user = CurrentUser::from_claims(Claims {
        user_id:      UserId(Uuid::new_v4()),
        username: "testuser".into(),
        role:         Role::BoardOwner,
        owned_boards: vec![board.id],
        volunteer_boards: vec![],
        exp:          (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    });
    req.extensions_mut().insert(user);
    let ctx = ExtractedBoardConfig {
        board:    board.clone(),
        board_id: board.id,
        config:   BoardConfig::default(),
        slug:     board.slug.clone(),
    };
    req.extensions_mut().insert(ctx);
    req
}

#[tokio::test]
async fn list_volunteers_returns_200_empty() {
    let repo  = OkConfigRepo::for_slug("tech");
    let board = repo.board.clone();
    let app   = board_owner_routes(Arc::new(repo), make_request_svc());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/board/tech/volunteers")
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap();
    let req = with_board_owner_user(req, &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn add_volunteer_returns_201() {
    let repo  = OkConfigRepo::for_slug("tech");
    let board = repo.board.clone();
    let app   = board_owner_routes(Arc::new(repo), make_request_svc());

    let req = Request::builder()
        .method(Method::POST)
        .uri("/board/tech/volunteers")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"testuser"}"#))
        .unwrap();
    let req = with_board_owner_user(req, &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn list_volunteers_returns_401_without_auth() {
    let repo = OkConfigRepo::for_slug("tech");
    let app  = board_owner_routes(Arc::new(repo), make_request_svc());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/board/tech/volunteers")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "expected 401/403, got {}", resp.status()
    );
}
