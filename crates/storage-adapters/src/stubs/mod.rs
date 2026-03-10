//! In-process stub adapters for ports that have no concrete implementation yet.
//!
//! These stubs allow the binary to compile and start without requiring every
//! adapter to be fully implemented. They return empty results on reads and
//! silently succeed on writes. **Not suitable for production use.**
//!
//! Each stub is replaced by a real adapter in the release it is scheduled for.
//!
//! | Stub | Replaced by | Version |
//! |------|-------------|---------|
//! | `NoopStaffRequestRepository` | `PgStaffRequestRepository` | v1.1 ✅ (now shipped) |

use async_trait::async_trait;
use domains::errors::DomainError;
use domains::models::{Slug, StaffRequest, StaffRequestId, StaffRequestStatus, UserId};
use domains::ports::StaffRequestRepository;

/// No-op `StaffRequestRepository` — returns empty collections and quietly
/// accepts writes without persisting anything.
///
/// Retained for integration tests that do not need real persistence.
/// Production wiring uses `PgStaffRequestRepository` (see `composition.rs`).
pub struct NoopStaffRequestRepository;

#[async_trait]
impl StaffRequestRepository for NoopStaffRequestRepository {
    async fn save(&self, _request: &StaffRequest) -> Result<(), DomainError> {
        Ok(())
    }

    async fn find_by_id(&self, id: StaffRequestId) -> Result<StaffRequest, DomainError> {
        Err(DomainError::not_found(format!("staff_request:{id}")))
    }

    async fn find_by_user(&self, _user_id: UserId) -> Result<Vec<StaffRequest>, DomainError> {
        Ok(vec![])
    }

    async fn find_pending(&self) -> Result<Vec<StaffRequest>, DomainError> {
        Ok(vec![])
    }

    async fn find_pending_for_board(
        &self,
        _slug: &Slug,
    ) -> Result<Vec<StaffRequest>, DomainError> {
        Ok(vec![])
    }

    async fn update_status(
        &self,
        id:          StaffRequestId,
        _status:     StaffRequestStatus,
        _reviewed_by: UserId,
        _note:       Option<String>,
    ) -> Result<(), DomainError> {
        Err(DomainError::not_found(format!("staff_request:{id}")))
    }
}
