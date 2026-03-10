//! Argon2id password hashing shared across all auth adapters.
//!
//! This module is **not feature-gated** — argon2id hashing is used regardless
//! of which authentication mechanism is active (JWT, cookie, OIDC).
//!
//! Parameters match OWASP recommended minimums for 2024:
//! - m = 19456 (19 MB)
//! - t = 2 (iterations)
//! - p = 1 (parallelism)

use argon2::{
    password_hash::{PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Params,
};
use domains::errors::DomainError;
use domains::models::PasswordHash;

/// Hash a plaintext password using argon2id with OWASP-recommended parameters.
///
/// Returns a PHC-format string suitable for storage.
/// Returns `DomainError::Internal` on failure (should not occur in normal operation).
pub async fn hash_password(
    password: &str,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<PasswordHash, DomainError> {
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || {
        let params = Params::new(m_cost, t_cost, p_cost, None)
            .map_err(|e| DomainError::internal(format!("argon2 params error: {e}")))?;
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| DomainError::internal(format!("argon2 hash error: {e}")))?;
        Ok(PasswordHash::new(hash.to_string()))
    })
    .await
    .map_err(|e| DomainError::internal(format!("blocking task error: {e}")))?
}

/// Verify a plaintext password against a stored argon2id PHC hash.
///
/// Returns `DomainError::Auth` if the password does not match.
pub async fn verify_password(password: &str, hash: &PasswordHash) -> Result<(), DomainError> {
    let password = password.to_owned();
    let hash_str = hash.0.clone();
    tokio::task::spawn_blocking(move || {
        let parsed_hash = argon2::password_hash::PasswordHash::new(&hash_str)
            .map_err(|e| DomainError::internal(format!("password hash parse error: {e}")))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .map_err(|_| DomainError::Auth)
    })
    .await
    .map_err(|e| DomainError::internal(format!("blocking task error: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_and_verify_roundtrip() {
        let hash = hash_password("correct-horse-battery", 4096, 1, 1).await.unwrap();
        assert!(verify_password("correct-horse-battery", &hash).await.is_ok());
    }

    #[tokio::test]
    async fn wrong_password_returns_auth_error() {
        let hash = hash_password("correct-horse-battery", 4096, 1, 1).await.unwrap();
        let result = verify_password("wrong-password", &hash).await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }
}
