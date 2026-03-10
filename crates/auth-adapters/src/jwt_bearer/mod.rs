//! JWT bearer token authentication adapter (`auth-jwt` feature).
//!
//! Implements `AuthProvider` using `jsonwebtoken` for HS256 token signing/verification
//! and the shared argon2id hashing functions in `common/hashing.rs`.

pub mod errors;

use async_trait::async_trait;
use domains::errors::DomainError;
use domains::models::{Claims, PasswordHash, Role, Token};
use uuid::Uuid;
use domains::ports::AuthProvider;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use tracing::instrument;

/// JWT authentication provider.
///
/// Signs tokens with HS256. Token secret is loaded from `Settings.jwt_secret`.
#[derive(Clone)]
pub struct JwtAuthProvider {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

impl JwtAuthProvider {
    /// Create a new `JwtAuthProvider`.
    ///
    /// `secret` must be the raw bytes of the JWT secret (at least 32 bytes recommended).
    pub fn new(secret: &[u8], m_cost: u32, t_cost: u32, p_cost: u32) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            m_cost,
            t_cost,
            p_cost,
        }
    }
}

/// JWT claims structure for serialisation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct JwtClaims {
    sub:              String,       // user_id
    #[serde(default)]
    username:         String,       // display name; default="" for backward compat with old tokens
    role:             String,
    owned_boards:     Vec<String>,  // board_id UUIDs
    volunteer_boards: Vec<String>,  // board_id UUIDs
    exp:              i64,
}

#[async_trait]
impl AuthProvider for JwtAuthProvider {
    #[instrument(skip(self, claims))]
    async fn create_token(&self, claims: &Claims) -> Result<Token, DomainError> {
        let jwt_claims = JwtClaims {
            sub:              claims.user_id.to_string(),
            username:         claims.username.clone(),
            role:             claims.role.to_string(),
            owned_boards:     claims.owned_boards.iter().map(|b| b.to_string()).collect(),
            volunteer_boards: claims.volunteer_boards.iter().map(|b| b.to_string()).collect(),
            exp:              claims.exp,
        };
        let token = jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &jwt_claims,
            &self.encoding_key,
        )
        .map_err(|e| DomainError::internal(format!("JWT encoding error: {e}")))?;
        Ok(Token::new(token))
    }

    #[instrument(skip(self, token))]
    async fn verify_token(&self, token: &Token) -> Result<Claims, DomainError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.leeway = 0; // no clock-skew tolerance; callers refresh before expiry

        let data = jsonwebtoken::decode::<JwtClaims>(
            &token.0,
            &self.decoding_key,
            &validation,
        )
        .map_err(|_| DomainError::Auth)?;

        let c = data.claims;
        use std::str::FromStr;
        let user_id = Uuid::from_str(&c.sub)
            .map(domains::models::UserId)
            .map_err(|_| DomainError::Auth)?;
        let role = Role::from_str(&c.role).map_err(|_| DomainError::Auth)?;
        let owned_boards = c
            .owned_boards
            .iter()
            .map(|s| {
                Uuid::from_str(s)
                    .map(domains::models::BoardId)
                    .map_err(|_| DomainError::Auth)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let volunteer_boards = c
            .volunteer_boards
            .iter()
            .map(|s| {
                Uuid::from_str(s)
                    .map(domains::models::BoardId)
                    .map_err(|_| DomainError::Auth)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Claims { user_id, username: c.username, role, owned_boards, volunteer_boards, exp: c.exp })
    }

    async fn hash_password(&self, password: &str) -> Result<PasswordHash, DomainError> {
        crate::common::hashing::hash_password(password, self.m_cost, self.t_cost, self.p_cost)
            .await
    }

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

    fn provider() -> JwtAuthProvider {
        JwtAuthProvider::new(b"test_secret_at_least_32_bytes_long!!", 4096, 1, 1)
    }

    #[tokio::test]
    async fn token_roundtrip() {
        let p = provider();
        let claims = Claims {
            user_id:          UserId::new(),
            username:         "testuser".into(),
            role:             Role::Janitor,
            owned_boards:     vec![],
            volunteer_boards: vec![],
            exp:              chrono::Utc::now().timestamp() + 3600,
        };
        let token = p.create_token(&claims).await.unwrap();
        let decoded = p.verify_token(&token).await.unwrap();
        assert_eq!(decoded.user_id, claims.user_id);
        assert_eq!(decoded.role, Role::Janitor);
    }

    #[tokio::test]
    async fn expired_token_returns_auth_error() {
        let p = provider();
        let claims = Claims {
            user_id:          UserId::new(),
            username:         "testuser".into(),
            role:             Role::Admin,
            owned_boards:     vec![],
            volunteer_boards: vec![],
            exp:              chrono::Utc::now().timestamp() - 120, // clearly expired
        };
        let token = p.create_token(&claims).await.unwrap();
        let result = p.verify_token(&token).await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }

    #[tokio::test]
    async fn wrong_secret_returns_auth_error() {
        let p1 = provider();
        let p2 = JwtAuthProvider::new(b"completely_different_secret_12345678!!", 4096, 1, 1);
        let claims = Claims {
            user_id:          UserId::new(),
            username:         "testuser".into(),
            role:             Role::Janitor,
            owned_boards:     vec![],
            volunteer_boards: vec![],
            exp:              chrono::Utc::now().timestamp() + 3600,
        };
        let token = p1.create_token(&claims).await.unwrap();
        let result = p2.verify_token(&token).await;
        assert!(matches!(result, Err(DomainError::Auth)));
    }
}
