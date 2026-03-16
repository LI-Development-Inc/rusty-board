//! Shared test fixtures for integration tests.
//!
//! # Modules
//! - `board_configs` — named `BoardConfig` variants (permissive, strict, nsfw, …)
//! - `boards`        — `Board` and `Thread` constructors
//! - `users`         — `User`, `Claims` and token helpers

use chrono::Utc;
use domains::models::*;
use uuid::Uuid;

// ─── board_configs ───────────────────────────────────────────────────────────

/// Named `BoardConfig` fixtures used across integration and unit tests.
///
/// Each function returns a fully populated `BoardConfig` with a single named
/// characteristic enabled and all unrelated toggles set to safe defaults.
pub mod board_configs {
    use domains::models::BoardConfig;

    /// All restrictions disabled — suitable for happy-path post-creation tests.
    pub fn permissive() -> BoardConfig {
        BoardConfig { rate_limit_enabled: false, spam_filter_enabled: false, duplicate_check: false, ..BoardConfig::default() }
    }

    /// Rate limit set to 1 post/60s, spam filter on, duplicate check on.
    pub fn strict() -> BoardConfig {
        BoardConfig { rate_limit_enabled: true, rate_limit_posts: 1, rate_limit_window_secs: 60, spam_filter_enabled: true, spam_score_threshold: 0.5, duplicate_check: true, ..BoardConfig::default() }
    }

    /// NSFW flag set; all other fields at their defaults.
    pub fn nsfw() -> BoardConfig { BoardConfig { nsfw: true, ..BoardConfig::default() } }

    /// No file attachments allowed (`max_files: 0`, empty MIME list).
    pub fn no_media() -> BoardConfig { BoardConfig { max_files: 0, allowed_mimes: vec![], ..BoardConfig::default() } }

    /// `forced_anon: true` — name field is always ignored.
    pub fn forced_anon() -> BoardConfig { BoardConfig { forced_anon: true, ..BoardConfig::default() } }

    /// Custom `bump_limit` with rate-limiting and spam-filtering disabled.
    pub fn tight_bump_limit(limit: u32) -> BoardConfig {
        BoardConfig { bump_limit: limit, rate_limit_enabled: false, spam_filter_enabled: false, ..BoardConfig::default() }
    }

    /// Custom `max_post_length` with rate-limiting and spam-filtering disabled.
    pub fn short_posts(max_chars: u32) -> BoardConfig {
        BoardConfig { max_post_length: max_chars, rate_limit_enabled: false, spam_filter_enabled: false, ..BoardConfig::default() }
    }

    /// Only JPEG attachments allowed; rate-limiting and spam-filtering disabled.
    pub fn jpeg_only() -> BoardConfig {
        BoardConfig { allowed_mimes: vec!["image/jpeg".to_owned()], rate_limit_enabled: false, spam_filter_enabled: false, ..BoardConfig::default() }
    }
}

// ─── boards ──────────────────────────────────────────────────────────────────

/// `Board` and `Thread` constructors for use in integration and unit tests.
pub mod boards {
    use chrono::Utc;
    use domains::models::*;
    use uuid::Uuid;

    /// Construct a `Board` with the given slug and a generated UUID.
    pub fn board(slug: &str) -> Board {
        Board { id: BoardId(Uuid::new_v4()), slug: Slug::new(slug).unwrap(), title: format!("/{slug}/ — Test Board"), rules: "".to_owned(), created_at: Utc::now() }
    }

    /// A pre-built board with slug `tech`.
    pub fn tech_board() -> Board { board("tech") }

    /// A pre-built board with slug `b`.
    pub fn random_board() -> Board { board("b") }

    /// Construct an open, non-sticky `Thread` belonging to `board_id`.
    pub fn thread(board_id: BoardId) -> Thread {
        Thread { id: ThreadId(Uuid::new_v4()), board_id, op_post_id: None, reply_count: 0, bumped_at: Utc::now(), sticky: false, closed: false, cycle: false, created_at: Utc::now() }
    }

    /// A sticky thread (`sticky: true`).
    pub fn sticky_thread(board_id: BoardId) -> Thread { Thread { sticky: true, ..thread(board_id) } }

    /// A closed thread (`closed: true, cycle: false`).
    pub fn closed_thread(board_id: BoardId) -> Thread { Thread { closed: true, cycle: false, ..thread(board_id) } }

    /// A thread whose `reply_count` equals `limit` (used to test bump-limit behaviour).
    pub fn bumped_out_thread(board_id: BoardId, limit: u32) -> Thread { Thread { reply_count: limit, ..thread(board_id) } }
}

// ─── users ───────────────────────────────────────────────────────────────────

/// `User`, `Claims`, and bearer token fixtures for integration tests.
pub mod users {
    use chrono::Utc;
    use domains::models::*;
    use uuid::Uuid;

    /// An active admin `User` with a stub Argon2 hash (never used for real auth).
    pub fn admin_user() -> User {
        User { id: UserId(Uuid::new_v4()), username: "admin".to_owned(), password_hash: PasswordHash::new("$argon2id$v=19$m=19456,t=2,p=1$stub"), role: Role::Admin, is_active: true, created_at: Utc::now() }
    }

    /// An active janitor `User` (global site moderator) with a stub Argon2 hash.
    pub fn mod_user() -> User {
        User { id: UserId(Uuid::new_v4()), username: "janitor1".to_owned(), password_hash: PasswordHash::new("$argon2id$v=19$m=19456,t=2,p=1$stub"), role: Role::Janitor, is_active: true, created_at: Utc::now() }
    }

    /// `Claims` for an admin user valid for 24 hours from now.
    pub fn admin_claims() -> Claims {
        Claims { user_id: UserId(Uuid::new_v4()), username: "admin".into(), role: Role::Admin, owned_boards: vec![], volunteer_boards: vec![], exp: (Utc::now() + chrono::Duration::hours(24)).timestamp() }
    }

    /// `Claims` for a janitor user (global moderator) valid for 24 hours from now.
    pub fn mod_claims() -> Claims {
        Claims { user_id: UserId(Uuid::new_v4()), username: "janitor".into(), role: Role::Janitor, owned_boards: vec![], volunteer_boards: vec![], exp: (Utc::now() + chrono::Duration::hours(24)).timestamp() }
    }

    /// `Claims` for a board owner who owns the given board.
    pub fn board_owner_claims(board_id: BoardId) -> Claims {
        Claims { user_id: UserId(Uuid::new_v4()), username: "boardowner".into(), role: Role::BoardOwner, owned_boards: vec![board_id], volunteer_boards: vec![], exp: (Utc::now() + chrono::Duration::hours(24)).timestamp() }
    }
}

// ─── Legacy top-level helpers ─────────────────────────────────────────────────

/// Build a minimal `Board` with the given slug for use in handler tests.
pub fn board_fixture(slug: &str) -> Board {
    Board { id: BoardId(Uuid::new_v4()), slug: Slug::new(slug).unwrap(), title: format!("/{slug}/ — Test Board"), rules: "Be excellent.".to_owned(), created_at: Utc::now() }
}

/// A default `BoardConfig` (all fields at their zero-restriction defaults).
pub fn config_fixture() -> BoardConfig { BoardConfig::default() }

/// A single-page `Paginated<Board>` containing one board with the given slug.
pub fn board_page_fixture(slug: &str) -> Paginated<Board> {
    let b = board_fixture(slug);
    Paginated::new(vec![b], 1, Page::new(1), 15)
}

/// Build a minimal open `Thread` belonging to `board_id`.
pub fn thread_fixture(board_id: BoardId) -> Thread {
    Thread { id: ThreadId(Uuid::new_v4()), board_id, op_post_id: None, reply_count: 0, bumped_at: Utc::now(), sticky: false, closed: false, cycle: false, created_at: Utc::now() }
}

/// Build a minimal text `Post` belonging to `thread_id`.
pub fn post_fixture(thread_id: ThreadId) -> Post {
    Post { id: PostId(Uuid::new_v4()), thread_id, body: "Test post body.".to_owned(), ip_hash: IpHash::new("deadbeef".repeat(8)), name: None, email: None, tripcode: None, created_at: Utc::now(), post_number: 1, pinned: false }
}

/// A bearer token with an invalid signature — triggers `401` on protected routes.
pub fn anon_token() -> Token { Token::new("invalid.token.value".to_owned()) }

#[test]
fn board_fixture_has_valid_slug() {
    let b = board_fixture("tech");
    assert_eq!(b.slug.as_str(), "tech");
}

#[test]
fn post_fixture_has_correct_thread_id() {
    let tid = ThreadId(Uuid::new_v4());
    let p = post_fixture(tid);
    assert_eq!(p.thread_id, tid);
}

#[test]
fn board_configs_permissive_disables_rate_limit() {
    let c = board_configs::permissive();
    assert!(!c.rate_limit_enabled);
    assert!(!c.spam_filter_enabled);
}

#[test]
fn board_configs_strict_has_low_rate_limit() {
    let c = board_configs::strict();
    assert!(c.rate_limit_enabled);
    assert_eq!(c.rate_limit_posts, 1);
}
