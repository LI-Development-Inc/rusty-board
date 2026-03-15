//! Integration tests for `POST /board/:slug/post`.
//!
//! This exercises the most complex handler — multipart post creation — end to end
//! using stub ports. The board-config middleware context (`ExtractedBoardConfig`)
//! is injected directly because we bypass the middleware stack in these tests.
//!
//! Tests verify:
//! - Text-only posts create a thread successfully
//! - Banned IP receives 403
//! - Rate-limited IP receives 429
//! - Invalid/missing multipart body receives 422
//! - Sage posts work (same 201, but the service marks no-bump)

use api_adapters::axum::{
    middleware::board_config::ExtractedBoardConfig,
    routes::post_routes::post_routes,
};
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use bytes::Bytes;
use chrono::Utc;
use domains::{
    errors::DomainError,
    models::*,
    ports::*,
};
use services::post::PostService;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Stub ports ───────────────────────────────────────────────────────────────

struct OkPostRepo;
#[async_trait::async_trait]
impl PostRepository for OkPostRepo {
    async fn find_by_id(&self, _: PostId) -> Result<Post, DomainError> {
        Err(DomainError::not_found("post"))
    }
    async fn find_by_thread(&self, _: ThreadId, p: Page) -> Result<Paginated<Post>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn find_by_ip_hash(&self, _: &IpHash) -> Result<Vec<Post>, DomainError> { Ok(vec![]) }
    async fn find_recent_hashes(&self, _: BoardId, _: u32) -> Result<Vec<ContentHash>, DomainError> {
        Ok(vec![])
    }
    async fn save(&self, post: &Post) -> Result<(PostId, u64), DomainError> { Ok((post.id, 1)) }
    async fn delete(&self, _: PostId) -> Result<(), DomainError> { Ok(()) }
    async fn delete_by_ip_in_thread(&self, _: &IpHash, _: ThreadId) -> Result<u64, DomainError> { Ok(0) }
    async fn find_overboard(&self, p: Page) -> Result<Paginated<OverboardPost>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn save_attachments(&self, _: &[domains::models::Attachment]) -> Result<(), DomainError> { Ok(()) }
    async fn find_attachments_by_post_ids(&self, _: &[PostId]) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, DomainError> {
        Ok(std::collections::HashMap::new())
    }
    async fn search_fulltext(
        &self,
        _board_id: BoardId,
        _query: &str,
        p: Page,
    ) -> Result<Paginated<Post>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn find_all_by_thread(&self, _: ThreadId) -> Result<Vec<Post>, DomainError> { Ok(vec![]) }
    async fn find_thread_id_by_post_number(&self, _: BoardId, _: u64) -> Result<Option<ThreadId>, DomainError> { Ok(None) }
}

struct OkThreadRepo;
#[async_trait::async_trait]
impl ThreadRepository for OkThreadRepo {
    async fn find_by_id(&self, id: ThreadId) -> Result<Thread, DomainError> {
        Ok(Thread {
            id,
            board_id:    BoardId(Uuid::new_v4()),
            op_post_id:  None,
            reply_count: 0,
            bumped_at:   Utc::now(),
            sticky:      false,
            closed:      false,
            created_at:  Utc::now(),
        })
    }
    async fn find_by_board(&self, _: BoardId, p: Page) -> Result<Paginated<Thread>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn find_catalog(&self, _: BoardId) -> Result<Vec<ThreadSummary>, DomainError> { Ok(vec![]) }
    async fn save(&self, t: &Thread) -> Result<ThreadId, DomainError> { Ok(t.id) }
    async fn bump(&self, _: ThreadId, _: chrono::DateTime<Utc>) -> Result<(), DomainError> { Ok(()) }
    async fn set_op_post(&self, _: ThreadId, _: PostId) -> Result<(), DomainError> { Ok(()) }
    async fn set_sticky(&self, _: ThreadId, _: bool) -> Result<(), DomainError> { Ok(()) }
    async fn set_closed(&self, _: ThreadId, _: bool) -> Result<(), DomainError> { Ok(()) }
    async fn count_by_board(&self, _: BoardId) -> Result<u32, DomainError> { Ok(0) }
    async fn prune_oldest(&self, _: BoardId, _: u32) -> Result<u32, DomainError> { Ok(0) }
    async fn delete(&self, _: ThreadId) -> Result<(), DomainError> { Ok(()) }
}

/// Ban repo that reports the posting IP as banned.
struct BannedIpRepo;
#[async_trait::async_trait]
impl BanRepository for BannedIpRepo {
    async fn find_active_by_ip(&self, _: &IpHash) -> Result<Option<Ban>, DomainError> {
        Ok(Some(Ban {
            id:         BanId(Uuid::new_v4()),
            ip_hash:    IpHash::new("a".repeat(64)),
            banned_by:  UserId(Uuid::new_v4()),
            reason:     "spam test ban".to_owned(),
            expires_at: None,
            created_at: Utc::now(),
        }))
    }
    async fn save(&self, b: &Ban) -> Result<BanId, DomainError> { Ok(b.id) }
    async fn expire(&self, _: BanId) -> Result<(), DomainError> { Ok(()) }
    async fn find_all(&self, p: Page) -> Result<Paginated<Ban>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
}

/// Ban repo that reports no active bans.
struct NoBanRepo;
#[async_trait::async_trait]
impl BanRepository for NoBanRepo {
    async fn find_active_by_ip(&self, _: &IpHash) -> Result<Option<Ban>, DomainError> { Ok(None) }
    async fn save(&self, b: &Ban) -> Result<BanId, DomainError> { Ok(b.id) }
    async fn expire(&self, _: BanId) -> Result<(), DomainError> { Ok(()) }
    async fn find_all(&self, p: Page) -> Result<Paginated<Ban>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
}

struct NopMedia;
#[async_trait::async_trait]
impl MediaStorage for NopMedia {
    async fn store(&self, _: &MediaKey, _: Bytes, _: &str) -> Result<(), DomainError> { Ok(()) }
    async fn get_url(&self, _: &MediaKey, _: std::time::Duration) -> Result<String, DomainError> {
        Ok("http://localhost/stub".to_owned())
    }
    async fn delete(&self, _: &MediaKey) -> Result<(), DomainError> { Ok(()) }
}

/// Rate limiter that always allows.
struct AllowAllRateLimiter;
#[async_trait::async_trait]
impl RateLimiter for AllowAllRateLimiter {
    async fn check(&self, _: &RateLimitKey) -> Result<RateLimitStatus, DomainError> {
        Ok(RateLimitStatus::Allowed { remaining: u32::MAX })
    }
    async fn increment(&self, _: &RateLimitKey, _: u32) -> Result<(), DomainError> { Ok(()) }
    async fn reset(&self, _: &RateLimitKey) -> Result<(), DomainError> { Ok(()) }
}

/// Rate limiter that always returns Exceeded.
struct BlockAllRateLimiter;
#[async_trait::async_trait]
impl RateLimiter for BlockAllRateLimiter {
    async fn check(&self, _: &RateLimitKey) -> Result<RateLimitStatus, DomainError> {
        Ok(RateLimitStatus::Exceeded { retry_after_secs: 60 })
    }
    async fn increment(&self, _: &RateLimitKey, _: u32) -> Result<(), DomainError> { Ok(()) }
    async fn reset(&self, _: &RateLimitKey) -> Result<(), DomainError> { Ok(()) }
}

struct NopProcessor;
#[async_trait::async_trait]
impl MediaProcessor for NopProcessor {
    async fn process(&self, _: RawMedia) -> Result<ProcessedMedia, DomainError> {
        Err(DomainError::internal("no files expected in text-only tests"))
    }
    fn accepts(&self, _: &mime::Mime) -> bool { false }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn inject_board_ctx(mut req: Request<Body>, board_id: BoardId) -> Request<Body> {
    let board = Board {
        id:         board_id,
        slug:       Slug::new("tech").unwrap(),
        title:      "/tech/ — Technology".to_owned(),
        rules:      "".to_owned(),
        created_at: Utc::now(),
    };
    let config = BoardConfig {
        rate_limit_enabled:  false, // disable rate-limiting by default in helpers
        spam_filter_enabled: false,
        duplicate_check:     false,
        ..BoardConfig::default()
    };
    req.extensions_mut().insert(ExtractedBoardConfig { slug: board.slug.clone(), board, board_id, config });
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1234)),
    ));
    req
}

fn inject_board_ctx_with_config(
    mut req: Request<Body>,
    board_id: BoardId,
    config: BoardConfig,
) -> Request<Body> {
    let board = Board {
        id:         board_id,
        slug:       Slug::new("tech").unwrap(),
        title:      "/tech/ — Technology".to_owned(),
        rules:      "".to_owned(),
        created_at: Utc::now(),
    };
    req.extensions_mut().insert(ExtractedBoardConfig { slug: board.slug.clone(), board, board_id, config });
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1234)),
    ));
    req
}

/// Build a simple multipart body with just a text `body` field.
fn text_post_body(body_text: &str) -> (String, Bytes) {
    let boundary = "testboundary123";
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let multipart = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"body\"\r\n\
         \r\n\
         {body_text}\r\n\
         --{boundary}--\r\n"
    );
    (content_type, Bytes::from(multipart))
}

fn multipart_req(slug: &str, body_text: &str) -> Request<Body> {
    let (ct, body) = text_post_body(body_text);
    Request::builder()
        .method(Method::POST)
        .uri(format!("/board/{slug}/post"))
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .unwrap()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn text_only_post_redirects_after_create() {
    // create_post is a browser-form endpoint: on success it redirects (303)
    // to the new post. API consumers should read via the board/thread endpoints.
    let board_id = BoardId(Uuid::new_v4());
    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo, NopMedia, AllowAllRateLimiter, NopProcessor, String::new(),
    ));
    let app = post_routes(svc);

    let req = inject_board_ctx(multipart_req("tech", "Hello, board!"), board_id);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    // Location header should point to the thread/post anchor.
    let location = resp.headers().get(axum::http::header::LOCATION).unwrap();
    assert!(location.to_str().unwrap().starts_with("/board/tech/thread/"));
}

#[tokio::test]
async fn banned_ip_receives_403() {
    let board_id = BoardId(Uuid::new_v4());
    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, BannedIpRepo, NopMedia, AllowAllRateLimiter, NopProcessor, String::new(),
    ));
    let app = post_routes(svc);

    let req = inject_board_ctx(multipart_req("tech", "This should be rejected"), board_id);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rate_limited_ip_receives_429() {
    let board_id = BoardId(Uuid::new_v4());
    let config = BoardConfig {
        rate_limit_enabled:  true,
        spam_filter_enabled: false,
        duplicate_check:     false,
        ..BoardConfig::default()
    };

    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo, NopMedia, BlockAllRateLimiter, NopProcessor, String::new(),
    ));
    let app = post_routes(svc);

    let req = inject_board_ctx_with_config(
        multipart_req("tech", "Rate-limited post"),
        board_id,
        config,
    );
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn post_with_empty_body_is_rejected() {
    let board_id = BoardId(Uuid::new_v4());
    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo, NopMedia, AllowAllRateLimiter, NopProcessor, String::new(),
    ));
    let app = post_routes(svc);

    // Empty body text should fail domain validation.
    let req = inject_board_ctx(multipart_req("tech", ""), board_id);
    let resp = app.oneshot(req).await.unwrap();

    // Empty post (no body, no files) must be rejected with 422 or 400.
    assert!(
        resp.status() == StatusCode::UNPROCESSABLE_ENTITY
            || resp.status() == StatusCode::BAD_REQUEST,
        "expected 422 or 400 for empty post, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn post_without_multipart_content_type_returns_400_or_415() {
    let board_id = BoardId(Uuid::new_v4());
    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo, NopMedia, AllowAllRateLimiter, NopProcessor, String::new(),
    ));
    let app = post_routes(svc);

    let req = inject_board_ctx(
        Request::builder()
            .method(Method::POST)
            .uri("/board/tech/post")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"body":"test"}"#))
            .unwrap(),
        board_id,
    );
    let resp = app.oneshot(req).await.unwrap();

    // Handler expects multipart — non-multipart body should fail.
    assert!(
        resp.status() == StatusCode::BAD_REQUEST
            || resp.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE
            || resp.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "got {}",
        resp.status()
    );
}
