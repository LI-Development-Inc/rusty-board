//! Integration tests for the board HTTP handlers.
//!
//! These tests build a minimal Axum router with a hand-rolled `BoardRepo` stub
//! and exercise the full request → handler → response pipeline without a real
//! database. Auth middleware is bypassed by injecting `CurrentUser` extensions
//! directly into test requests where needed.
//!
//! # Why hand-rolled stubs instead of mockall mocks?
//! Integration test files cannot import types from `#[cfg(test)]`-gated code in
//! other crates. The stubs below satisfy the `BoardRepo` trait with minimal code.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::Utc;
use domains::models::*;
use services::board::{BoardError, BoardRepo};
use tower::ServiceExt; // for `.oneshot()`
use uuid::Uuid;

// ─── Stub BoardRepo implementations ──────────────────────────────────────────

/// A `BoardRepo` stub that always returns a single fixed board.
struct StubBoardRepoOk {
    board: Board,
}

impl StubBoardRepoOk {
    fn new(slug: &str) -> Self {
        Self {
            board: Board {
                id:         BoardId(Uuid::new_v4()),
                slug:       Slug::new(slug).unwrap(),
                title:      "Test Board".to_owned(),
                rules:      "Be nice.".to_owned(),
                created_at: Utc::now(),
            },
        }
    }
}

#[async_trait::async_trait]
impl BoardRepo for StubBoardRepoOk {
    async fn create_board(&self, slug: &str, title: &str, _rules: &str) -> Result<Board, BoardError> {
        Ok(Board {
            id:         BoardId(Uuid::new_v4()),
            slug:       Slug::new(slug).unwrap(),
            title:      title.to_owned(),
            rules:      "".to_owned(),
            created_at: Utc::now(),
        })
    }

    async fn get_by_slug(&self, _slug: &str) -> Result<Board, BoardError> {
        Ok(self.board.clone())
    }

    async fn get_by_id(&self, _id: BoardId) -> Result<Board, BoardError> {
        Ok(self.board.clone())
    }

    async fn update_board(
        &self,
        _id: BoardId,
        title: Option<&str>,
        _rules: Option<&str>,
    ) -> Result<Board, BoardError> {
        let mut b = self.board.clone();
        if let Some(t) = title {
            b.title = t.to_owned();
        }
        Ok(b)
    }

    async fn delete_board(&self, _id: BoardId) -> Result<(), BoardError> {
        Ok(())
    }

    async fn list_boards(&self, page: Page) -> Result<Paginated<Board>, BoardError> {
        Ok(Paginated::new(vec![self.board.clone()], 1, page, 15))
    }

    async fn get_config(&self, _board_id: BoardId) -> Result<BoardConfig, BoardError> {
        Ok(BoardConfig::default())
    }

    async fn update_config(&self, _board_id: BoardId, config: BoardConfig) -> Result<BoardConfig, BoardError> {
        Ok(config)
    }
    async fn list_volunteers(&self, _: BoardId) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> { Ok(vec![]) }
    async fn add_volunteer_by_username(&self, _: BoardId, _: &str, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
}

/// A `BoardRepo` stub that always returns `NotFound`.
struct StubBoardRepoNotFound;

#[async_trait::async_trait]
impl BoardRepo for StubBoardRepoNotFound {
    async fn create_board(&self, _: &str, _: &str, _: &str) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: "test-board".into() })
    }
    async fn get_by_slug(&self, slug: &str) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: slug.to_owned() })
    }
    async fn get_by_id(&self, id: BoardId) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: id.0.to_string() })
    }
    async fn update_board(&self, id: BoardId, _: Option<&str>, _: Option<&str>) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: id.0.to_string() })
    }
    async fn delete_board(&self, id: BoardId) -> Result<(), BoardError> {
        Err(BoardError::NotFound { slug: id.0.to_string() })
    }
    async fn list_boards(&self, page: Page) -> Result<Paginated<Board>, BoardError> {
        Ok(Paginated::new(vec![], 0, page, 15))
    }
    async fn get_config(&self, id: BoardId) -> Result<BoardConfig, BoardError> {
        Err(BoardError::NotFound { slug: id.0.to_string() })
    }
    async fn update_config(&self, id: BoardId, _: BoardConfig) -> Result<BoardConfig, BoardError> {
        Err(BoardError::NotFound { slug: id.0.to_string() })
    }
    async fn list_volunteers(&self, _: BoardId) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> { Ok(vec![]) }
    async fn add_volunteer_by_username(&self, _: BoardId, _: &str, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: domains::models::UserId) -> Result<(), BoardError> { Ok(()) }
}


// ─── Minimal PostRepository stub for search route ────────────────────────────

#[derive(Clone)]
struct NopPostRepo;

#[async_trait::async_trait]
impl domains::ports::PostRepository for NopPostRepo {
    async fn find_by_id(&self, _: PostId) -> Result<Post, domains::errors::DomainError> { unimplemented!() }
    async fn find_by_thread(&self, _: ThreadId, _: Page) -> Result<Paginated<Post>, domains::errors::DomainError> { unimplemented!() }
    async fn find_recent_hashes(&self, _: BoardId, _: u32) -> Result<Vec<domains::models::ContentHash>, domains::errors::DomainError> { unimplemented!() }
    async fn find_by_ip_hash(&self, _: &domains::models::IpHash) -> Result<Vec<Post>, domains::errors::DomainError> { unimplemented!() }
    async fn save(&self, _: &Post) -> Result<(PostId, u64), domains::errors::DomainError> { unimplemented!() }
    async fn delete(&self, _: PostId) -> Result<(), domains::errors::DomainError> { unimplemented!() }
    async fn delete_by_ip_in_thread(&self, _: &domains::models::IpHash, _: domains::models::ThreadId) -> Result<u64, domains::errors::DomainError> { Ok(0) }
    async fn save_attachments(&self, _: &[domains::models::Attachment]) -> Result<(), domains::errors::DomainError> { Ok(()) }
    async fn find_attachments_by_post_ids(&self, _: &[PostId]) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, domains::errors::DomainError> { Ok(std::collections::HashMap::new()) }
    async fn find_overboard(&self, p: Page) -> Result<Paginated<domains::models::OverboardPost>, domains::errors::DomainError> { Ok(Paginated::new(vec![], 0, p, 15)) }
    async fn search_fulltext(&self, _: BoardId, _: &str, p: Page) -> Result<Paginated<Post>, domains::errors::DomainError> { Ok(Paginated::new(vec![], 0, p, 15)) }
}

// ─── Router factory helpers ───────────────────────────────────────────────────

fn board_public_router(repo: impl BoardRepo) -> Router {
    api_adapters::axum::routes::board_routes::board_public_routes(Arc::new(repo), NopPostRepo)
}

fn json_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_boards_returns_200_with_list() {
    let app = board_public_router(StubBoardRepoOk::new("tech"));
    let resp = app.oneshot(json_get("/boards")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["total"], 1);
    assert!(body["items"].as_array().unwrap().len() == 1);
}

#[tokio::test]
async fn get_boards_empty_returns_200_with_empty_list() {
    let app = board_public_router(StubBoardRepoNotFound);
    let resp = app.oneshot(json_get("/boards")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn show_board_returns_200_for_existing_slug() {
    let app = board_public_router(StubBoardRepoOk::new("tech"));
    let resp = app.oneshot(json_get("/boards/tech")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["slug"], "tech");
    assert_eq!(body["title"], "Test Board");
}

#[tokio::test]
async fn show_board_returns_404_for_missing_slug() {
    let app = board_public_router(StubBoardRepoNotFound);
    let resp = app.oneshot(json_get("/boards/nope")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn health_check_returns_200() {
    use api_adapters::axum::health::{DatabaseProbe, RedisProbe, HealthState};
    use std::sync::Arc;

    struct OkDb;
    #[async_trait::async_trait]
    impl DatabaseProbe for OkDb {
        async fn ping(&self) -> bool { true }
    }

    struct OkRedis;
    #[async_trait::async_trait]
    impl RedisProbe for OkRedis {
        async fn ping(&self) -> bool { true }
    }

    let health_state = HealthState {
        db:    Arc::new(OkDb),
        redis: Arc::new(OkRedis),
    };

    let app = Router::new()
        .route("/healthz", axum::routing::get(api_adapters::axum::health::health_check))
        .with_state(health_state);
    let resp = app.oneshot(json_get("/healthz")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
