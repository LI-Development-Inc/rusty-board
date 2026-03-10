//! Integration tests for `PostService`.
//!
//! Covers every validation branch required by the project plan:
//!  - Anonymous post creation (text only)
//!  - Post that fails body-length validation
//!  - Post that fails max-files validation
//!  - Post that is rate-limited
//!  - Post from a banned IP
//!  - Thread creation and reply (bump)
//!  - Sage reply (no bump when allow_sage = true)
//!  - Bump limit enforcement
//!  - forced_anon strips the name field
//!  - Spam filter rejection

use chrono::Utc;
use domains::{
    models::{
        BanId, BoardId, BoardConfig, IpHash, Thread, ThreadId, UserId,
    },
    ports::{
        MockBanRepository, MockMediaProcessor, MockMediaStorage, MockPostRepository,
        MockRateLimiter, MockThreadRepository, RateLimitStatus,
    },
};
use services::post::{PostDraft, PostService, PostError};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn permissive_config() -> BoardConfig {
    BoardConfig {
        rate_limit_enabled:  false,
        spam_filter_enabled: false,
        duplicate_check:     false,
        ..BoardConfig::default()
    }
}

fn make_service(
    post_mock: MockPostRepository,
    thread_mock: MockThreadRepository,
    ban_mock: MockBanRepository,
    storage_mock: MockMediaStorage,
    rl_mock: MockRateLimiter,
    proc_mock: MockMediaProcessor,
) -> PostService<
    MockPostRepository,
    MockThreadRepository,
    MockBanRepository,
    MockMediaStorage,
    MockRateLimiter,
    MockMediaProcessor,
> {
    PostService::new(post_mock, thread_mock, ban_mock, storage_mock, rl_mock, proc_mock, String::new())
}

fn text_draft(board_id: BoardId, thread_id: Option<ThreadId>) -> PostDraft {
    PostDraft {
        board_id,
        thread_id,
        body:        "Hello, board!".to_owned(),
        ip_hash:     IpHash::new("192.168.0.1"),
        name:        None,
        email:       None,
        files:       vec![],
        is_staff:    false,
        poster_role: None,
    }
}

fn open_thread(board_id: BoardId, thread_id: ThreadId) -> Thread {
    Thread {
        id:          thread_id,
        board_id,
        op_post_id:  None,
        reply_count: 0,
        bumped_at:   Utc::now(),
        sticky:      false,
        closed:      false,
        created_at:  Utc::now(),
    }
}

// ── Text-only new thread ──────────────────────────────────────────────────────

#[tokio::test]
async fn create_text_only_thread_succeeds() {
    let board_id = BoardId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock.expect_save().returning(|t| Ok(t.id));
    thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock.expect_save().returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let result = svc.create_post(text_draft(board_id, None), &permissive_config()).await;
    assert!(result.is_ok(), "text-only thread creation must succeed: {result:?}");
}

// ── Reply to existing thread ──────────────────────────────────────────────────

#[tokio::test]
async fn reply_to_existing_thread_succeeds_and_bumps() {
    let board_id = BoardId::new();
    let thread_id = ThreadId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock
        .expect_find_by_id()
        .returning(move |_| Ok(open_thread(board_id, thread_id)));
    thread_mock.expect_bump().times(1).returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock.expect_save().returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut config = permissive_config();
    config.allow_sage = false; // ensure bump happens

    let result = svc
        .create_post(text_draft(board_id, Some(thread_id)), &config)
        .await;
    assert!(result.is_ok(), "reply must succeed: {result:?}");
}

// ── Banned IP ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn banned_ip_is_rejected() {
    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| {
        Ok(Some(domains::models::Ban {
            id:         BanId::new(),
            ip_hash:    IpHash::new("192.168.0.1"),
            banned_by:  UserId::new(),
            reason:     "spam".to_owned(),
            expires_at: None,
            created_at: Utc::now(),
        }))
    });

    let svc = make_service(
        MockPostRepository::new(),
        MockThreadRepository::new(),
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let result = svc
        .create_post(text_draft(BoardId::new(), None), &permissive_config())
        .await;
    assert!(
        matches!(result, Err(PostError::Banned { .. })),
        "banned IP must be rejected: {result:?}"
    );
}

// ── Rate limit ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rate_limited_post_is_rejected() {
    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut rl_mock = MockRateLimiter::new();
    rl_mock.expect_check().returning(|_| {
        Ok(RateLimitStatus::Exceeded {
            retry_after_secs: 60,
        })
    });

    let svc = make_service(
        MockPostRepository::new(),
        MockThreadRepository::new(),
        ban_mock,
        MockMediaStorage::new(),
        rl_mock,
        MockMediaProcessor::new(),
    );

    let mut config = permissive_config();
    config.rate_limit_enabled = true;
    config.rate_limit_posts = 3;
    config.rate_limit_window_secs = 60;

    let result = svc.create_post(text_draft(BoardId::new(), None), &config).await;
    assert!(
        matches!(result, Err(PostError::RateLimited { .. })),
        "rate-limited post must be rejected: {result:?}"
    );
}

// ── Body too long ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn body_exceeding_max_length_is_rejected() {
    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let svc = make_service(
        MockPostRepository::new(),
        MockThreadRepository::new(),
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(BoardId::new(), None);
    draft.body = "a".repeat(8001); // default max_post_length = 4000 (or whatever is default)

    let mut config = permissive_config();
    config.max_post_length = 4000;

    let result = svc.create_post(draft, &config).await;
    assert!(
        matches!(result, Err(PostError::Validation { .. })),
        "oversized body must be rejected: {result:?}"
    );
}

// ── Too many files ────────────────────────────────────────────────────────────

#[tokio::test]
async fn too_many_files_is_rejected() {
    use bytes::Bytes;
    use mime::IMAGE_PNG;
    use domains::ports::RawMedia;

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let svc = make_service(
        MockPostRepository::new(),
        MockThreadRepository::new(),
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(BoardId::new(), None);
    // Default max_files = 4; submit 5
    for _ in 0..5 {
        draft.files.push(RawMedia {
            data:     Bytes::from_static(b"fake-png-data"),
            mime:     IMAGE_PNG,
            filename: "img.png".to_owned(),
        });
    }

    let mut config = permissive_config();
    config.max_files = 4;

    let result = svc.create_post(draft, &config).await;
    assert!(
        matches!(result, Err(PostError::Validation { .. })),
        "post with too many files must be rejected: {result:?}"
    );
}

// ── Spam filter ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn spam_filter_enabled_rejects_high_score_body() {
    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let svc = make_service(
        MockPostRepository::new(),
        MockThreadRepository::new(),
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(BoardId::new(), None);
    // Many URLs push spam score above threshold
    draft.body =
        "http://a.com http://b.com http://c.com http://d.com http://e.com http://f.com"
            .to_owned();

    let mut config = permissive_config();
    config.spam_filter_enabled = true;
    config.spam_score_threshold = 0.4;

    let result = svc.create_post(draft, &config).await;
    assert!(
        matches!(result, Err(PostError::SpamDetected { .. })),
        "high-spam body must be rejected when filter is on: {result:?}"
    );
}

#[tokio::test]
async fn spam_filter_disabled_allows_high_score_body() {
    let board_id = BoardId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock.expect_save().returning(|t| Ok(t.id));
    thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock.expect_save().returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(board_id, None);
    draft.body =
        "http://a.com http://b.com http://c.com http://d.com http://e.com http://f.com"
            .to_owned();

    // spam_filter_enabled = false (permissive_config default)
    let result = svc.create_post(draft, &permissive_config()).await;
    assert!(result.is_ok(), "spam filter off must allow body: {result:?}");
}

// ── Sage (allow_sage = true) ──────────────────────────────────────────────────

#[tokio::test]
async fn sage_reply_skips_bump_when_sage_allowed() {
    let board_id = BoardId::new();
    let thread_id = ThreadId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock
        .expect_find_by_id()
        .returning(move |_| Ok(open_thread(board_id, thread_id)));
    // bump must NOT be called
    thread_mock.expect_bump().times(0).returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock.expect_save().returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(board_id, Some(thread_id));
    draft.email = Some("sage".to_owned());

    let mut config = permissive_config();
    config.allow_sage = true;

    let result = svc.create_post(draft, &config).await;
    assert!(result.is_ok());
}

// ── Bump limit ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reply_at_bump_limit_does_not_bump() {
    let board_id = BoardId::new();
    let thread_id = ThreadId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock.expect_find_by_id().returning(move |_| {
        Ok(Thread {
            id:          thread_id,
            board_id,
            op_post_id:  None,
            reply_count: 200, // at or beyond bump_limit
            bumped_at:   Utc::now(),
            sticky:      false,
            closed:      false,
            created_at:  Utc::now(),
        })
    });
    thread_mock.expect_bump().times(0).returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock.expect_save().returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut config = permissive_config();
    config.bump_limit = 200;

    let result = svc
        .create_post(text_draft(board_id, Some(thread_id)), &config)
        .await;
    assert!(result.is_ok());
}

// ── forced_anon strips name ───────────────────────────────────────────────────

#[tokio::test]
async fn forced_anon_erases_submitted_name() {
    let board_id = BoardId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock.expect_save().returning(|t| Ok(t.id));
    thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock
        .expect_save()
        .withf(|p| p.name.is_none())
        .times(1)
        .returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(board_id, None);
    draft.name = Some("NamedUser".to_owned());

    let mut config = permissive_config();
    config.forced_anon = true;

    let result = svc.create_post(draft, &config).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn forced_anon_false_preserves_submitted_name() {
    let board_id = BoardId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock.expect_save().returning(|t| Ok(t.id));
    thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock
        .expect_save()
        .withf(|p| p.name.as_deref() == Some("NamedUser"))
        .times(1)
        .returning(|p| Ok((p.id, 1u64)));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let mut draft = text_draft(board_id, None);
    draft.name = Some("NamedUser".to_owned());

    // forced_anon = false (default)
    let result = svc.create_post(draft, &permissive_config()).await;
    assert!(result.is_ok());
}

// ── Repo error propagation ────────────────────────────────────────────────────

#[tokio::test]
async fn post_repo_error_propagates() {
    let board_id = BoardId::new();

    let mut ban_mock = MockBanRepository::new();
    ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

    let mut thread_mock = MockThreadRepository::new();
    thread_mock.expect_save().returning(|t| Ok(t.id));
    thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

    let mut post_mock = MockPostRepository::new();
    post_mock
        .expect_save()
        .returning(|_| Err(domains::errors::DomainError::internal("db error")));

    let svc = make_service(
        post_mock,
        thread_mock,
        ban_mock,
        MockMediaStorage::new(),
        MockRateLimiter::new(),
        MockMediaProcessor::new(),
    );

    let result = svc.create_post(text_draft(board_id, None), &permissive_config()).await;
    assert!(result.is_err(), "repo error must propagate");
}
