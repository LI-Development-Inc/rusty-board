//! `ModerationService` — business logic for all privileged moderation actions.
//!
//! Responsibilities:
//! - Delete posts and threads
//! - Toggle sticky/closed on threads
//! - Issue and expire bans
//! - Resolve flags (approve or reject)
//! - Write an audit log entry for every action
//!
//! Generic over 6 port traits. Every mutating operation writes an `AuditEntry`.
//! Audit log write failures are logged but do not propagate — they must not
//! interrupt the primary moderation action.

pub mod errors;
pub use errors::ModerationError;

use domains::errors::DomainError;
use domains::models::{
    AuditAction, AuditEntry, Ban, BanId, FlagId, FlagResolution, IpHash, Page,
    Paginated, PostId, ThreadId, UserId,
};
use domains::ports::{
    AuditRepository, BanRepository, FlagRepository, PostRepository, ThreadRepository,
    UserRepository,
};
use chrono::Utc;
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::common::utils::now_utc;

/// Service handling all moderation and administrative actions.
///
/// Generic over 6 port traits. The composition root injects concrete implementations.
pub struct ModerationService<BR, PR, TR, FR, AR, UR>
where
    BR: BanRepository,
    PR: PostRepository,
    TR: ThreadRepository,
    FR: FlagRepository,
    AR: AuditRepository,
    UR: UserRepository,
{
    ban_repo:   BR,
    post_repo:  PR,
    thread_repo: TR,
    flag_repo:  FR,
    audit_repo: AR,
    #[allow(dead_code)]
    user_repo:  UR,
}

impl<BR, PR, TR, FR, AR, UR> ModerationService<BR, PR, TR, FR, AR, UR>
where
    BR: BanRepository,
    PR: PostRepository,
    TR: ThreadRepository,
    FR: FlagRepository,
    AR: AuditRepository,
    UR: UserRepository,
{
    /// Construct a `ModerationService` by injecting all required ports.
    pub fn new(
        ban_repo: BR,
        post_repo: PR,
        thread_repo: TR,
        flag_repo: FR,
        audit_repo: AR,
        user_repo: UR,
    ) -> Self {
        Self {
            ban_repo,
            post_repo,
            thread_repo,
            flag_repo,
            audit_repo,
            user_repo,
        }
    }

    /// Delete a single post and record an audit entry.
    ///
    /// Returns `ModerationError::NotFound` if the post does not exist.
    #[instrument(skip(self), fields(post_id = %post_id, actor_id = %actor_id))]
    pub async fn delete_post(
        &self,
        post_id: PostId,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.post_repo.delete(post_id).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: post_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::DeletePost,
            Some(post_id.0),
            Some("post".to_owned()),
            None,
        )
        .await;
        info!(post_id = %post_id, "post deleted");
        Ok(())
    }

    /// Delete all posts by a given IP hash within a thread.
    ///
    /// Used for the [D*] moderation action. Returns the count of deleted posts.
    #[instrument(skip(self), fields(thread_id = %thread_id, actor_id = %actor_id))]
    pub async fn delete_posts_by_ip_in_thread(
        &self,
        ip_hash: IpHash,
        thread_id: ThreadId,
        actor_id: UserId,
    ) -> Result<u64, ModerationError> {
        let count = self.post_repo
            .delete_by_ip_in_thread(&ip_hash, thread_id)
            .await
            .map_err(ModerationError::Internal)?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::DeletePost,
            Some(thread_id.0),
            Some("thread".to_owned()),
            Some(serde_json::json!({ "ip_hash": ip_hash.0, "count": count, "bulk": true })),
        )
        .await;
        info!(thread_id = %thread_id, count, "bulk-deleted posts by IP");
        Ok(count)
    }

    /// Delete a thread and all its posts; record an audit entry.
    ///
    /// Returns `ModerationError::NotFound` if the thread does not exist.
    #[instrument(skip(self), fields(thread_id = %thread_id, actor_id = %actor_id))]
    pub async fn delete_thread(
        &self,
        thread_id: ThreadId,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.thread_repo.delete(thread_id).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: thread_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::DeleteThread,
            Some(thread_id.0),
            Some("thread".to_owned()),
            None,
        )
        .await;
        info!(thread_id = %thread_id, "thread deleted");
        Ok(())
    }

    /// Set the sticky flag on a thread and record an audit entry.
    #[instrument(skip(self), fields(thread_id = %thread_id, actor_id = %actor_id, sticky))]
    pub async fn set_sticky(
        &self,
        thread_id: ThreadId,
        sticky: bool,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.thread_repo.set_sticky(thread_id, sticky).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: thread_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::StickyThread,
            Some(thread_id.0),
            Some("thread".to_owned()),
            Some(serde_json::json!({ "sticky": sticky })),
        )
        .await;
        info!(thread_id = %thread_id, sticky, "thread sticky updated");
        Ok(())
    }

    /// Set the closed flag on a thread and record an audit entry.
    #[instrument(skip(self), fields(thread_id = %thread_id, actor_id = %actor_id, closed))]
    pub async fn set_closed(
        &self,
        thread_id: ThreadId,
        closed: bool,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.thread_repo.set_closed(thread_id, closed).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: thread_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::CloseThread,
            Some(thread_id.0),
            Some("thread".to_owned()),
            Some(serde_json::json!({ "closed": closed })),
        )
        .await;
        info!(thread_id = %thread_id, closed, "thread closed status updated");
        Ok(())
    }

    /// Set cycle mode on a thread. When `cycle=true` and the thread is past the
    /// bump limit, posting prunes the oldest unpinned reply instead of stopping.
    #[instrument(skip(self), fields(actor_id = %actor_id))]
    pub async fn set_cycle(
        &self,
        thread_id: ThreadId,
        cycle: bool,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.thread_repo.set_cycle(thread_id, cycle).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: thread_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::CycleThread,
            Some(thread_id.0),
            Some("thread".to_owned()),
            Some(serde_json::json!({ "cycle": cycle })),
        )
        .await;
        info!(thread_id = %thread_id, cycle, "thread cycle mode updated");
        Ok(())
    }

    /// Pin or unpin a post. Pinned posts are excluded from cycle-mode pruning.
    #[instrument(skip(self), fields(actor_id = %actor_id))]
    pub async fn set_pinned(
        &self,
        post_id: PostId,
        pinned: bool,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.post_repo.set_pinned(post_id, pinned).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: post_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::PinPost,
            Some(post_id.0),
            Some("post".to_owned()),
            Some(serde_json::json!({ "pinned": pinned })),
        )
        .await;
        info!(post_id = %post_id, pinned, "post pinned status updated");
        Ok(())
    }

    /// Issue an IP ban and record an audit entry.
    ///
    /// Returns the assigned `BanId`.
    #[instrument(skip(self), fields(actor_id = %actor_id))]
    pub async fn ban_ip(
        &self,
        ip_hash: IpHash,
        reason: String,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        actor_id: UserId,
    ) -> Result<BanId, ModerationError> {
        let ban = Ban {
            id:         BanId::new(),
            ip_hash:    ip_hash.clone(),
            banned_by:  actor_id,
            reason:     reason.clone(),
            expires_at,
            created_at: now_utc(),
        };
        let ban_id = self.ban_repo.save(&ban).await?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::BanIp,
            Some(ban_id.0),
            Some("ban".to_owned()),
            Some(serde_json::json!({ "reason": reason, "expires_at": expires_at })),
        )
        .await;
        info!(ban_id = %ban_id, "ip banned");
        Ok(ban_id)
    }

    /// Expire a ban immediately and record an audit entry.
    ///
    /// Returns `ModerationError::NotFound` if the ban does not exist.
    #[instrument(skip(self), fields(ban_id = %ban_id, actor_id = %actor_id))]
    pub async fn expire_ban(
        &self,
        ban_id: BanId,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.ban_repo.expire(ban_id).await.map_err(|e| match e {
            DomainError::NotFound { .. } => ModerationError::NotFound {
                resource: ban_id.to_string(),
            },
            other => ModerationError::Internal(other),
        })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::ExpireBan,
            Some(ban_id.0),
            Some("ban".to_owned()),
            None,
        )
        .await;
        info!(ban_id = %ban_id, "ban expired");
        Ok(())
    }

    /// Resolve (approve or reject) a flag and record an audit entry.
    ///
    /// Returns `ModerationError::NotFound` if the flag does not exist.
    #[instrument(skip(self), fields(flag_id = %flag_id, actor_id = %actor_id))]
    pub async fn resolve_flag(
        &self,
        flag_id: FlagId,
        resolution: FlagResolution,
        actor_id: UserId,
    ) -> Result<(), ModerationError> {
        self.flag_repo
            .resolve(flag_id, resolution, actor_id)
            .await
            .map_err(|e| match e {
                DomainError::NotFound { .. } => ModerationError::NotFound {
                    resource: flag_id.to_string(),
                },
                other => ModerationError::Internal(other),
            })?;
        self.write_audit(
            Some(actor_id),
            None,
            AuditAction::ResolveFlag,
            Some(flag_id.0),
            Some("flag".to_owned()),
            Some(serde_json::json!({ "resolution": format!("{:?}", resolution) })),
        )
        .await;
        info!(flag_id = %flag_id, ?resolution, "flag resolved");
        Ok(())
    }

    /// List pending flags for the moderation queue.
    pub async fn list_pending_flags(
        &self,
        page: Page,
    ) -> Result<Paginated<domains::models::Flag>, ModerationError> {
        Ok(self.flag_repo.find_pending(page).await?)
    }

    /// Fetch a thread by its UUID — used by the flag handler to resolve the OP post_id.
    pub async fn get_thread(
        &self,
        thread_id: domains::models::ThreadId,
    ) -> Result<domains::models::Thread, ModerationError> {
        self.thread_repo
            .find_by_id(thread_id)
            .await
            .map_err(|e| match e {
                DomainError::NotFound { .. } => ModerationError::NotFound {
                    resource: format!("thread {thread_id}"),
                },
                other => ModerationError::Internal(other),
            })
    }

    /// Submit a user flag on a post, creating a new moderation queue entry.
    ///
    /// `reason` is the reporter's description of the rule violation.
    /// `reporter_ip_hash` is the hashed IP of the reporter.
    /// Returns the assigned `FlagId`.
    #[instrument(skip(self), fields(post_id = %post_id))]
    pub async fn file_flag(
        &self,
        post_id:            PostId,
        reason:             String,
        reporter_ip_hash:   IpHash,
    ) -> Result<FlagId, ModerationError> {
        let flag = domains::models::Flag {
            id:               FlagId(Uuid::new_v4()),
            post_id,
            reason,
            reporter_ip_hash,
            status:           domains::models::FlagStatus::Pending,
            resolved_by:      None,
            created_at:       Utc::now(),
        };
        let flag_id = self.flag_repo.save(&flag).await?;
        info!(flag_id = %flag_id, post_id = %post_id, "flag filed");
        Ok(flag_id)
    }

    /// List all bans (active and expired) for review.
    pub async fn list_bans(
        &self,
        page: Page,
    ) -> Result<Paginated<Ban>, ModerationError> {
        Ok(self.ban_repo.find_all(page).await?)
    }

    /// Fetch the `n` most recent audit log entries for dashboard display.
    pub async fn recent_audit_entries(
        &self,
        n: u32,
    ) -> Result<Vec<AuditEntry>, ModerationError> {
        Ok(self.audit_repo.find_recent(n).await?)
    }

    /// Full paginated audit log — all entries, newest first.
    ///
    /// Used by the Janitor audit log page (`/janitor/logs`).
    pub async fn audit_log_all(
        &self,
        page: Page,
    ) -> Result<domains::models::Paginated<AuditEntry>, ModerationError> {
        Ok(self.audit_repo.find_all(page).await?)
    }

    /// Paginated audit log scoped to a single board.
    ///
    /// Used by Board Owner (`/board-owner/logs`) and Volunteer (`/volunteer/logs`) pages.
    /// Entries are matched by the `board_id` field in their JSON `details` column.
    pub async fn audit_log_for_board(
        &self,
        board_id: domains::models::BoardId,
        page: Page,
    ) -> Result<domains::models::Paginated<AuditEntry>, ModerationError> {
        Ok(self.audit_repo.find_by_board(board_id, page).await?)
    }

    /// Write an audit log entry. Failures are logged and swallowed.
    ///
    /// # INVARIANT
    /// Audit log write failures MUST NOT propagate to the caller. The primary
    /// moderation action has already succeeded at this point. Failing to log it
    /// is serious but less bad than rolling back the action.
    async fn write_audit(
        &self,
        actor_id: Option<UserId>,
        actor_ip_hash: Option<IpHash>,
        action: AuditAction,
        target_id: Option<Uuid>,
        target_type: Option<String>,
        details: Option<serde_json::Value>,
    ) {
        let entry = AuditEntry {
            id:            Uuid::new_v4(),
            actor_id,
            actor_ip_hash,
            action,
            target_id,
            target_type,
            details,
            created_at:    now_utc(),
        };
        if let Err(e) = self.audit_repo.record(&entry).await {
            error!(error = %e, "audit log write failed — action was performed but not logged");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domains::ports::{
        MockAuditRepository, MockBanRepository, MockFlagRepository, MockPostRepository,
        MockThreadRepository, MockUserRepository,
    };

    fn make_service() -> ModerationService<
        MockBanRepository,
        MockPostRepository,
        MockThreadRepository,
        MockFlagRepository,
        MockAuditRepository,
        MockUserRepository,
    > {
        let mut audit = MockAuditRepository::new();
        audit.expect_record().returning(|_| Ok(()));
        ModerationService::new(
            MockBanRepository::new(),
            MockPostRepository::new(),
            MockThreadRepository::new(),
            MockFlagRepository::new(),
            audit,
            MockUserRepository::new(),
        )
    }

    #[tokio::test]
    async fn delete_post_happy_path() {
        let mut svc = make_service();
        svc.post_repo
            .expect_delete()
            .times(1)
            .returning(|_| Ok(()));

        let result = svc.delete_post(PostId::new(), UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn delete_post_not_found() {
        let mut svc = make_service();
        svc.post_repo
            .expect_delete()
            .times(1)
            .returning(|_| Err(DomainError::not_found("post")));

        let result = svc.delete_post(PostId::new(), UserId::new()).await;
        assert!(matches!(result, Err(ModerationError::NotFound { .. })));
    }

    #[tokio::test]
    async fn ban_ip_creates_ban() {
        let mut svc = make_service();
        svc.ban_repo
            .expect_save()
            .times(1)
            .returning(|b| Ok(b.id));

        let result = svc
            .ban_ip(IpHash::new("abc"), "spam".to_owned(), None, UserId::new())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn resolve_flag_happy_path() {
        let mut svc = make_service();
        svc.flag_repo
            .expect_resolve()
            .times(1)
            .returning(|_, _, _| Ok(()));

        let result = svc
            .resolve_flag(FlagId::new(), FlagResolution::Approved, UserId::new())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn resolve_flag_rejected_variant() {
        let mut svc = make_service();
        svc.flag_repo
            .expect_resolve()
            .times(1)
            .returning(|_, _, _| Ok(()));

        let result = svc
            .resolve_flag(FlagId::new(), FlagResolution::Rejected, UserId::new())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn delete_thread_happy_path() {
        let mut svc = make_service();
        svc.thread_repo
            .expect_delete()
            .times(1)
            .returning(|_| Ok(()));

        let result = svc.delete_thread(ThreadId::new(), UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn delete_thread_not_found() {
        let mut svc = make_service();
        svc.thread_repo
            .expect_delete()
            .times(1)
            .returning(|_| Err(DomainError::not_found("thread")));

        let result = svc.delete_thread(ThreadId::new(), UserId::new()).await;
        assert!(matches!(result, Err(ModerationError::NotFound { .. })));
    }

    #[tokio::test]
    async fn set_sticky_true() {
        let mut svc = make_service();
        svc.thread_repo
            .expect_set_sticky()
            .withf(|_, sticky| *sticky)
            .times(1)
            .returning(|_, _| Ok(()));

        let result = svc.set_sticky(ThreadId::new(), true, UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_sticky_false() {
        let mut svc = make_service();
        svc.thread_repo
            .expect_set_sticky()
            .withf(|_, sticky| !sticky)
            .times(1)
            .returning(|_, _| Ok(()));

        let result = svc.set_sticky(ThreadId::new(), false, UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_closed_true() {
        let mut svc = make_service();
        svc.thread_repo
            .expect_set_closed()
            .withf(|_, closed| *closed)
            .times(1)
            .returning(|_, _| Ok(()));

        let result = svc.set_closed(ThreadId::new(), true, UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_closed_false() {
        let mut svc = make_service();
        svc.thread_repo
            .expect_set_closed()
            .withf(|_, closed| !closed)
            .times(1)
            .returning(|_, _| Ok(()));

        let result = svc.set_closed(ThreadId::new(), false, UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn ban_ip_with_expiry() {
        use chrono::Duration;
        let mut svc = make_service();
        svc.ban_repo
            .expect_save()
            .times(1)
            .returning(|b| Ok(b.id));

        let expires_at = Some(now_utc() + Duration::hours(24));
        let result = svc
            .ban_ip(IpHash::new("abc"), "spam".to_owned(), expires_at, UserId::new())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn expire_ban_happy_path() {
        let mut svc = make_service();
        svc.ban_repo
            .expect_expire()
            .times(1)
            .returning(|_| Ok(()));

        let result = svc.expire_ban(BanId::new(), UserId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn expire_ban_not_found() {
        let mut svc = make_service();
        svc.ban_repo
            .expect_expire()
            .times(1)
            .returning(|_| Err(DomainError::not_found("ban")));

        let result = svc.expire_ban(BanId::new(), UserId::new()).await;
        assert!(matches!(result, Err(ModerationError::NotFound { .. })));
    }

    #[tokio::test]
    async fn file_flag_happy_path() {
        let mut svc = make_service();
        svc.flag_repo
            .expect_save()
            .times(1)
            .returning(|f| Ok(f.id));

        let result = svc
            .file_flag(PostId::new(), "off-topic".to_owned(), IpHash::new("xyz"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn audit_log_failure_does_not_propagate() {
        // delete_post succeeds even when the audit log write fails
        let mut audit = MockAuditRepository::new();
        audit.expect_record().returning(|_| Err(DomainError::internal("db gone")));

        let svc = ModerationService::new(
            MockBanRepository::new(),
            {
                let mut m = MockPostRepository::new();
                m.expect_delete().returning(|_| Ok(()));
                m
            },
            MockThreadRepository::new(),
            MockFlagRepository::new(),
            audit,
            MockUserRepository::new(),
        );

        let result = svc.delete_post(PostId::new(), UserId::new()).await;
        assert!(result.is_ok(), "audit failure must not propagate");
    }
}
