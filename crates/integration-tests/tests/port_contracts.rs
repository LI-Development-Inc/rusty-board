//! Adapter contract tests for domain port traits.
//!
//! These tests are structural: they verify that our hand-rolled stub adapters
//! satisfy the port trait contracts defined in `domains::ports`.
//!
//! Compile-time only — if a port method signature changes without updating stubs,
//! compilation fails here before any test runs.

use chrono::{DateTime, Utc};
use domains::{
    errors::DomainError,
    models::*,
    ports::*,
};

// ─── No-op stubs ─────────────────────────────────────────────────────────────

struct NoOpBoardRepo;
#[async_trait::async_trait]
impl BoardRepository for NoOpBoardRepo {
    async fn save(&self, _: &Board) -> Result<(), DomainError> { unimplemented!() }
    async fn find_by_id(&self, _: BoardId) -> Result<Board, DomainError> { unimplemented!() }
    async fn find_by_slug(&self, _: &Slug) -> Result<Board, DomainError> { unimplemented!() }
    async fn find_all(&self, _: Page) -> Result<Paginated<Board>, DomainError> { unimplemented!() }
    async fn delete(&self, _: BoardId) -> Result<(), DomainError> { unimplemented!() }
    async fn find_config(&self, _: BoardId) -> Result<BoardConfig, DomainError> { unimplemented!() }
    async fn save_config(&self, _: BoardId, _: &BoardConfig) -> Result<(), DomainError> { unimplemented!() }
}

#[async_trait::async_trait]
impl domains::ports::BoardVolunteerRepository for NoOpBoardRepo {
    async fn list_volunteers(&self, _: BoardId) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, DomainError> { Ok(vec![]) }
    async fn add_volunteer_by_username(&self, _: BoardId, _: &str, _: domains::models::UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: domains::models::UserId) -> Result<(), DomainError> { Ok(()) }
}

struct NoOpThreadRepo;
#[async_trait::async_trait]
impl ThreadRepository for NoOpThreadRepo {
    async fn find_by_id(&self, _: ThreadId) -> Result<Thread, DomainError> { unimplemented!() }
    async fn find_by_board(&self, _: BoardId, _: Page) -> Result<Paginated<Thread>, DomainError> { unimplemented!() }
    async fn find_catalog(&self, _: BoardId) -> Result<Vec<ThreadSummary>, DomainError> { unimplemented!() }
    async fn save(&self, _: &Thread) -> Result<ThreadId, DomainError> { unimplemented!() }
    async fn bump(&self, _: ThreadId, _: DateTime<Utc>) -> Result<(), DomainError> { unimplemented!() }
    async fn set_op_post(&self, _: ThreadId, _: PostId) -> Result<(), DomainError> { unimplemented!() }
    async fn set_sticky(&self, _: ThreadId, _: bool) -> Result<(), DomainError> { unimplemented!() }
    async fn set_closed(&self, _: ThreadId, _: bool) -> Result<(), DomainError> { unimplemented!() }
    async fn count_by_board(&self, _: BoardId) -> Result<u32, DomainError> { unimplemented!() }
    async fn prune_oldest(&self, _: BoardId, _: u32) -> Result<u32, DomainError> { unimplemented!() }
    async fn delete(&self, _: ThreadId) -> Result<(), DomainError> { unimplemented!() }
}

struct NoOpPostRepo;
#[async_trait::async_trait]
impl PostRepository for NoOpPostRepo {
    async fn find_by_id(&self, _: PostId) -> Result<Post, DomainError> { unimplemented!() }
    async fn find_by_thread(&self, _: ThreadId, _: Page) -> Result<Paginated<Post>, DomainError> { unimplemented!() }
    async fn find_by_ip_hash(&self, _: &IpHash) -> Result<Vec<Post>, DomainError> { unimplemented!() }
    async fn find_recent_hashes(&self, _: BoardId, _: u32) -> Result<Vec<ContentHash>, DomainError> { unimplemented!() }
    async fn save(&self, _: &Post) -> Result<(PostId, u64), DomainError> { unimplemented!() }
    async fn delete(&self, _: PostId) -> Result<(), DomainError> { unimplemented!() }
    async fn delete_by_ip_in_thread(&self, _: &IpHash, _: ThreadId) -> Result<u64, DomainError> { Ok(0) }
    async fn save_attachments(&self, _: &[domains::models::Attachment]) -> Result<(), DomainError> { Ok(()) }
    async fn find_attachments_by_post_ids(&self, _: &[PostId]) -> Result<std::collections::HashMap<PostId, Vec<domains::models::Attachment>>, DomainError> { Ok(std::collections::HashMap::new()) }
    async fn find_overboard(&self, _: Page) -> Result<Paginated<OverboardPost>, DomainError> { unimplemented!() }
    async fn search_fulltext(
        &self,
        _: BoardId,
        _: &str,
        _: Page,
    ) -> Result<Paginated<Post>, DomainError> { unimplemented!() }
    async fn find_all_by_thread(&self, _: ThreadId) -> Result<Vec<Post>, DomainError> { Ok(vec![]) }
    async fn find_thread_id_by_post_number(&self, _: BoardId, _: u64) -> Result<Option<ThreadId>, DomainError> { Ok(None) }
}

struct NoOpBanRepo;
#[async_trait::async_trait]
impl BanRepository for NoOpBanRepo {
    async fn find_active_by_ip(&self, _: &IpHash) -> Result<Option<Ban>, DomainError> { unimplemented!() }
    async fn save(&self, _: &Ban) -> Result<BanId, DomainError> { unimplemented!() }
    async fn expire(&self, _: BanId) -> Result<(), DomainError> { unimplemented!() }
    async fn find_all(&self, _: Page) -> Result<Paginated<Ban>, DomainError> { unimplemented!() }
}

struct NoOpFlagRepo;
#[async_trait::async_trait]
impl FlagRepository for NoOpFlagRepo {
    async fn find_by_id(&self, _: FlagId) -> Result<Flag, DomainError> { unimplemented!() }
    async fn find_pending(&self, _: Page) -> Result<Paginated<Flag>, DomainError> { unimplemented!() }
    async fn save(&self, _: &Flag) -> Result<FlagId, DomainError> { unimplemented!() }
    async fn resolve(&self, _: FlagId, _: FlagResolution, _: UserId) -> Result<(), DomainError> { unimplemented!() }
}

struct NoOpAuditRepo;
#[async_trait::async_trait]
impl AuditRepository for NoOpAuditRepo {
    async fn record(&self, _: &AuditEntry) -> Result<(), DomainError> { unimplemented!() }
    async fn find_recent(&self, _: u32) -> Result<Vec<AuditEntry>, DomainError> { unimplemented!() }
    async fn find_by_actor(&self, _: UserId, _: Page) -> Result<Paginated<AuditEntry>, DomainError> { unimplemented!() }
    async fn find_by_target(&self, _: uuid::Uuid, _: Page) -> Result<Paginated<AuditEntry>, DomainError> { unimplemented!() }
    async fn find_all(&self, _: Page) -> Result<Paginated<AuditEntry>, DomainError> { unimplemented!() }
    async fn find_by_board(&self, _: BoardId, _: Page) -> Result<Paginated<AuditEntry>, DomainError> { unimplemented!() }
}

struct NoOpUserRepo;
#[async_trait::async_trait]
impl UserRepository for NoOpUserRepo {
    async fn find_by_id(&self, _: UserId) -> Result<User, DomainError> { unimplemented!() }
    async fn find_by_username(&self, _: &str) -> Result<User, DomainError> { unimplemented!() }
    async fn find_all(&self, _: Page) -> Result<Paginated<User>, DomainError> { unimplemented!() }
    async fn save(&self, _: &User) -> Result<(), DomainError> { unimplemented!() }
    async fn deactivate(&self, _: UserId) -> Result<(), DomainError> { unimplemented!() }
    async fn find_owned_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { unimplemented!() }
    async fn find_volunteer_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn add_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn add_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { unimplemented!() }
    async fn remove_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { unimplemented!() }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Compile-only test: all no-op stubs satisfy their respective port traits.
#[test]
fn adapter_stubs_satisfy_port_contracts() {
    fn _assert_board(_: &dyn BoardRepository) {}
    fn _assert_board_volunteer(_: &dyn domains::ports::BoardVolunteerRepository) {}
    fn _assert_thread(_: &dyn ThreadRepository) {}
    fn _assert_post(_: &dyn PostRepository) {}
    fn _assert_ban(_: &dyn BanRepository) {}
    fn _assert_flag(_: &dyn FlagRepository) {}
    fn _assert_audit(_: &dyn AuditRepository) {}
    fn _assert_user(_: &dyn UserRepository) {}

    _assert_board(&NoOpBoardRepo);
    _assert_board_volunteer(&NoOpBoardRepo);
    _assert_thread(&NoOpThreadRepo);
    _assert_post(&NoOpPostRepo);
    _assert_ban(&NoOpBanRepo);
    _assert_flag(&NoOpFlagRepo);
    _assert_audit(&NoOpAuditRepo);
    _assert_user(&NoOpUserRepo);
}

// ─── ID type contracts ────────────────────────────────────────────────────────

#[test]
fn all_id_types_have_new_constructors() {
    let _ = BoardId::new();
    let _ = ThreadId::new();
    let _ = PostId::new();
    let _ = UserId::new();
    let _ = BanId::new();
    let _ = FlagId::new();
}

#[test]
fn slug_is_case_sensitive_and_validates_pattern() {
    assert!(Slug::new("tech").is_ok());
    assert!(Slug::new("Tech").is_err());
    assert!(Slug::new("has space").is_err());
    assert!(Slug::new("").is_err());
    assert!(Slug::new("a".repeat(17)).is_err());
    assert!(Slug::new("a".repeat(16)).is_ok());
}

#[test]
fn board_config_default_is_conservative() {
    let cfg = BoardConfig::default();
    // Rate limiting on
    assert!(cfg.rate_limit_enabled);
    // Spam filter on
    assert!(cfg.spam_filter_enabled);
    // Duplicate check on
    assert!(cfg.duplicate_check);
    // Not NSFW by default
    assert!(!cfg.nsfw);
    // No captcha by default
    assert!(!cfg.captcha_required);
    // Federation off by default
    assert!(!cfg.federation_enabled);
}
