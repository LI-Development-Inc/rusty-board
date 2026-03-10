//! Cookie session authentication adapter (`auth-cookie` feature).
//!
//! Implements `AuthProvider` using server-side sessions stored via `SessionRepository`.
//!
//! # How it works
//!
//! 1. `create_token` — generates a cryptographically random session ID, serialises the
//!    `Claims` to JSON, persists a `Session` row, and returns the session ID as the
//!    `Token` value. The caller is responsible for setting the `Set-Cookie` header.
//!
//! 2. `verify_token` — looks up the session by ID, checks expiry, deserialises claims,
//!    and returns them. Returns `DomainError::Auth` on any failure.
//!
//! 3. `revoke_token` — deletes the session row, immediately invalidating the session.
//!    This is the key advantage over JWT: logout is instant and server-enforced.
//!
//! 4. `hash_password` / `verify_password` — delegates to the shared argon2id helpers
//!    in `common/hashing.rs`, identical to `JwtAuthProvider`.
//!
//! # CSRF protection
//!
//! Cookie sessions are vulnerable to Cross-Site Request Forgery. This adapter uses the
//! **double-submit cookie pattern**:
//!
//! - `generate_csrf_token` — returns a random token. The caller sets it as both a
//!   `Set-Cookie` (HttpOnly=false so JS can read it) and includes it in the HTML form
//!   as a hidden field or response header.
//! - `verify_csrf_token` — compares the cookie value against the submitted value using
//!   constant-time comparison. Returns `DomainError::Auth` on mismatch.
//!
//! The CSRF methods are on `CookieAuthProvider` directly (not on the `AuthProvider` trait)
//! because CSRF is a transport-layer concern handled in the Axum middleware, not in
//! service-layer code.
//!
//! # Session TTL
//!
//! `ttl_secs` is set at construction time and applied to every new session.
//! Typical value: 86400 (24 hours). Sessions are fixed-lifetime (not sliding window).

use async_trait::async_trait;
use chrono::Utc;
use domains::errors::DomainError;
use domains::models::{Claims, PasswordHash, Token};
use domains::ports::{AuthProvider, Session, SessionRepository};
use tracing::instrument;
use uuid::Uuid;

/// Cookie session `AuthProvider`.
///
/// Generic over `SR: SessionRepository` so the composition root can select
/// `PgSessionRepository` for production or `InMemorySessionRepository` for
/// development and tests without changing this code.
#[derive(Clone)]
pub struct CookieAuthProvider<SR: SessionRepository> {
    /// Backing store for session rows.
    session_repo: SR,
    /// Lifetime in seconds applied to every new session.
    ttl_secs:     i64,
    /// Argon2id memory cost.
    m_cost:       u32,
    /// Argon2id time cost.
    t_cost:       u32,
    /// Argon2id parallelism cost.
    p_cost:       u32,
}

impl<SR: SessionRepository> CookieAuthProvider<SR> {
    /// Create a new `CookieAuthProvider`.
    ///
    /// - `session_repo` — backing store for sessions
    /// - `ttl_secs` — lifetime of each new session in seconds (e.g. 86400 for 24 h)
    /// - `m_cost`, `t_cost`, `p_cost` — argon2id parameters for password hashing
    pub fn new(session_repo: SR, ttl_secs: i64, m_cost: u32, t_cost: u32, p_cost: u32) -> Self {
        Self { session_repo, ttl_secs, m_cost, t_cost, p_cost }
    }

    /// Generate a random CSRF token (URL-safe base64, 256 bits of entropy).
    ///
    /// The caller must:
    /// 1. Set this as a `Set-Cookie` with `SameSite=Strict; Path=/` (readable by JS).
    /// 2. Include it in every state-changing HTML form as a hidden field named `_csrf`.
    /// 3. On form submission, call `verify_csrf_token` before processing the form.
    pub fn generate_csrf_token() -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut bytes = [0u8; 32];
        bytes[..16].copy_from_slice(a.as_bytes());
        bytes[16..].copy_from_slice(b.as_bytes());
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Verify that the submitted CSRF token matches the cookie token.
    ///
    /// Uses constant-time comparison (manual XOR fold) to prevent timing attacks.
    /// Returns `DomainError::Auth` on mismatch, empty inputs, or length mismatch.
    pub fn verify_csrf_token(cookie_token: &str, submitted_token: &str) -> Result<(), DomainError> {
        if cookie_token.is_empty() || submitted_token.is_empty() {
            return Err(DomainError::Auth);
        }
        if cookie_token.len() != submitted_token.len() {
            return Err(DomainError::Auth);
        }
        let mismatch: u8 = cookie_token
            .as_bytes()
            .iter()
            .zip(submitted_token.as_bytes().iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b));
        if mismatch == 0 { Ok(()) } else { Err(DomainError::Auth) }
    }
}

#[async_trait]
impl<SR: SessionRepository> AuthProvider for CookieAuthProvider<SR> {
    /// Create a session, persist it, and return the session ID as the `Token`.
    ///
    /// The returned `Token` value is the opaque session ID to be set as a cookie.
    #[instrument(skip(self, claims))]
    async fn create_token(&self, claims: &Claims) -> Result<Token, DomainError> {
        let session_id  = Uuid::new_v4().to_string();
        let claims_json = serde_json::to_string(claims)
            .map_err(|e| DomainError::internal(format!("claims serialisation error: {e}")))?;
        let session = Session {
            session_id:  session_id.clone(),
            user_id:     claims.user_id,
            claims_json,
            expires_at:  Utc::now() + chrono::Duration::seconds(self.ttl_secs),
        };
        self.session_repo.save(&session).await?;
        Ok(Token::new(session_id))
    }

    /// Look up the session and deserialise its claims.
    ///
    /// Returns `DomainError::Auth` if the session does not exist, has expired,
    /// or if the stored claims JSON is malformed.
    #[instrument(skip(self, token))]
    async fn verify_token(&self, token: &Token) -> Result<Claims, DomainError> {
        let session = self.session_repo.find_by_id(&token.0).await?;
        serde_json::from_str::<Claims>(&session.claims_json)
            .map_err(|_| DomainError::Auth)
    }

    /// Delete the session row, immediately invalidating the session.
    #[instrument(skip(self, token))]
    async fn revoke_token(&self, token: &Token) -> Result<(), DomainError> {
        self.session_repo.delete(&token.0).await
    }

    async fn hash_password(&self, password: &str) -> Result<PasswordHash, DomainError> {
        crate::common::hashing::hash_password(password, self.m_cost, self.t_cost, self.p_cost)
            .await
    }

    /// Verify a plaintext password against a stored argon2id hash.
    ///
    /// Delegates to `common::hashing::verify_password`. Returns `DomainError::Auth`
    /// if the password does not match.
    async fn verify_password(
        &self,
        password: &str,
        hash: &PasswordHash,
    ) -> Result<(), DomainError> {
        crate::common::hashing::verify_password(password, hash).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domains::models::{Role, UserId};
    use storage_adapters::in_memory::InMemorySessionRepository;

    fn provider() -> CookieAuthProvider<InMemorySessionRepository> {
        CookieAuthProvider::new(InMemorySessionRepository::new(), 3600, 4096, 1, 1)
    }

    fn test_claims() -> Claims {
        Claims {
            user_id:          UserId::new(),
            username:         "testuser".into(),
            role:             Role::Janitor,
            owned_boards:     vec![],
            volunteer_boards: vec![],
            exp:              Utc::now().timestamp() + 3600,
        }
    }

    #[tokio::test]
    async fn token_roundtrip() {
        let p = provider();
        let c = test_claims();
        let token   = p.create_token(&c).await.unwrap();
        let decoded = p.verify_token(&token).await.unwrap();
        assert_eq!(decoded.user_id, c.user_id);
        assert_eq!(decoded.role, Role::Janitor);
    }

    #[tokio::test]
    async fn verify_unknown_token_returns_auth_error() {
        let p = provider();
        let result = p.verify_token(&Token::new("no-such-session")).await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn verify_after_expiry_returns_auth_error() {
        // TTL=-1 means expires_at is in the past
        let p = CookieAuthProvider::new(InMemorySessionRepository::new(), -1, 4096, 1, 1);
        let token = p.create_token(&test_claims()).await.unwrap();
        let result = p.verify_token(&token).await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn revoke_invalidates_session() {
        let p = provider();
        let token = p.create_token(&test_claims()).await.unwrap();
        p.revoke_token(&token).await.unwrap();
        let result = p.verify_token(&token).await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn revoke_nonexistent_session_is_ok() {
        let p = provider();
        assert!(p.revoke_token(&Token::new("ghost")).await.is_ok());
    }

    #[test]
    fn csrf_matching_tokens_ok() {
        let t = CookieAuthProvider::<InMemorySessionRepository>::generate_csrf_token();
        assert!(CookieAuthProvider::<InMemorySessionRepository>::verify_csrf_token(&t, &t).is_ok());
    }

    #[test]
    fn csrf_mismatched_tokens_error() {
        let t1 = CookieAuthProvider::<InMemorySessionRepository>::generate_csrf_token();
        let t2 = CookieAuthProvider::<InMemorySessionRepository>::generate_csrf_token();
        let result = CookieAuthProvider::<InMemorySessionRepository>::verify_csrf_token(&t1, &t2);
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[test]
    fn csrf_empty_input_errors() {
        let t = CookieAuthProvider::<InMemorySessionRepository>::generate_csrf_token();
        assert!(CookieAuthProvider::<InMemorySessionRepository>::verify_csrf_token("", &t).is_err());
        assert!(CookieAuthProvider::<InMemorySessionRepository>::verify_csrf_token(&t, "").is_err());
    }

    #[test]
    fn csrf_tokens_are_unique() {
        let t1 = CookieAuthProvider::<InMemorySessionRepository>::generate_csrf_token();
        let t2 = CookieAuthProvider::<InMemorySessionRepository>::generate_csrf_token();
        assert_ne!(t1, t2);
    }

    #[tokio::test]
    async fn password_hash_and_verify() {
        let p = provider();
        let hash = p.hash_password("hunter2").await.unwrap();
        assert!(p.verify_password("hunter2", &hash).await.is_ok());
        assert!(matches!(p.verify_password("wrong", &hash).await, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn multiple_sessions_for_same_user_all_valid() {
        let p  = provider();
        let c  = test_claims();
        let t1 = p.create_token(&c).await.unwrap();
        let t2 = p.create_token(&c).await.unwrap();
        assert_ne!(t1.0, t2.0, "each session must have a unique ID");
        assert!(p.verify_token(&t1).await.is_ok());
        assert!(p.verify_token(&t2).await.is_ok());
    }
}
