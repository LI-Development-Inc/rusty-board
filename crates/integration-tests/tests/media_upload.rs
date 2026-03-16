//! Integration tests for the media upload path within post creation.
//!
//! These tests confirm that the `create_post` handler correctly parses
//! multipart file parts and routes them through `MediaProcessor` → `MediaStorage`.
//! Stubs make the pipeline observable without real S3 or image processing.
//!
//! See `api_post.rs` for the text-only post coverage. This file focuses on the
//! file-attachment code paths.

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
use domains::{errors::DomainError, models::*, ports::*};
use services::post::PostService;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use tower::ServiceExt;

// ─── Stubs ────────────────────────────────────────────────────────────────────

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
    async fn save(&self, p: &Post) -> Result<(PostId, u64), DomainError> { Ok((p.id, 1)) }
    async fn delete(&self, _: PostId) -> Result<(), DomainError> { Ok(()) }
    async fn delete_by_ip_in_thread(&self, _: &IpHash, _: ThreadId) -> Result<u64, DomainError> { Ok(0) }
    async fn save_attachments(&self, _: &[domains::models::Attachment]) -> Result<(), DomainError> { Ok(()) }
    async fn find_attachments_by_post_ids(&self, _: &[PostId]) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, DomainError> { Ok(std::collections::HashMap::new()) }
    async fn find_overboard(&self, p: Page) -> Result<Paginated<OverboardPost>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
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
    async fn set_pinned(&self, _: domains::models::PostId, _: bool) -> Result<(), domains::errors::DomainError> { Ok(()) }
    async fn find_oldest_unpinned_reply(&self, _: domains::models::ThreadId) -> Result<Option<domains::models::PostId>, domains::errors::DomainError> { Ok(None) }
    async fn find_attachment_by_hash(&self, _: &domains::models::ContentHash) -> Result<Option<domains::models::Attachment>, domains::errors::DomainError> { Ok(None) }
    async fn delete_by_id(&self, _: domains::models::PostId) -> Result<(), domains::errors::DomainError> { Ok(()) }
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
            closed:      false, cycle: false,
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
    async fn set_cycle(&self, _: domains::models::ThreadId, _: bool) -> Result<(), domains::errors::DomainError> { Ok(()) }
    async fn find_oldest_for_archive(&self, _: domains::models::BoardId, _: u32) -> Result<Vec<domains::models::Thread>, domains::errors::DomainError> { Ok(vec![]) }
    async fn count_by_board(&self, _: BoardId) -> Result<u32, DomainError> { Ok(0) }
    async fn prune_oldest(&self, _: BoardId, _: u32) -> Result<u32, DomainError> { Ok(0) }
    async fn delete(&self, _: ThreadId) -> Result<(), DomainError> { Ok(()) }
}

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

/// Tracking `MediaStorage` stub — records the keys of every file stored.
#[derive(Clone, Default)]
struct TrackingStorage {
    stored: Arc<Mutex<Vec<String>>>,
}

impl TrackingStorage {
    fn stored_keys(&self) -> Vec<String> {
        self.stored.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl MediaStorage for TrackingStorage {
    async fn store(&self, key: &MediaKey, _: Bytes, _: &str) -> Result<(), DomainError> {
        self.stored.lock().unwrap().push(key.0.clone());
        Ok(())
    }
    async fn get_url(&self, _: &MediaKey, _: std::time::Duration) -> Result<String, DomainError> {
        Ok("http://localhost/stub".to_owned())
    }
    async fn delete(&self, _: &MediaKey) -> Result<(), DomainError> { Ok(()) }
}

struct AllowAllRateLimiter;
#[async_trait::async_trait]
impl RateLimiter for AllowAllRateLimiter {
    async fn check(&self, _: &RateLimitKey) -> Result<RateLimitStatus, DomainError> {
        Ok(RateLimitStatus::Allowed { remaining: u32::MAX })
    }
    async fn increment(&self, _: &RateLimitKey, _: u32) -> Result<(), DomainError> { Ok(()) }
    async fn reset(&self, _: &RateLimitKey) -> Result<(), DomainError> { Ok(()) }
}

/// `MediaProcessor` that produces a minimal stub `ProcessedMedia`.
struct StubProcessor;
#[async_trait::async_trait]
impl MediaProcessor for StubProcessor {
    async fn process(&self, input: RawMedia) -> Result<ProcessedMedia, DomainError> {
        let key = MediaKey(format!("uploads/{}.bin", Uuid::new_v4()));
        let thumb_key = MediaKey(format!("thumbs/{}.bin", Uuid::new_v4()));
        let hash = ContentHash(crate::content_hash_from(&input.data));
        Ok(ProcessedMedia {
            original_key:   key,
            original_data:  input.data.clone(),
            thumbnail_key:  Some(thumb_key),
            thumbnail_data: Some(input.data),
            hash,
            size_kb:        0,
        })
    }
    fn accepts(&self, _: &mime::Mime) -> bool { true }
}

fn content_hash_from(data: &Bytes) -> String {
    // Simple deterministic pseudo-hash for tests; not SHA-256.
    format!("{:064x}", data.len() as u64 * 0xDEADBEEF)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn inject_ctx(mut req: Request<Body>, board_id: BoardId) -> Request<Body> {
    let board = Board {
        id:         board_id,
        slug:       Slug::new("tech").unwrap(),
        title:      "/tech/ — Technology".to_owned(),
        rules:      "".to_owned(),
        created_at: Utc::now(),
    };
    let config = BoardConfig {
        rate_limit_enabled:  false,
        spam_filter_enabled: false,
        duplicate_check:     false,
        allowed_mimes:       vec!["image/jpeg".to_owned(), "image/png".to_owned()],
        max_files:           3,
        ..BoardConfig::default()
    };
    req.extensions_mut().insert(ExtractedBoardConfig { slug: board.slug.clone(), board, board_id, config });
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1234)),
    ));
    req
}

/// Build a multipart body with one fake JPEG attachment and an optional text body.
fn multipart_with_file(body_text: &str, filename: &str, mime: &str, data: &[u8]) -> (String, Bytes) {
    let boundary = "filetestboundary";
    let ct = format!("multipart/form-data; boundary={boundary}");

    let mut mp = String::new();
    if !body_text.is_empty() {
        mp.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"body\"\r\n\r\n{body_text}\r\n"
        ));
    }
    mp.push_str(&format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n"
    ));

    let raw = Bytes::from(mp);
    let mut combined = raw.to_vec();
    combined.extend_from_slice(data);
    combined.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (ct, Bytes::from(combined))
}

fn file_post_req(slug: &str, body_text: &str, filename: &str, mime: &str, data: &[u8])
    -> Request<Body>
{
    let (ct, body) = multipart_with_file(body_text, filename, mime, data);
    let mut req = Request::builder()
        .method(Method::POST)
        .uri(format!("/board/{slug}/post"))
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .unwrap();
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1234)),
    ));
    req
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn post_with_jpeg_attachment_redirects_after_create() {
    // create_post is a browser-form endpoint: on success it redirects (303)
    // to the new post. The storage side-effect is what matters here.
    let board_id = BoardId(Uuid::new_v4());
    let storage = TrackingStorage::default();
    let storage_clone = storage.clone();

    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo,
        storage_clone, AllowAllRateLimiter, StubProcessor, String::new(),
    ));
    let app = post_routes(svc);

    // Minimal 1×1 JPEG (not real, but the stub processor accepts anything).
    let fake_jpeg = &[0xFF, 0xD8, 0xFF, 0xD9]; // SOI + EOI
    let req = inject_ctx(
        file_post_req("tech", "Check out this image", "photo.jpg", "image/jpeg", fake_jpeg),
        board_id,
    );
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers().get(axum::http::header::LOCATION).unwrap();
    assert!(location.to_str().unwrap().starts_with("/board/tech/thread/"));

    // The tracking storage should have received at least the original.
    let keys = storage.stored_keys();
    assert!(!keys.is_empty(), "expected storage to record at least one file, got none");
}

#[tokio::test]
async fn post_with_disallowed_mime_is_rejected() {
    let board_id = BoardId(Uuid::new_v4());

    // Config that only allows image/jpeg — a PDF should be rejected.
    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo,
        TrackingStorage::default(), AllowAllRateLimiter, StubProcessor, String::new(),
    ));
    let app = post_routes(svc);

    let mut req = inject_ctx(
        file_post_req("tech", "Here is a PDF", "doc.pdf", "application/pdf", b"%PDF-1.4"),
        board_id,
    );
    // Tighten the config to disallow PDFs.
    if let Some(ctx) = req.extensions_mut().get_mut::<ExtractedBoardConfig>() {
        ctx.config.allowed_mimes = vec!["image/jpeg".to_owned()];
    }

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status() == StatusCode::UNPROCESSABLE_ENTITY
            || resp.status() == StatusCode::BAD_REQUEST,
        "expected 422 or 400 for disallowed MIME, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn post_exceeding_max_files_is_rejected() {
    let board_id = BoardId(Uuid::new_v4());
    let svc = Arc::new(PostService::new(
        OkPostRepo, OkThreadRepo, NoBanRepo,
        TrackingStorage::default(), AllowAllRateLimiter, StubProcessor, String::new(),
    ));
    let app = post_routes(svc);

    // Build a config with max_files=1 then try to upload 2 files.
    let boundary = "maxfilesboundary";
    let ct = format!("multipart/form-data; boundary={boundary}");
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"body\"\r\n\r\nsome text\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"files\"; filename=\"a.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\nJPEG\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"files\"; filename=\"b.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\nJPEG\r\n\
         --{boundary}--\r\n"
    );

    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/board/tech/post")
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .unwrap();
    req = inject_ctx(req, board_id);
    if let Some(ctx) = req.extensions_mut().get_mut::<ExtractedBoardConfig>() {
        ctx.config.max_files = 1;
    }

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status() == StatusCode::UNPROCESSABLE_ENTITY
            || resp.status() == StatusCode::BAD_REQUEST,
        "expected 422 or 400 when too many files, got {}",
        resp.status()
    );
}
