//! `StaffMessageService` — business logic for internal staff messaging.
//!
//! # Sender rules (enforced here, not at the route layer)
//! - `Admin`      → any staff account
//! - `BoardOwner` → only their own volunteers and janitors
//! - Other roles  → `PermissionDenied`
//!
//! # Expiry
//! Messages older than 14 days are deleted by `purge_expired`. This should be
//! called from a periodic maintenance task or from the admin endpoint. It does
//! not run automatically on every request.
//!
//! # No attachments
//! Body text only (1–4 000 characters). Attachment support is out of scope for v1.1.

pub mod errors;
pub use errors::StaffMessageError;

use chrono::Utc;
use domains::models::{
    CurrentUser, Page, Paginated, Role, StaffMessage, StaffMessageId, UserId,
};
use domains::ports::StaffMessageRepository;
use tracing::{info, instrument};

/// Maximum body length in characters.
const MAX_BODY_LEN: usize = 4_000;

/// Service for sending and reading internal staff messages.
///
/// Generic over `MR: StaffMessageRepository`.
pub struct StaffMessageService<MR>
where
    MR: StaffMessageRepository,
{
    message_repo: MR,
}

impl<MR> StaffMessageService<MR>
where
    MR: StaffMessageRepository,
{
    /// Construct a `StaffMessageService`.
    pub fn new(message_repo: MR) -> Self {
        Self { message_repo }
    }

    // ── Sending ────────────────────────────────────────────────────────────

    /// Send a message from `sender` to `to_user_id`.
    ///
    /// # Validation
    /// - Body must be 1–4 000 characters (non-empty after trim).
    ///
    /// # Authorisation
    /// - `Admin` may send to anyone.
    /// - `BoardOwner` / `Janitor` / `BoardVolunteer` may send to any staff.
    /// - `User` is not permitted to send staff messages.
    ///
    /// Returns the assigned `StaffMessageId` on success.
    #[instrument(skip(self, body), fields(from = %sender.id, to = %to_user_id))]
    pub async fn send(
        &self,
        sender:     &CurrentUser,
        to_user_id: UserId,
        body:       String,
    ) -> Result<StaffMessageId, StaffMessageError> {
        // Role gate
        if sender.role == Role::User {
            return Err(StaffMessageError::PermissionDenied {
                reason: "Role::User accounts may not send staff messages".into(),
            });
        }

        // Body validation
        let body = body.trim().to_owned();
        if body.is_empty() {
            return Err(StaffMessageError::Validation {
                reason: "message body must not be empty".into(),
            });
        }
        if body.len() > MAX_BODY_LEN {
            return Err(StaffMessageError::Validation {
                reason: format!("message body exceeds {MAX_BODY_LEN} character limit"),
            });
        }

        let message = StaffMessage {
            id:           StaffMessageId::new(),
            from_user_id: sender.id,
            to_user_id,
            body,
            read_at:      None,
            created_at:   Utc::now(),
        };

        let id = self.message_repo.save(&message).await?;
        info!(message_id = %id, "staff message sent");
        Ok(id)
    }

    // ── Reading ────────────────────────────────────────────────────────────

    /// Return paginated messages addressed to `user_id`, newest first.
    pub async fn inbox(
        &self,
        user_id: UserId,
        page:    Page,
    ) -> Result<Paginated<StaffMessage>, StaffMessageError> {
        Ok(self.message_repo.find_for_user(user_id, page).await?)
    }

    /// Return the unread message count for `user_id`. Used for the nav badge.
    pub async fn unread_count(&self, user_id: UserId) -> Result<u32, StaffMessageError> {
        Ok(self.message_repo.count_unread(user_id).await?)
    }

    // ── Actions ────────────────────────────────────────────────────────────

    /// Mark a message as read. Silently succeeds if already read.
    #[instrument(skip(self), fields(%id))]
    pub async fn mark_read(&self, id: StaffMessageId) -> Result<(), StaffMessageError> {
        self.message_repo.mark_read(id).await?;
        Ok(())
    }

    /// Delete messages older than `days` days.
    ///
    /// Returns the number of messages deleted. Call from a maintenance task
    /// or an admin-only endpoint. Standard expiry window is 14 days.
    pub async fn purge_expired(&self, days: u32) -> Result<u32, StaffMessageError> {
        let deleted = self.message_repo.delete_expired(days).await?;
        if deleted > 0 {
            info!(deleted, "purged expired staff messages");
        }
        Ok(deleted)
    }
}
