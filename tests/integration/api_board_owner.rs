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

impl BoardRepo for OkConfigRepo {
    async fn create_board(&self, s: &str, t: &str, _: &str) -> Result<Board, BoardError> {
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
    let app = board_owner_routes(Arc::new(repo));

    let req = with_board_owner_context(get_config("tech"), &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Default bump limit is 300 per BoardConfig::default().
    assert!(json["bump_limit"].as_u64().is_some());
}

#[tokio::test]
async fn get_board_config_returns_401_without_auth() {
    let repo = OkConfigRepo::for_slug("tech");
    let board = repo.board.clone();
    let app = board_owner_routes(Arc::new(repo));

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
    let app = board_owner_routes(Arc::new(repo));

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
    let app = board_owner_routes(Arc::new(repo));

    // Only set forced_anon; all other fields should remain at their defaults.
    let body = r#"{"forced_anon":true}"#;
    let req = with_board_owner_context(put_config("b", body), &board);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["forced_anon"], true);
    // Default bump_limit is 300 — it should be unchanged.
    assert_eq!(json["bump_limit"], 300);
}
