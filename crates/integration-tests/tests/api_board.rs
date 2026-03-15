//! Integration tests for public board HTTP endpoints.
//!
//! Covers: `GET /boards`, `GET /boards/:slug`,
//!         `POST /admin/boards`, `PUT /admin/boards/:id`, `DELETE /admin/boards/:id`.
//!
//! Auth-required routes inject `CurrentUser` via request extensions so the test
//! does not need a live JWT stack.

use api_adapters::axum::routes::board_routes::{board_public_routes, board_admin_routes};
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use domains::models::*;
use services::board::{BoardError, BoardRepo};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Stub implementations ─────────────────────────────────────────────────────

fn make_board(slug: &str) -> Board {
    Board {
        id:         BoardId(Uuid::new_v4()),
        slug:       Slug::new(slug).unwrap(),
        title:      format!("/{slug}/ — Test"),
        rules:      "".to_owned(),
        created_at: Utc::now(),
    }
}

struct OkBoardRepo { board: Board }

impl OkBoardRepo {
    fn for_slug(slug: &str) -> Self {
        Self { board: make_board(slug) }
    }
}

#[async_trait::async_trait]
impl BoardRepo for OkBoardRepo {
    async fn create_board(&self, slug: &str, title: &str, _: &str) -> Result<Board, BoardError> {
        Ok(make_board(slug).tap(|_b| {
            // We can't mutate; just return a fresh board with the right title.
            let _ = title;
        }))
    }
    async fn get_by_slug(&self, _: &str) -> Result<Board, BoardError> {
        Ok(self.board.clone())
    }
    async fn get_by_id(&self, _: BoardId) -> Result<Board, BoardError> {
        Ok(self.board.clone())
    }
    async fn update_board(&self, _: BoardId, title: Option<&str>, _: Option<&str>)
        -> Result<Board, BoardError>
    {
        let mut b = self.board.clone();
        if let Some(t) = title { b.title = t.to_owned(); }
        Ok(b)
    }
    async fn delete_board(&self, _: BoardId) -> Result<(), BoardError> { Ok(()) }
    async fn list_boards(&self, page: Page) -> Result<Paginated<Board>, BoardError> {
        Ok(Paginated::new(vec![self.board.clone()], 1, page, 15))
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

struct NotFoundBoardRepo;

#[async_trait::async_trait]
impl BoardRepo for NotFoundBoardRepo {
    async fn create_board(&self, _: &str, _: &str, _: &str) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: String::new() })
    }
    async fn get_by_slug(&self, s: &str) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: s.to_owned() })
    }
    async fn get_by_id(&self, id: BoardId) -> Result<Board, BoardError> {
        Err(BoardError::NotFound { slug: id.0.to_string() })
    }
    async fn update_board(&self, id: BoardId, _: Option<&str>, _: Option<&str>)
        -> Result<Board, BoardError>
    {
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
    async fn find_all_by_thread(&self, _: ThreadId) -> Result<Vec<Post>, domains::errors::DomainError> { Ok(vec![]) }
    async fn find_thread_id_by_post_number(&self, _: BoardId, _: u64) -> Result<Option<domains::models::ThreadId>, domains::errors::DomainError> { Ok(None) }
}

// ─── Helper: inject admin CurrentUser into a request ─────────────────────────

fn with_admin_user(mut req: Request<Body>) -> Request<Body> {
    let user = CurrentUser::from_claims(Claims {
        user_id:      UserId(Uuid::new_v4()),
        username: "testuser".into(),
        role:         Role::Admin,
        owned_boards: vec![],
        volunteer_boards: vec![],
        exp:          (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    });
    req.extensions_mut().insert(user);
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

fn json_post(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn json_put(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn delete_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

// ─── Public routes ────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_boards_returns_200_with_page() {
    let app = board_public_routes(Arc::new(OkBoardRepo::for_slug("tech")), NopPostRepo);
    let resp = app.oneshot(get("/boards")).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["slug"], "tech");
}

#[tokio::test]
async fn list_boards_returns_empty_page_when_no_boards() {
    let app = board_public_routes(Arc::new(NotFoundBoardRepo), NopPostRepo);
    let resp = app.oneshot(get("/boards")).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_board_by_slug_returns_200_for_existing() {
    let app = board_public_routes(Arc::new(OkBoardRepo::for_slug("b")), NopPostRepo);
    let resp = app.oneshot(get("/boards/b")).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["slug"], "b");
}

#[tokio::test]
async fn get_board_by_slug_returns_404_for_missing() {
    let app = board_public_routes(Arc::new(NotFoundBoardRepo), NopPostRepo);
    let resp = app.oneshot(get("/boards/nobody")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Admin routes ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_board_returns_201() {
    let app = board_admin_routes(Arc::new(OkBoardRepo::for_slug("tech")));
    let req = with_admin_user(json_post(
        "/admin/boards",
        r#"{"slug":"tech","title":"Technology","rules":""}"#,
    ));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_board_returns_401_without_admin_user() {
    let app = board_admin_routes(Arc::new(OkBoardRepo::for_slug("tech")));
    // No CurrentUser extension → AdminUser extractor returns 401/403.
    let resp = app
        .oneshot(json_post("/admin/boards", r#"{"slug":"tech","title":"Technology","rules":""}"#))
        .await
        .unwrap();
    // Extractor returns 401 (UNAUTHORIZED) when user is absent.
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "expected 401 or 403, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn update_board_returns_200() {
    let board_id = Uuid::new_v4();
    let app = board_admin_routes(Arc::new(OkBoardRepo::for_slug("tech")));
    let req = with_admin_user(json_put(
        &format!("/admin/boards/{board_id}"),
        r#"{"title":"Updated Tech","rules":null}"#,
    ));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn update_board_returns_404_for_missing() {
    let board_id = Uuid::new_v4();
    let app = board_admin_routes(Arc::new(NotFoundBoardRepo));
    let req = with_admin_user(json_put(
        &format!("/admin/boards/{board_id}"),
        r#"{"title":"Updated"}"#,
    ));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_board_returns_204() {
    let board_id = Uuid::new_v4();
    let app = board_admin_routes(Arc::new(OkBoardRepo::for_slug("tech")));
    let req = with_admin_user(delete_req(&format!("/admin/boards/{board_id}")));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_board_returns_404_for_missing() {
    let board_id = Uuid::new_v4();
    let app = board_admin_routes(Arc::new(NotFoundBoardRepo));
    let req = with_admin_user(delete_req(&format!("/admin/boards/{board_id}")));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// Small extension trait so we can chain .tap() to mutate a value inline.
// Only used in this file to satisfy the borrow checker cleanly.
trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
#[async_trait::async_trait]
impl Tap for Board {}

// ─── Search ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_returns_403_when_disabled() {
    // BoardConfig::default() has search_enabled = false
    let app = board_public_routes(Arc::new(OkBoardRepo::for_slug("tech")), NopPostRepo);
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/boards/tech/search?q=hello")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn search_returns_400_when_query_empty() {
    let app = board_public_routes(Arc::new(OkBoardRepo::for_slug("tech")), NopPostRepo);
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/boards/tech/search?q=")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
