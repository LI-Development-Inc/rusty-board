//! `PostService` — the central business logic for post creation.
//!
//! This is the most complex service. It orchestrates:
//! 1. Active ban check (always runs, regardless of BoardConfig)
//! 2. Rate limit check (if `board_config.rate_limit_enabled`)
//! 3. Spam heuristics (if `board_config.spam_filter_enabled`)
//! 4. Duplicate detection (if `board_config.duplicate_check`)
//! 5. Post body validation (length, content)
//! 6. Media processing (if attachments present)
//! 7. Media storage
//! 8. Post persistence
//! 9. Thread bump (unless email == "sage" and `board_config.allow_sage`)
//! 10. Thread prune check
//!
//! Generic over 6 port traits. All conditional logic is driven by `BoardConfig`
//! fields — never by feature flags or environment variables.

pub mod errors;
pub use errors::PostError;

use domains::errors::DomainError;
use domains::models::{
    Attachment, BoardConfig, BoardId, IpHash, OverboardPost, Page, Post, PostId,
    Thread, ThreadId,
};
use domains::ports::{
    BanRepository, MediaProcessor, MediaStorage, PostRepository, RateLimitKey, RateLimitStatus,
    RateLimiter, RawMedia, ThreadRepository,
};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::common::utils::{hash_content, now_utc, score_spam};

/// A post draft submitted by a poster — the input to `PostService::create_post`.
///
/// This is a service-level DTO, not an HTTP DTO. The API layer is responsible for
/// extracting and validating the HTTP request before constructing a `PostDraft`.
#[derive(Debug)]
pub struct PostDraft {
    /// The board the post is being made on.
    pub board_id: BoardId,
    /// The thread to reply to. `None` if creating a new thread.
    pub thread_id: Option<ThreadId>,
    /// The post body text.
    pub body: String,
    /// The poster's hashed IP address.
    pub ip_hash: IpHash,
    /// The poster's display name. `None` for anonymous.
    pub name: Option<String>,
    /// The email field. `Some("sage")` prevents thread bump.
    pub email: Option<String>,
    /// Raw media attachments.
    pub files: Vec<RawMedia>,
    /// When `true` the poster is an authenticated staff member (janitor/mod/admin).
    /// Staff bypass rate-limiting, spam scoring, and duplicate detection.
    pub is_staff: bool,
    /// The authenticated role of the poster, if any. `None` for anonymous posts.
    /// Required to verify capcode claims (`### Admin`, etc.).
    pub poster_role: Option<domains::models::Role>,
}

/// The result of a successful post creation.
#[derive(Debug)]
pub struct PostResult {
    /// The newly created post.
    pub post: Post,
    /// The thread the post was added to (new or existing).
    pub thread: Thread,
    /// The processed and stored attachments.
    pub attachments: Vec<Attachment>,
}

/// Service handling post creation.
///
/// Generic over all 6 required port traits. The composition root injects concrete
/// implementations; unit tests inject `MockXxx` types from `mockall`.
pub struct PostService<PR, TR, BR, MS, RL, MP>
where
    PR: PostRepository,
    TR: ThreadRepository,
    BR: BanRepository,
    MS: MediaStorage,
    RL: RateLimiter,
    MP: MediaProcessor,
{
    post_repo:        PR,
    thread_repo:      TR,
    ban_repo:         BR,
    media_storage:    MS,
    rate_limiter:     RL,
    media_processor:  MP,
    /// Server-side secret used for `##` secure tripcodes. Empty = no pepper.
    tripcode_pepper:  String,
}

impl<PR, TR, BR, MS, RL, MP> PostService<PR, TR, BR, MS, RL, MP>
where
    PR: PostRepository,
    TR: ThreadRepository,
    BR: BanRepository,
    MS: MediaStorage,
    RL: RateLimiter,
    MP: MediaProcessor,
{
    /// Construct a `PostService` by injecting all required ports.
    /// Construct a `PostService` by injecting all required ports.
    ///
    /// `tripcode_pepper` is the server-side secret for `##` secure tripcodes.
    /// Pass an empty string to disable the server secret (reduces `##` security).
    pub fn new(
        post_repo: PR,
        thread_repo: TR,
        ban_repo: BR,
        media_storage: MS,
        rate_limiter: RL,
        media_processor: MP,
        tripcode_pepper: String,
    ) -> Self {
        Self {
            post_repo,
            thread_repo,
            ban_repo,
            media_storage,
            rate_limiter,
            media_processor,
            tripcode_pepper,
        }
    }

    /// Create a new post (and optionally a new thread if no `thread_id` is provided).
    ///
    /// # Behaviour controlled by `BoardConfig`
    /// - `rate_limit_enabled` / `rate_limit_window_secs` / `rate_limit_posts`
    /// - `spam_filter_enabled` / `spam_score_threshold`
    /// - `duplicate_check`
    /// - `max_post_length`
    /// - `max_files` / `max_file_size` / `allowed_mimes`
    /// - `allow_sage` (controls whether sage email prevents bump)
    /// - `forced_anon` (ignores the name field when true)
    /// - `bump_limit` (posts past this count no longer bump the thread)
    ///
    /// # Staff bypass (`PostDraft::is_staff`)
    /// When `draft.is_staff` is `true` (set by the handler when a valid staff JWT is
    /// present), the following checks are **skipped entirely**, regardless of the
    /// `BoardConfig` values:
    /// - Rate limit check and counter increment (`rate_limit_enabled`)
    /// - Spam scoring (`spam_filter_enabled`)
    /// - Duplicate content detection (`duplicate_check`)
    ///
    /// The ban check (step 1) is **never** bypassed — it applies to all posters.
    ///
    /// # Error conditions
    /// - `PostError::Banned` — the poster's IP has an active ban (always checked)
    /// - `PostError::RateLimited` — rate limit exceeded (anonymous posters only)
    /// - `PostError::SpamDetected` — spam score above threshold (anonymous posters only)
    /// - `PostError::DuplicatePost` — duplicate content hash (anonymous posters only)
    /// - `PostError::Validation` — body/file validation failed
    /// - `PostError::ThreadNotFound` — specified thread does not exist
    /// - `PostError::ThreadClosed` — thread is closed
    /// - `PostError::MediaError` — media processing failed
    #[instrument(skip(self, draft, board_config), fields(
        board_id = %draft.board_id,
        thread_id = ?draft.thread_id,
        has_files = !draft.files.is_empty(),
    ))]
    pub async fn create_post(
        &self,
        draft: PostDraft,
        board_config: &BoardConfig,
    ) -> Result<PostResult, PostError> {
        // ── Step 1: Active ban check ─────────────────────────────────────────
        // INVARIANT: ban check ALWAYS runs — it is not a BoardConfig toggle.
        if let Some(ban) = self
            .ban_repo
            .find_active_by_ip(&draft.ip_hash)
            .await?
        {
            return Err(PostError::Banned {
                reason:     ban.reason.clone(),
                expires_at: ban.expires_at,
            });
        }

        // ── Step 2: Rate limit check ─────────────────────────────────────────
        // Staff (authenticated janitor/mod/admin) bypass rate limiting entirely.
        if board_config.rate_limit_enabled && !draft.is_staff {
            let key = RateLimitKey {
                ip_hash:  draft.ip_hash.clone(),
                board_id: draft.board_id,
            };
            match self.rate_limiter.check(&key).await? {
                RateLimitStatus::Exceeded { retry_after_secs } => {
                    return Err(PostError::RateLimited { retry_after_secs });
                }
                RateLimitStatus::Allowed { .. } => {}
            }
        }

        // ── Step 3: Post body validation ─────────────────────────────────────
        // A post must have either a non-empty body or at least one attachment.
        if draft.body.trim().is_empty() && draft.files.is_empty() {
            return Err(PostError::Validation {
                reason: "post must contain a body or at least one attachment".to_owned(),
            });
        }
        if !board_config.allows_post_length(draft.body.len()) {
            return Err(PostError::Validation {
                reason: format!(
                    "post body length {} exceeds maximum {}",
                    draft.body.len(),
                    board_config.max_post_length,
                ),
            });
        }

        // ── Step 4: Attachment count validation ──────────────────────────────
        if draft.files.len() > board_config.max_files as usize {
            return Err(PostError::Validation {
                reason: format!(
                    "too many files: {} (max {})",
                    draft.files.len(),
                    board_config.max_files,
                ),
            });
        }

        // ── Step 5a: Name-based rate limiting ────────────────────────────────
        // Prevents a named identity from flooding without triggering IP rate limits.
        // Only applies when forced_anon is false, a name was supplied, and the
        // `name_rate_limit_window_secs` config is non-zero.
        if board_config.name_rate_limit_window_secs > 0
            && !draft.is_staff
            && !board_config.forced_anon
        {
            if let Some(ref name) = draft.name {
                if !name.trim().is_empty() {
                    // Derive a pseudo-IP-hash from the name so we can reuse the
                    // existing RateLimiter port without a new port method.
                    let name_pseudo_hash = hash_content(
                        format!("name:{}:{}", name, draft.board_id).as_bytes()
                    );
                    let name_key = domains::ports::RateLimitKey {
                        ip_hash:  domains::models::IpHash::new(name_pseudo_hash.0),
                        board_id: draft.board_id,
                    };
                    let status = self
                        .rate_limiter
                        .check(&name_key)
                        .await
                        .map_err(PostError::Internal)?;
                    if let domains::ports::RateLimitStatus::Exceeded { retry_after_secs } = status {
                        return Err(PostError::RateLimited { retry_after_secs });
                    }
                    self.rate_limiter
                        .increment(&name_key, board_config.name_rate_limit_window_secs)
                        .await
                        .map_err(PostError::Internal)?;
                }
            }
        }

        // ── Step 5b: Spam heuristics ─────────────────────────────────────────
        // Staff bypass spam and duplicate checks (they can be trusted).
        if board_config.spam_filter_enabled && !draft.is_staff && !draft.body.is_empty() {
            let spam_score = score_spam(&draft.body, &board_config.link_blacklist);
            if spam_score >= board_config.spam_score_threshold {
                warn!(
                    score = spam_score,
                    threshold = board_config.spam_score_threshold,
                    "post rejected as spam"
                );
                return Err(PostError::SpamDetected { score: spam_score });
            }
        }

        // ── Step 6: Duplicate content detection ──────────────────────────────
        if board_config.duplicate_check && !draft.is_staff && !draft.body.is_empty() {
            let body_hash = hash_content(draft.body.as_bytes());
            let recent_hashes = self
                .post_repo
                .find_recent_hashes(draft.board_id, 100)
                .await?;
            if recent_hashes.contains(&body_hash) {
                return Err(PostError::DuplicatePost);
            }
        }

        // ── Step 7: Resolve or create thread ─────────────────────────────────
        let (thread, is_new_thread) = match draft.thread_id {
            Some(thread_id) => {
                let thread = self
                    .thread_repo
                    .find_by_id(thread_id)
                    .await
                    .map_err(|e| match e {
                        DomainError::NotFound { .. } => PostError::ThreadNotFound {
                            id: thread_id.to_string(),
                        },
                        other => PostError::Internal(other),
                    })?;
                if thread.closed {
                    return Err(PostError::ThreadClosed);
                }
                (thread, false)
            }
            None => {
                // New thread
                let now = now_utc();
                let thread = domains::models::Thread {
                    id:          ThreadId(Uuid::new_v4()),
                    board_id:    draft.board_id,
                    op_post_id:  None,
                    reply_count: 0,
                    bumped_at:   now,
                    sticky:      false,
                    closed:      false,
                    created_at:  now,
                };
                let thread_id = self.thread_repo.save(&thread).await?;
                let thread = domains::models::Thread { id: thread_id, ..thread };
                (thread, true)
            }
        };

        // ── Step 8: Process and store media attachments ───────────────────────
        let mut attachments: Vec<Attachment> = Vec::new();
        for raw_file in draft.files {
            // Validate MIME type against board config
            let mime_str = raw_file.mime.to_string();
            if !board_config.allows_mime(&mime_str) {
                return Err(PostError::Validation {
                    reason: format!("mime type '{}' is not allowed on this board", mime_str),
                });
            }
            // Validate file size
            let size_kb = (raw_file.data.len() as u32).div_ceil(1024);
            if !board_config.allows_file_size_kb(size_kb) {
                return Err(PostError::Validation {
                    reason: format!(
                        "file size {}KB exceeds board maximum {}KB",
                        size_kb, board_config.max_file_size.0,
                    ),
                });
            }
            // Process
            let processed = self.media_processor.process(raw_file).await.map_err(|e| {
                PostError::MediaError { reason: e.to_string() }
            })?;
            // Store original
            self.media_storage
                .store(
                    &processed.original_key,
                    processed.original_data.clone(),
                    &mime_str,
                )
                .await
                .map_err(|e| PostError::MediaError { reason: e.to_string() })?;
            // Store thumbnail if present
            if let (Some(thumb_key), Some(thumb_data)) =
                (&processed.thumbnail_key, &processed.thumbnail_data)
            {
                self.media_storage
                    .store(thumb_key, thumb_data.clone(), "image/png")
                    .await
                    .map_err(|e| PostError::MediaError { reason: e.to_string() })?;
            }
            attachments.push(Attachment {
                id:            Uuid::new_v4(),
                post_id:       PostId(Uuid::nil()), // filled in after post is saved
                filename:      processed.original_key.0.clone(),
                mime:          mime_str,
                hash:          processed.hash,
                size_kb:       processed.size_kb,
                media_key:     processed.original_key,
                thumbnail_key: processed.thumbnail_key,
                spoiler:       false,
            });
        }

        // ── Step 9: Apply forced_anon + tripcode/capcode parsing ─────────────
        // Parse the name field for `#` tripcode specifiers and `### Role` capcodes.
        // forced_anon strips both name and tripcode.
        let (name, tripcode) = if board_config.forced_anon {
            (None, None)
        } else {
            use crate::common::tripcode::parse_name_field;
            match draft.name.as_deref() {
                None | Some("") => (None, None),
                Some(raw_name) => {
                    match parse_name_field(raw_name, draft.poster_role.as_ref(), &self.tripcode_pepper) {
                        Ok(parsed) => (parsed.name, parsed.tripcode),
                        Err(crate::common::tripcode::NameParseError::CapcodePermissionDenied { .. }) => {
                            // Reject the post — impersonation attempt
                            return Err(PostError::Validation {
                                reason: "capcode permission denied: you do not have that staff role".to_owned(),
                            });
                        }
                        Err(crate::common::tripcode::NameParseError::UnknownCapcodeRole { role }) => {
                            return Err(PostError::Validation {
                                reason: format!("unknown capcode role '{role}'"),
                            });
                        }
                    }
                }
            }
        };

        // ── Step 10: Insert post ──────────────────────────────────────────────
        let post = Post {
            id:          PostId(Uuid::new_v4()),
            thread_id:   thread.id,
            body:        draft.body.clone(),
            ip_hash:     draft.ip_hash.clone(),
            name,
            tripcode,    // computed in step 9
            email:       draft.email.clone(),
            created_at:  now_utc(),
            post_number: 0, // assigned atomically by the repository via board counter
        };
        let (post_id, post_number) = self.post_repo.save(&post).await?;
        let post = Post { id: post_id, post_number, ..post };

        // If this is the OP, link op_post_id on the thread
        if is_new_thread {
            self.thread_repo
                .set_op_post(thread.id, post.id)
                .await?;
        }

        // ── Step 11: Bump thread ──────────────────────────────────────────────
        // Sage: if allow_sage is true and email == "sage", skip the bump.
        let is_sage = board_config.allow_sage
            && draft.email.as_deref() == Some("sage");
        // Bump limit: past bump_limit replies, thread no longer bumps.
        let past_bump_limit = thread.reply_count >= board_config.bump_limit;

        if !is_sage && !past_bump_limit && !is_new_thread {
            self.thread_repo.bump(thread.id, now_utc()).await?;
        }

        // TODO v1.2 — Cycle mode: when `thread.cycle == true` and `past_bump_limit`,
        // instead of silently stopping bumps, prune the oldest post in the thread
        // that is NOT pinned (`post.pinned == false`). This keeps cycle threads
        // perpetually live. Implementation sketch:
        //
        //   if past_bump_limit && thread.cycle {
        //       self.post_repo
        //           .delete_oldest_unpinned(thread.id)
        //           .await?;
        //   }

        // ── Step 12: Increment rate limit counter ─────────────────────────────
        if board_config.rate_limit_enabled && !draft.is_staff {
            let key = RateLimitKey {
                ip_hash:  draft.ip_hash.clone(),
                board_id: draft.board_id,
            };
            // Increment after the post is saved so a save failure doesn't consume quota
            let _ = self
                .rate_limiter
                .increment(&key, board_config.rate_limit_window_secs)
                .await;
        }

        info!(
            post_id = %post.id,
            thread_id = %thread.id,
            board_id = %draft.board_id,
            is_new_thread,
            "post created"
        );

        // Update attachment post_ids now that we have the real post_id
        let attachments: Vec<Attachment> = attachments
            .into_iter()
            .map(|a| Attachment { post_id: post.id, ..a })
            .collect();

        // Persist attachment metadata to the database so the thread view can load them.
        if !attachments.is_empty() {
            self.post_repo.save_attachments(&attachments).await?;
        }

        Ok(PostResult { post, thread, attachments })
    }

    /// `GET /board/:slug/thread/:id` — list posts in a thread, paginated.
    pub async fn list_posts(
        &self,
        thread_id: ThreadId,
        page: Page,
    ) -> Result<domains::models::Paginated<Post>, PostError> {
        let paginated = self.post_repo.find_by_thread(thread_id, page).await?;
        Ok(paginated)
    }

    /// `GET /overboard` — recent posts across all boards, paginated.
    pub async fn list_overboard(
        &self,
        page: Page,
    ) -> Result<domains::models::Paginated<OverboardPost>, PostError> {
        let paginated = self.post_repo.find_overboard(page).await?;
        Ok(paginated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    // Ban, ContentHash, FileSizeKb, MediaKey accessed via full path in tests
    use domains::ports::{
        MockBanRepository, MockMediaProcessor, MockMediaStorage, MockPostRepository,
        MockRateLimiter, MockThreadRepository, RateLimitStatus,
    };

    fn permissive_config() -> BoardConfig {
        BoardConfig {
            rate_limit_enabled:  false,
            spam_filter_enabled: false,
            duplicate_check:     false,
            ..BoardConfig::default()
        }
    }

    fn make_post_service(
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
        PostService::new(
            post_mock,
            thread_mock,
            ban_mock,
            storage_mock,
            rl_mock,
            proc_mock,
            String::new(), // empty pepper in tests
        )
    }

    fn text_draft(board_id: BoardId, thread_id: Option<ThreadId>) -> PostDraft {
        PostDraft {
            board_id,
            thread_id,
            body: "Hello world".to_owned(),
            ip_hash: IpHash::new("abc123"),
            name: None,
            email: None,
            files: vec![],
            is_staff: false,
            poster_role: None,
        }
    }

    #[allow(dead_code)]
    fn sample_thread(board_id: BoardId, thread_id: ThreadId) -> Thread {
        Thread {
            id: thread_id,
            board_id,
            op_post_id: None,
            reply_count: 0,
            bumped_at: Utc::now(),
            sticky: false,
            closed: false,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn create_post_new_thread_happy_path() {
        let board_id = BoardId::new();
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_save().returning(|t| Ok(t.id));
        thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock.expect_save().returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
            post_mock,
            thread_mock,
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let result = svc
            .create_post(text_draft(board_id, None), &permissive_config())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().thread.op_post_id.is_none()); // set async after save
    }

    #[tokio::test]
    async fn create_post_banned_ip_rejected() {
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| {
            Ok(Some(domains::models::Ban {
                id:         domains::models::BanId::new(),
                ip_hash:    IpHash::new("abc123"),
                banned_by:  domains::models::UserId::new(),
                reason:     "spam".to_owned(),
                expires_at: None,
                created_at: Utc::now(),
            }))
        });

        let svc = make_post_service(
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
        assert!(matches!(result, Err(PostError::Banned { .. })));
    }

    #[tokio::test]
    async fn create_post_rate_limited() {
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut rl_mock = MockRateLimiter::new();
        rl_mock.expect_check().returning(|_| {
            Ok(RateLimitStatus::Exceeded { retry_after_secs: 30 })
        });

        let svc = make_post_service(
            MockPostRepository::new(),
            MockThreadRepository::new(),
            ban_mock,
            MockMediaStorage::new(),
            rl_mock,
            MockMediaProcessor::new(),
        );

        let config = BoardConfig { rate_limit_enabled: true, ..BoardConfig::default() };

        let result = svc
            .create_post(text_draft(BoardId::new(), None), &config)
            .await;
        assert!(matches!(result, Err(PostError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn create_post_body_too_long() {
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let svc = make_post_service(
            MockPostRepository::new(),
            MockThreadRepository::new(),
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let mut draft = text_draft(BoardId::new(), None);
        draft.body = "a".repeat(5000); // exceeds default max_post_length of 4000

        let result = svc.create_post(draft, &permissive_config()).await;
        assert!(matches!(result, Err(PostError::Validation { .. })));
    }

    #[tokio::test]
    async fn create_post_thread_closed() {
        let thread_id = ThreadId::new();
        let board_id = BoardId::new();

        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_find_by_id().returning(move |_| {
            let t = Thread {
                id: thread_id,
                board_id,
                op_post_id: None,
                reply_count: 0,
                bumped_at: Utc::now(),
                sticky: false,
                closed: true,  // CLOSED
                created_at: Utc::now(),
            };
            Ok(t)
        });

        let svc = make_post_service(
            MockPostRepository::new(),
            thread_mock,
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let result = svc
            .create_post(text_draft(board_id, Some(thread_id)), &permissive_config())
            .await;
        assert!(matches!(result, Err(PostError::ThreadClosed)));
    }

    #[tokio::test]
    async fn forced_anon_strips_name() {
        let board_id = BoardId::new();
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_save().returning(|t| Ok(t.id));
        thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock
            .expect_save()
            .withf(|p| p.name.is_none()) // name must be None when forced_anon
            .times(1)
            .returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
            post_mock,
            thread_mock,
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let mut draft = text_draft(board_id, None);
        draft.name = Some("Alice".to_owned());

        let mut config = permissive_config();
        config.forced_anon = true;

        let result = svc.create_post(draft, &config).await;
        assert!(result.is_ok());
    }

    // ── forced_anon = false: name preserved ───────────────────────────────────
    #[tokio::test]
    async fn forced_anon_false_preserves_name() {
        let board_id = BoardId::new();
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_save().returning(|t| Ok(t.id));
        thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock
            .expect_save()
            .withf(|p| p.name.as_deref() == Some("Alice"))
            .times(1)
            .returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
            post_mock,
            thread_mock,
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let mut draft = text_draft(board_id, None);
        draft.name = Some("Alice".to_owned());

        // forced_anon is false by default in permissive_config
        let result = svc.create_post(draft, &permissive_config()).await;
        assert!(result.is_ok());
    }

    // ── spam_filter_enabled = true: high-spam body rejected ───────────────────
    #[tokio::test]
    async fn spam_filter_enabled_rejects_spammy_body() {
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let svc = make_post_service(
            MockPostRepository::new(),
            MockThreadRepository::new(),
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let mut draft = text_draft(BoardId::new(), None);
        // Many URLs drive spam score very high
        draft.body =
            "http://a.com http://b.com http://c.com http://d.com http://e.com http://f.com"
                .to_owned();

        let mut config = permissive_config();
        config.spam_filter_enabled = true;
        // URL heuristic caps at 0.4; threshold must be <= that to trigger.
        config.spam_score_threshold = 0.3;

        let result = svc.create_post(draft, &config).await;
        assert!(
            matches!(result, Err(PostError::SpamDetected { .. })),
            "expected spam rejection"
        );
    }

    // ── spam_filter_enabled = false: same spammy body accepted ────────────────
    #[tokio::test]
    async fn spam_filter_disabled_allows_spammy_body() {
        let board_id = BoardId::new();
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_save().returning(|t| Ok(t.id));
        thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock.expect_save().returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
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

        // spam_filter_enabled is false in permissive_config
        let result = svc.create_post(draft, &permissive_config()).await;
        assert!(result.is_ok(), "spam filter disabled should allow the post");
    }

    // ── allow_sage = true: sage reply does not bump thread ────────────────────
    #[tokio::test]
    async fn sage_reply_does_not_bump_when_allow_sage_true() {
        let board_id = BoardId::new();
        let thread_id = ThreadId::new();

        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_find_by_id().returning(move |_| {
            Ok(Thread {
                id:         thread_id,
                board_id,
                op_post_id: None,
                reply_count: 0,
                bumped_at:  Utc::now(),
                sticky:     false,
                closed:     false,
                created_at: Utc::now(),
            })
        });
        // bump must NOT be called for a sage reply
        thread_mock.expect_bump().times(0).returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock.expect_save().returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
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

    // ── allow_sage = false: sage email treated as normal, thread bumps ─────────
    #[tokio::test]
    async fn sage_email_bumps_when_allow_sage_false() {
        let board_id = BoardId::new();
        let thread_id = ThreadId::new();

        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_find_by_id().returning(move |_| {
            Ok(Thread {
                id:         thread_id,
                board_id,
                op_post_id: None,
                reply_count: 0,
                bumped_at:  Utc::now(),
                sticky:     false,
                closed:     false,
                created_at: Utc::now(),
            })
        });
        // bump MUST be called because allow_sage = false
        thread_mock.expect_bump().times(1).returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock.expect_save().returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
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
        config.allow_sage = false;

        let result = svc.create_post(draft, &config).await;
        assert!(result.is_ok());
    }

    // ── bump_limit: past limit thread does not bump ───────────────────────────
    #[tokio::test]
    async fn reply_past_bump_limit_does_not_bump() {
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
                reply_count: 500, // exactly at bump_limit
                bumped_at:   Utc::now(),
                sticky:      false,
                closed:      false,
                created_at:  Utc::now(),
            })
        });
        thread_mock.expect_bump().times(0).returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock.expect_save().returning(|p| Ok((p.id, 1)));

        let svc = make_post_service(
            post_mock,
            thread_mock,
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(),
            MockMediaProcessor::new(),
        );

        let mut config = permissive_config();
        config.bump_limit = 500; // reply_count >= bump_limit → no bump

        let result = svc
            .create_post(text_draft(board_id, Some(thread_id)), &config)
            .await;
        assert!(result.is_ok());
    }

    // ── rate_limit_enabled = false: limiter never called ─────────────────────
    #[tokio::test]
    async fn rate_limit_disabled_never_checks_limiter() {
        let board_id = BoardId::new();
        let mut ban_mock = MockBanRepository::new();
        ban_mock.expect_find_active_by_ip().returning(|_| Ok(None));

        let mut thread_mock = MockThreadRepository::new();
        thread_mock.expect_save().returning(|t| Ok(t.id));
        thread_mock.expect_set_op_post().returning(|_, _| Ok(()));

        let mut post_mock = MockPostRepository::new();
        post_mock.expect_save().returning(|p| Ok((p.id, 1)));

        // rl_mock has no expectations → any call to check/increment would panic
        let svc = make_post_service(
            post_mock,
            thread_mock,
            ban_mock,
            MockMediaStorage::new(),
            MockRateLimiter::new(), // no expectations set
            MockMediaProcessor::new(),
        );

        // rate_limit_enabled = false (permissive_config)
        let result = svc
            .create_post(text_draft(board_id, None), &permissive_config())
            .await;
        assert!(result.is_ok());
    }
}
