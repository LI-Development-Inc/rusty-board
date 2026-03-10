//! Integration tests for moderation HTTP endpoints.
//!
//! Covers: flag queue, flag resolution, post/thread delete,
//!         sticky/close toggles, ban creation, ban expiry, and `POST .../flag`.
//!
//! All tests use stub implementations of the six port traits required by
//! `ModerationService`. No real database or Redis is used.

use api_adapters::axum::routes::moderation_routes::moderation_routes;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use domains::{errors::DomainError, models::*, ports::*};
use services::board::{BoardError, BoardRepo};
use services::moderation::ModerationService;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ─── No-op stubs (all succeed, return minimal data) ──────────────────────────

struct NopBan;
#[async_trait::async_trait]
impl BanRepository for NopBan {
    async fn find_active_by_ip(&self, _: &IpHash) -> Result<Option<Ban>, DomainError> { Ok(None) }
    async fn save(&self, ban: &Ban) -> Result<BanId, DomainError> { Ok(ban.id) }
    async fn expire(&self, _: BanId) -> Result<(), DomainError> { Ok(()) }
    async fn find_all(&self, page: Page) -> Result<Paginated<Ban>, DomainError> {
        Ok(Paginated::new(vec![], 0, page, 15))
    }
}

struct NopPost;
#[async_trait::async_trait]
impl PostRepository for NopPost {
    async fn find_by_id(&self, _: PostId) -> Result<Post, DomainError> {
        Ok(Post {
            id:          PostId(Uuid::new_v4()),
            thread_id:   ThreadId(Uuid::new_v4()),
            body:        "stub".to_owned(),
            ip_hash:     IpHash::new("a".repeat(64)),
            name:        None,
            email:       None,
            tripcode:    None,
            post_number: 1,
            created_at:  Utc::now(),
        })
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
}

struct NopThread;
#[async_trait::async_trait]
impl ThreadRepository for NopThread {
    async fn find_by_id(&self, id: ThreadId) -> Result<Thread, DomainError> {
        Ok(Thread {
            id,
            board_id:    BoardId(Uuid::new_v4()),
            op_post_id:  Some(PostId(Uuid::new_v4())),  // required by create_flag
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

struct NopFlag {
    flag: Flag,
}

impl NopFlag {
    fn new() -> Self {
        Self {
            flag: Flag {
                id:               FlagId(Uuid::new_v4()),
                post_id:          PostId(Uuid::new_v4()),
                reason:           "spam".to_owned(),
                reporter_ip_hash: IpHash::new("a".repeat(64)),
                status:           FlagStatus::Pending,
                resolved_by:      None,
                created_at:       Utc::now(),
            },
        }
    }
}

#[async_trait::async_trait]
impl FlagRepository for NopFlag {
    async fn find_by_id(&self, _: FlagId) -> Result<Flag, DomainError> {
        Ok(self.flag.clone())
    }
    async fn find_pending(&self, page: Page) -> Result<Paginated<Flag>, DomainError> {
        Ok(Paginated::new(vec![self.flag.clone()], 1, page, 15))
    }
    async fn save(&self, f: &Flag) -> Result<FlagId, DomainError> { Ok(f.id) }
    async fn resolve(&self, _: FlagId, _: FlagResolution, _: UserId)
        -> Result<(), DomainError>
    {
        Ok(())
    }
}

struct NopAudit;
#[async_trait::async_trait]
impl AuditRepository for NopAudit {
    async fn record(&self, _: &AuditEntry) -> Result<(), DomainError> { Ok(()) }
    async fn find_recent(&self, _: u32) -> Result<Vec<AuditEntry>, DomainError> { Ok(vec![]) }
    async fn find_by_actor(&self, _: UserId, p: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn find_by_target(&self, _: uuid::Uuid, p: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn find_all(&self, p: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn find_by_board(&self, _: BoardId, p: Page) -> Result<Paginated<AuditEntry>, DomainError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
}

struct NopUser;
#[async_trait::async_trait]
impl UserRepository for NopUser {
    async fn find_by_id(&self, id: UserId) -> Result<User, DomainError> {
        Ok(User {
            id,
            username:      "mod".to_owned(),
            password_hash: PasswordHash::new("x"),
            role:          Role::Janitor,
            is_active:     true,
            created_at:    Utc::now(),
        })
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
    async fn add_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn add_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
}

// ─── Board stub (dashboards need a board service) ────────────────────────────

struct NopBoardRepo;

#[async_trait::async_trait]
impl BoardRepo for NopBoardRepo {
    async fn create_board(&self, _: &str, _: &str, _: &str) -> Result<domains::models::Board, BoardError> {
        unimplemented!()
    }
    async fn get_by_slug(&self, _: &str) -> Result<domains::models::Board, BoardError> { unimplemented!() }
    async fn get_by_id(&self, _: BoardId) -> Result<domains::models::Board, BoardError> { unimplemented!() }
    async fn update_board(&self, _: BoardId, _: Option<&str>, _: Option<&str>) -> Result<domains::models::Board, BoardError> { unimplemented!() }
    async fn delete_board(&self, _: BoardId) -> Result<(), BoardError> { unimplemented!() }
    async fn list_boards(&self, p: Page) -> Result<Paginated<domains::models::Board>, BoardError> {
        Ok(Paginated::new(vec![], 0, p, 15))
    }
    async fn get_config(&self, _: BoardId) -> Result<BoardConfig, BoardError> { unimplemented!() }
    async fn update_config(&self, _: BoardId, c: BoardConfig) -> Result<BoardConfig, BoardError> { Ok(c) }
    async fn list_volunteers(&self, _: BoardId) -> Result<Vec<(UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> { Ok(vec![]) }
    async fn add_volunteer_by_username(&self, _: BoardId, _: &str, _: UserId) -> Result<(), BoardError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: UserId) -> Result<(), BoardError> { Ok(()) }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn mod_app() -> axum::Router {
    let svc = Arc::new(ModerationService::new(
        NopBan, NopPost, NopThread, NopFlag::new(), NopAudit, NopUser,
    ));
    let board_svc = Arc::new(NopBoardRepo);
    moderation_routes(svc, board_svc)
}

fn with_mod_user(mut req: Request<Body>) -> Request<Body> {
    let user = CurrentUser::from_claims(Claims {
        user_id:      UserId(Uuid::new_v4()),
        username: "testuser".into(),
        role:         Role::Janitor,
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

fn plain_post(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

// ─── Flag queue ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_flags_returns_200_with_pending_flags() {
    let resp = mod_app()
        .oneshot(with_mod_user(get("/mod/flags")))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"], 1, "stub returns one pending flag");
}

#[tokio::test]
async fn list_flags_returns_401_without_auth() {
    let resp = mod_app().oneshot(get("/mod/flags")).await.unwrap();
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn resolve_flag_returns_204_for_valid_id() {
    let flag_id = Uuid::new_v4();
    let resp = mod_app()
        .oneshot(with_mod_user(json_post(
            &format!("/mod/flags/{flag_id}/resolve"),
            r#"{"resolution":"approved"}"#,
        )))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// ─── Post/thread management ───────────────────────────────────────────────────

#[tokio::test]
async fn delete_post_returns_204() {
    let post_id = Uuid::new_v4();
    let resp = mod_app()
        .oneshot(with_mod_user(plain_post(&format!("/mod/posts/{post_id}/delete"))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_thread_returns_204() {
    let thread_id = Uuid::new_v4();
    let resp = mod_app()
        .oneshot(with_mod_user(plain_post(&format!("/mod/threads/{thread_id}/delete"))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn toggle_sticky_returns_204() {
    let thread_id = Uuid::new_v4();
    let resp = mod_app()
        .oneshot(with_mod_user(json_post(
            &format!("/mod/threads/{thread_id}/sticky"),
            r#"{"value":true}"#,
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn toggle_close_returns_204() {
    let thread_id = Uuid::new_v4();
    let resp = mod_app()
        .oneshot(with_mod_user(json_post(
            &format!("/mod/threads/{thread_id}/close"),
            r#"{"value":true}"#,
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// ─── Bans ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_bans_returns_200() {
    let resp = mod_app()
        .oneshot(with_mod_user(get("/mod/bans")))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn create_ban_returns_201() {
    let resp = mod_app()
        .oneshot(with_mod_user(json_post(
            "/mod/bans",
            r#"{"ip_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reason":"spam","expires_at":null}"#,
        )))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn expire_ban_returns_204() {
    let ban_id = Uuid::new_v4();
    let resp = mod_app()
        .oneshot(with_mod_user(plain_post(&format!("/mod/bans/{ban_id}/expire"))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// ─── Public flag creation ─────────────────────────────────────────────────────

#[tokio::test]
async fn create_flag_returns_201_without_auth() {
    // `POST /board/:slug/thread/:id/flag` requires no authentication.
    // ConnectInfo is required by create_flag to hash the reporter IP.
    let thread_id = Uuid::new_v4();
    let mut req = json_post(
        &format!("/board/tech/thread/{thread_id}/flag"),
        r#"{"reason":"off-topic content"}"#,
    );
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        std::net::SocketAddr::from(([127, 0, 0, 1], 1234)),
    ));
    let resp = mod_app()
        .oneshot(req)
        .await
        .unwrap();

    // 201 Created or 204 No Content — either is acceptable for flag creation.
    assert!(
        resp.status() == StatusCode::CREATED || resp.status() == StatusCode::NO_CONTENT,
        "got {}",
        resp.status()
    );
}

// ─── Dashboard ────────────────────────────────────────────────────────────────

fn with_role_user(mut req: Request<Body>, role: Role) -> Request<Body> {
    let user = CurrentUser::from_claims(Claims {
        user_id:          UserId(Uuid::new_v4()),
        username:         "testuser".into(),
        role,
        owned_boards:     vec![],
        volunteer_boards: vec![],
        exp:              (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    });
    req.extensions_mut().insert(user);
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

#[tokio::test]
async fn janitor_dashboard_returns_200() {
    let resp = mod_app()
        .oneshot(with_mod_user(html_get("/janitor/dashboard")))
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
async fn janitor_dashboard_returns_200_for_admin_too() {
    let resp = mod_app()
        .oneshot(with_role_user(html_get("/janitor/dashboard"), Role::Admin))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn mod_dashboard_returns_401_without_auth() {
    let resp = mod_app()
        .oneshot(html_get("/mod/dashboard"))
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "expected 401/403, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn volunteer_dashboard_returns_200_for_board_owner() {
    let resp = mod_app()
        .oneshot(with_role_user(html_get("/volunteer/dashboard"), Role::BoardOwner))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn volunteer_dashboard_returns_401_without_auth() {
    let resp = mod_app()
        .oneshot(html_get("/volunteer/dashboard"))
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "expected 401/403, got {}",
        resp.status()
    );
}
