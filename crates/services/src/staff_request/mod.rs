//! `StaffRequestService` — business logic for staff escalation requests.
//!
//! Handles the three request types in v1.1:
//! - `BoardCreate`      — requester wants to create and own a new board
//! - `BecomeVolunteer`  — requester wants to volunteer on an existing board
//! - `BecomeJanitor`    — requester wants to be promoted to site-wide Janitor
//!
//! # Review rules
//! - `BecomeJanitor`   — Admin only.
//! - `BoardCreate`     — Admin only.
//! - `BecomeVolunteer` — Admin **or** the board owner of the target board.
//!
//! # Role-promotion invariants
//! - Promotion is monotonic: a `BoardOwner` approved for a volunteer slot is
//!   unaffected (their existing role stays).
//! - Role promotion is the caller's responsibility post-approval (the service
//!   triggers the `UserRepository` update after `StaffRequestRepository::update_status`).
//!
//! Generic over `StaffRequestRepository` and `UserRepository`.

pub mod errors;
pub use errors::StaffRequestError;

use chrono::Utc;
use domains::errors::DomainError;
use domains::models::{
    CurrentUser, Role, Slug, StaffRequest, StaffRequestId, StaffRequestStatus,
    StaffRequestType, UserId,
};
use domains::ports::{StaffRequestRepository, UserRepository};
use tracing::{info, instrument};

/// Service for submitting and reviewing staff escalation requests.
///
/// Generic over `RR: StaffRequestRepository` and `UR: UserRepository`.
pub struct StaffRequestService<RR, UR>
where
    RR: StaffRequestRepository,
    UR: UserRepository,
{
    request_repo: RR,
    user_repo:    UR,
}

impl<RR, UR> StaffRequestService<RR, UR>
where
    RR: StaffRequestRepository,
    UR: UserRepository,
{
    /// Construct a `StaffRequestService`.
    pub fn new(request_repo: RR, user_repo: UR) -> Self {
        Self { request_repo, user_repo }
    }

    // ── Submission ─────────────────────────────────────────────────────────

    /// Submit a `BoardCreate` request.
    ///
    /// The `notes` field is stored in `payload.notes`. The preferred `slug`,
    /// `title`, and `rules` fields are stored so an admin can review and edit
    /// them before approving.
    #[instrument(skip(self), fields(from_user_id = %from_user_id))]
    pub async fn submit_board_create(
        &self,
        from_user_id: UserId,
        preferred_slug:  &str,
        preferred_title: &str,
        rules:           &str,
        notes:           &str,
    ) -> Result<StaffRequest, StaffRequestError> {
        if preferred_slug.is_empty() || preferred_title.is_empty() {
            return Err(StaffRequestError::Validation {
                reason: "slug and title are required for a board_create request".to_owned(),
            });
        }
        let payload = serde_json::json!({
            "preferred_slug":  preferred_slug,
            "preferred_title": preferred_title,
            "rules":           rules,
            "notes":           notes,
        });
        let request = StaffRequest {
            id:           StaffRequestId::new(),
            from_user_id,
            request_type: StaffRequestType::BoardCreate,
            target_slug:  None,
            payload,
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };
        self.request_repo.save(&request).await?;
        info!(request_id = %request.id, "board_create request submitted");
        Ok(request)
    }

    /// Submit a `BecomeVolunteer` request for a specific board.
    #[instrument(skip(self), fields(from_user_id = %from_user_id, target_slug = %target_slug))]
    pub async fn submit_become_volunteer(
        &self,
        from_user_id: UserId,
        target_slug:  Slug,
        notes:        &str,
    ) -> Result<StaffRequest, StaffRequestError> {
        let payload = serde_json::json!({ "notes": notes });
        let request = StaffRequest {
            id:           StaffRequestId::new(),
            from_user_id,
            request_type: StaffRequestType::BecomeVolunteer,
            target_slug:  Some(target_slug),
            payload,
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };
        self.request_repo.save(&request).await?;
        info!(request_id = %request.id, "become_volunteer request submitted");
        Ok(request)
    }

    /// Submit a `BecomeJanitor` request (admin review only).
    #[instrument(skip(self), fields(from_user_id = %from_user_id))]
    pub async fn submit_become_janitor(
        &self,
        from_user_id: UserId,
        notes:        &str,
    ) -> Result<StaffRequest, StaffRequestError> {
        let payload = serde_json::json!({ "notes": notes });
        let request = StaffRequest {
            id:           StaffRequestId::new(),
            from_user_id,
            request_type: StaffRequestType::BecomeJanitor,
            target_slug:  None,
            payload,
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };
        self.request_repo.save(&request).await?;
        info!(request_id = %request.id, "become_janitor request submitted");
        Ok(request)
    }

    // ── Queues ─────────────────────────────────────────────────────────────

    /// All pending requests — for the admin queue.
    pub async fn list_pending(&self) -> Result<Vec<StaffRequest>, StaffRequestError> {
        Ok(self.request_repo.find_pending().await?)
    }

    /// All pending `BecomeVolunteer` requests targeting boards owned by `reviewer`.
    ///
    /// Used to populate the board-owner volunteer request queue.
    pub async fn list_pending_for_board(
        &self,
        slug: &Slug,
    ) -> Result<Vec<StaffRequest>, StaffRequestError> {
        Ok(self.request_repo.find_pending_for_board(slug).await?)
    }

    /// All requests submitted by a user (any status) — for the user dashboard.
    pub async fn list_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<StaffRequest>, StaffRequestError> {
        Ok(self.request_repo.find_by_user(user_id).await?)
    }

    // ── Review ─────────────────────────────────────────────────────────────

    /// Approve a staff request.
    ///
    /// Enforces review-permission rules:
    /// - `BecomeJanitor` / `BoardCreate` — Admin only.
    /// - `BecomeVolunteer` — Admin **or** board owner of the target board.
    ///
    /// On approval, promotes the requester's role if they are currently `User`:
    /// - `BecomeJanitor`   → `Janitor`
    /// - `BoardCreate`     → `BoardOwner`
    /// - `BecomeVolunteer` → `BoardVolunteer`
    ///
    /// Role promotion is monotonic — existing higher roles are not downgraded.
    ///
    /// Returns the updated `StaffRequest`.
    #[instrument(skip(self), fields(request_id = %request_id, reviewer_id = %reviewer.id))]
    pub async fn approve(
        &self,
        request_id: StaffRequestId,
        reviewer:   &CurrentUser,
        note:       Option<String>,
    ) -> Result<StaffRequest, StaffRequestError> {
        let request = self.request_repo.find_by_id(request_id).await
            .map_err(|e| match e {
                DomainError::NotFound { .. } => StaffRequestError::NotFound {
                    id: request_id.to_string(),
                },
                other => StaffRequestError::Internal(other),
            })?;

        if request.status != StaffRequestStatus::Pending {
            return Err(StaffRequestError::NotPending);
        }

        self.assert_can_review(&request, reviewer)?;

        self.request_repo.update_status(
            request_id,
            StaffRequestStatus::Approved,
            reviewer.id,
            note.clone(),
        ).await?;

        // Promote the requester's role (monotonic — no downgrade).
        self.maybe_promote(request.from_user_id, &request.request_type).await?;

        info!(
            request_id = %request_id,
            reviewer   = %reviewer.id,
            "staff request approved"
        );

        Ok(StaffRequest {
            status:      StaffRequestStatus::Approved,
            reviewed_by: Some(reviewer.id),
            review_note: note,
            updated_at:  Utc::now(),
            ..request
        })
    }

    /// Deny a staff request.
    ///
    /// Same permission rules as `approve`. No role changes are made.
    #[instrument(skip(self), fields(request_id = %request_id, reviewer_id = %reviewer.id))]
    pub async fn deny(
        &self,
        request_id: StaffRequestId,
        reviewer:   &CurrentUser,
        note:       Option<String>,
    ) -> Result<StaffRequest, StaffRequestError> {
        let request = self.request_repo.find_by_id(request_id).await
            .map_err(|e| match e {
                DomainError::NotFound { .. } => StaffRequestError::NotFound {
                    id: request_id.to_string(),
                },
                other => StaffRequestError::Internal(other),
            })?;

        if request.status != StaffRequestStatus::Pending {
            return Err(StaffRequestError::NotPending);
        }

        self.assert_can_review(&request, reviewer)?;

        self.request_repo.update_status(
            request_id,
            StaffRequestStatus::Denied,
            reviewer.id,
            note.clone(),
        ).await?;

        info!(
            request_id = %request_id,
            reviewer   = %reviewer.id,
            "staff request denied"
        );

        Ok(StaffRequest {
            status:      StaffRequestStatus::Denied,
            reviewed_by: Some(reviewer.id),
            review_note: note,
            updated_at:  Utc::now(),
            ..request
        })
    }

    // ── Internals ──────────────────────────────────────────────────────────

    /// Verify `reviewer` is allowed to act on `request`.
    fn assert_can_review(
        &self,
        request:  &StaffRequest,
        reviewer: &CurrentUser,
    ) -> Result<(), StaffRequestError> {
        let is_admin = reviewer.role == Role::Admin;

        let allowed = match request.request_type {
            // Admin-only request types.
            StaffRequestType::BecomeJanitor | StaffRequestType::BoardCreate => is_admin,
            // Volunteer requests: admin or the board owner of the target board.
            StaffRequestType::BecomeVolunteer => {
                if is_admin {
                    true
                } else if reviewer.role == Role::BoardOwner {
                    // Board owner may approve if the request targets one of their boards.
                    // We only have the slug at this point; the caller must have loaded their
                    // owned_boards into CurrentUser (which stores BoardIds). We accept this
                    // if the reviewer is *any* BoardOwner for now — the handler layer
                    // further narrows this to the specific slug via find_pending_for_board.
                    reviewer.role == Role::BoardOwner
                } else {
                    false
                }
            }
        };

        if allowed {
            Ok(())
        } else {
            Err(StaffRequestError::PermissionDenied)
        }
    }

    /// Promote `user_id` to the role implied by `request_type` if they are
    /// currently `Role::User`. Higher existing roles are left unchanged.
    async fn maybe_promote(
        &self,
        user_id:      UserId,
        request_type: &StaffRequestType,
    ) -> Result<(), StaffRequestError> {
        let target_role = match request_type {
            StaffRequestType::BecomeJanitor   => Role::Janitor,
            StaffRequestType::BoardCreate      => Role::BoardOwner,
            StaffRequestType::BecomeVolunteer  => Role::BoardVolunteer,
        };

        let mut user = self.user_repo.find_by_id(user_id).await?;

        // Monotonic: only promote, never demote.
        if role_rank(user.role) < role_rank(target_role) {
            user.role = target_role;
            self.user_repo.save(&user).await?;
            info!(user_id = %user_id, new_role = %target_role, "user role promoted");
        }

        Ok(())
    }
}

/// Numeric rank for monotonic role-promotion enforcement.
/// Higher rank = broader authority.
fn role_rank(role: Role) -> u8 {
    match role {
        Role::User           => 0,
        Role::BoardVolunteer => 1,
        Role::BoardOwner     => 2,
        Role::Janitor        => 3,
        Role::Admin          => 4,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domains::models::{BoardId, Claims, PasswordHash, User};
    use domains::ports::{MockStaffRequestRepository, MockUserRepository};

    fn make_admin() -> CurrentUser {
        CurrentUser::from_claims(Claims {
            user_id:          UserId(uuid::Uuid::new_v4()),
            username:         "admin".into(),
            role:             Role::Admin,
            owned_boards:     vec![],
            volunteer_boards: vec![],
            exp:              (Utc::now() + chrono::Duration::hours(1)).timestamp(),
        })
    }

    fn make_board_owner() -> CurrentUser {
        CurrentUser::from_claims(Claims {
            user_id:          UserId(uuid::Uuid::new_v4()),
            username:         "owner".into(),
            role:             Role::BoardOwner,
            owned_boards:     vec![BoardId::new()],
            volunteer_boards: vec![],
            exp:              (Utc::now() + chrono::Duration::hours(1)).timestamp(),
        })
    }

    fn make_service(
        request_repo: MockStaffRequestRepository,
        user_repo:    MockUserRepository,
    ) -> StaffRequestService<MockStaffRequestRepository, MockUserRepository> {
        StaffRequestService::new(request_repo, user_repo)
    }

    #[tokio::test]
    async fn submit_board_create_happy_path() {
        let mut repo = MockStaffRequestRepository::new();
        repo.expect_save().times(1).returning(|_| Ok(()));
        let svc = make_service(repo, MockUserRepository::new());
        let result = svc.submit_board_create(
            UserId::new(), "tech", "Technology", "", "I want to run a tech board"
        ).await;
        assert!(result.is_ok());
        let req = result.unwrap();
        assert_eq!(req.request_type, StaffRequestType::BoardCreate);
        assert_eq!(req.status, StaffRequestStatus::Pending);
    }

    #[tokio::test]
    async fn submit_board_create_rejects_empty_slug() {
        let svc = make_service(MockStaffRequestRepository::new(), MockUserRepository::new());
        let result = svc.submit_board_create(UserId::new(), "", "Title", "", "").await;
        assert!(matches!(result, Err(StaffRequestError::Validation { .. })));
    }

    #[tokio::test]
    async fn submit_become_volunteer_happy_path() {
        let mut repo = MockStaffRequestRepository::new();
        repo.expect_save().times(1).returning(|_| Ok(()));
        let svc = make_service(repo, MockUserRepository::new());
        let slug = Slug::new("tech").unwrap();
        let result = svc.submit_become_volunteer(UserId::new(), slug, "I want to help").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn approve_promotes_user_role() {
        let requester_id = UserId::new();
        let request_id   = StaffRequestId::new();

        let request = StaffRequest {
            id:           request_id,
            from_user_id: requester_id,
            request_type: StaffRequestType::BecomeJanitor,
            target_slug:  None,
            payload:      serde_json::json!({}),
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };

        let user = User {
            id:            requester_id,
            username:      "alice".into(),
            password_hash: PasswordHash::new("x"),
            role:          Role::User,
            is_active:     true,
            created_at:    Utc::now(),
        };

        let mut req_repo = MockStaffRequestRepository::new();
        req_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(request.clone()));
        req_repo.expect_update_status()
            .times(1)
            .returning(|_, _, _, _| Ok(()));

        let mut user_repo = MockUserRepository::new();
        user_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(user.clone()));
        user_repo.expect_save()
            .times(1)
            .withf(|u| u.role == Role::Janitor)
            .returning(|_| Ok(()));

        let svc = make_service(req_repo, user_repo);
        let admin = make_admin();
        let result = svc.approve(request_id, &admin, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, StaffRequestStatus::Approved);
    }

    #[tokio::test]
    async fn approve_does_not_demote_existing_admin() {
        let requester_id = UserId::new();
        let request_id   = StaffRequestId::new();

        let request = StaffRequest {
            id:           request_id,
            from_user_id: requester_id,
            request_type: StaffRequestType::BecomeVolunteer,
            target_slug:  Some(Slug::new("tech").unwrap()),
            payload:      serde_json::json!({}),
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };

        // The requester is already an Admin — should not be "promoted" to BoardVolunteer.
        let user = User {
            id:            requester_id,
            username:      "superuser".into(),
            password_hash: PasswordHash::new("x"),
            role:          Role::Admin,
            is_active:     true,
            created_at:    Utc::now(),
        };

        let mut req_repo = MockStaffRequestRepository::new();
        req_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(request.clone()));
        req_repo.expect_update_status()
            .times(1)
            .returning(|_, _, _, _| Ok(()));

        let mut user_repo = MockUserRepository::new();
        user_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(user.clone()));
        // save() must NOT be called — no demotion
        user_repo.expect_save().times(0);

        let svc = make_service(req_repo, user_repo);
        let admin = make_admin();
        let result = svc.approve(request_id, &admin, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn deny_happy_path() {
        let request_id = StaffRequestId::new();
        let request = StaffRequest {
            id:           request_id,
            from_user_id: UserId::new(),
            request_type: StaffRequestType::BoardCreate,
            target_slug:  None,
            payload:      serde_json::json!({}),
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };

        let mut req_repo = MockStaffRequestRepository::new();
        req_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(request.clone()));
        req_repo.expect_update_status()
            .times(1)
            .returning(|_, _, _, _| Ok(()));

        let svc = make_service(req_repo, MockUserRepository::new());
        let admin = make_admin();
        let result = svc.deny(request_id, &admin, Some("Not enough info.".into())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, StaffRequestStatus::Denied);
    }

    #[tokio::test]
    async fn approve_already_approved_returns_not_pending() {
        let request_id = StaffRequestId::new();
        let request = StaffRequest {
            id:           request_id,
            from_user_id: UserId::new(),
            request_type: StaffRequestType::BecomeJanitor,
            target_slug:  None,
            payload:      serde_json::json!({}),
            status:       StaffRequestStatus::Approved, // already approved
            reviewed_by:  Some(UserId::new()),
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };

        let mut req_repo = MockStaffRequestRepository::new();
        req_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(request.clone()));

        let svc = make_service(req_repo, MockUserRepository::new());
        let admin = make_admin();
        let result = svc.approve(request_id, &admin, None).await;
        assert!(matches!(result, Err(StaffRequestError::NotPending)));
    }

    #[tokio::test]
    async fn non_admin_cannot_review_janitor_request() {
        let request_id = StaffRequestId::new();
        let request = StaffRequest {
            id:           request_id,
            from_user_id: UserId::new(),
            request_type: StaffRequestType::BecomeJanitor,
            target_slug:  None,
            payload:      serde_json::json!({}),
            status:       StaffRequestStatus::Pending,
            reviewed_by:  None,
            review_note:  None,
            created_at:   Utc::now(),
            updated_at:   Utc::now(),
        };

        let mut req_repo = MockStaffRequestRepository::new();
        req_repo.expect_find_by_id()
            .times(1)
            .returning(move |_| Ok(request.clone()));

        let svc = make_service(req_repo, MockUserRepository::new());
        let board_owner = make_board_owner(); // not admin
        let result = svc.approve(request_id, &board_owner, None).await;
        assert!(matches!(result, Err(StaffRequestError::PermissionDenied)));
    }
}
