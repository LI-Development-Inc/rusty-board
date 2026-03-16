//! In-memory session repository.
//!
//! Implements `SessionRepository` using a `DashMap`. Suitable for:
//! - `CookieAuthProvider` in single-instance development environments
//! - Integration tests that exercise cookie auth without a live Postgres instance
//! - CI pipelines without database services
//!
//! **Not suitable for production multi-instance deployments.** Sessions are
//! per-process and will not be shared across pods or restart boundaries.

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use domains::errors::DomainError;
use domains::models::UserId;
use domains::ports::{Session, SessionRepository};
use std::sync::Arc;

/// In-memory `SessionRepository` backed by a `DashMap`.
///
/// Expired entries are removed lazily on access and eagerly via `purge_expired()`.
pub struct InMemorySessionRepository {
    sessions: Arc<DashMap<String, Session>>,
}

impl InMemorySessionRepository {
    /// Create a new empty in-memory session store.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemorySessionRepository {
    /// Delegates to [`InMemorySessionRepository::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemorySessionRepository {
    /// Clones the repository by cloning the inner `Arc` — all clones share the same session store.
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
        }
    }
}

#[async_trait]
impl SessionRepository for InMemorySessionRepository {
    async fn save(&self, session: &Session) -> Result<(), DomainError> {
        self.sessions.insert(session.session_id.clone(), session.clone());
        Ok(())
    }

    async fn find_by_id(&self, session_id: &str) -> Result<Session, DomainError> {
        // Check expiry while holding the read guard, then drop it before any mutation.
        let expired = match self.sessions.get(session_id) {
            Some(s) if s.expires_at > Utc::now() => return Ok(s.clone()),
            Some(_) => true,  // expired
            None    => false, // not found
        };
        // Read guard is dropped here. Safe to call remove() without deadlocking.
        if expired {
            self.sessions.remove(session_id);
        }
        Err(DomainError::Auth)
    }

    /// Removes the session from the map. Silently succeeds if the session does not exist.
    async fn delete(&self, session_id: &str) -> Result<(), DomainError> {
        self.sessions.remove(session_id);
        Ok(())
    }

    /// Removes all sessions for the given user. Used on account deactivation or forced logout.
    async fn delete_for_user(&self, user_id: UserId) -> Result<(), DomainError> {
        self.sessions.retain(|_, s| s.user_id != user_id);
        Ok(())
    }

    /// Removes all sessions whose `expires_at` is in the past.
    async fn purge_expired(&self) -> Result<(), DomainError> {
        let now = Utc::now();
        self.sessions.retain(|_, s| s.expires_at > now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use domains::models::UserId;

    fn make_session(id: &str, ttl_secs: i64) -> Session {
        Session {
            session_id:  id.to_owned(),
            user_id:     UserId::new(),
            claims_json: r#"{"role":"janitor"}"#.to_owned(),
            expires_at:  Utc::now() + Duration::seconds(ttl_secs),
        }
    }

    #[tokio::test]
    async fn save_and_find() {
        let repo = InMemorySessionRepository::new();
        let s = make_session("abc", 3600);
        repo.save(&s).await.unwrap();
        let found = repo.find_by_id("abc").await.unwrap();
        assert_eq!(found.session_id, "abc");
    }

    #[tokio::test]
    async fn expired_session_returns_auth_error() {
        let repo = InMemorySessionRepository::new();
        let s = make_session("exp", -1); // already expired
        repo.save(&s).await.unwrap();
        let result = repo.find_by_id("exp").await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn delete_removes_session() {
        let repo = InMemorySessionRepository::new();
        let s = make_session("del", 3600);
        repo.save(&s).await.unwrap();
        repo.delete("del").await.unwrap();
        let result = repo.find_by_id("del").await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn delete_for_user_removes_all_user_sessions() {
        let repo  = InMemorySessionRepository::new();
        let uid   = UserId::new();
        let other = UserId::new();
        let s1 = Session { session_id: "s1".into(), user_id: uid, claims_json: "{}".into(), expires_at: Utc::now() + Duration::hours(1) };
        let s2 = Session { session_id: "s2".into(), user_id: uid, claims_json: "{}".into(), expires_at: Utc::now() + Duration::hours(1) };
        let s3 = Session { session_id: "s3".into(), user_id: other, claims_json: "{}".into(), expires_at: Utc::now() + Duration::hours(1) };
        repo.save(&s1).await.unwrap();
        repo.save(&s2).await.unwrap();
        repo.save(&s3).await.unwrap();
        repo.delete_for_user(uid).await.unwrap();
        assert!(matches!(repo.find_by_id("s1").await, Err(DomainError::Auth)));
        assert!(matches!(repo.find_by_id("s2").await, Err(DomainError::Auth)));
        assert!(repo.find_by_id("s3").await.is_ok());
    }

    #[tokio::test]
    async fn purge_expired_removes_only_expired() {
        let repo = InMemorySessionRepository::new();
        let live    = make_session("live", 3600);
        let expired = make_session("dead", -1);
        repo.save(&live).await.unwrap();
        repo.save(&expired).await.unwrap();
        repo.purge_expired().await.unwrap();
        assert!(repo.find_by_id("live").await.is_ok());
        assert!(matches!(repo.find_by_id("dead").await, Err(DomainError::Auth)));
    }
}
