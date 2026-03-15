//! `UserService` — business logic for moderator and admin account management.
//!
//! Responsibilities:
//! - Create new moderator/admin accounts (admin only — enforced by handler)
//! - Log in (verify password, issue token)
//! - Deactivate accounts (soft delete)
//!
//! Generic over `UserRepository` and `AuthProvider`.

pub mod errors;
pub use errors::UserError;

use domains::errors::DomainError;
use domains::models::{Claims, Page, Paginated, Role, Token, User, UserId};
use domains::ports::{AuthProvider, UserRepository};
use tracing::{info, instrument};
use uuid::Uuid;

use crate::common::utils::now_utc;

/// Minimum password length enforced by `UserService`.
const MIN_PASSWORD_LEN: usize = 12;

/// Service handling moderator and admin user account operations.
///
/// Generic over `UR: UserRepository` and `AP: AuthProvider`.
pub struct UserService<UR: UserRepository, AP: AuthProvider> {
    user_repo: UR,
    auth:      AP,
    jwt_ttl_secs: u64,
}

impl<UR: UserRepository, AP: AuthProvider> UserService<UR, AP> {
    /// Construct a `UserService`.
    ///
    /// `jwt_ttl_secs` controls how long issued tokens are valid.
    pub fn new(user_repo: UR, auth: AP, jwt_ttl_secs: u64) -> Self {
        Self { user_repo, auth, jwt_ttl_secs }
    }

    /// Create a new moderator or admin account.
    ///
    /// Validates that:
    /// - `username` is 3–32 characters, alphanumeric + underscore
    /// - `password` is at least 12 characters
    ///
    /// Returns `UserError::Validation` for invalid input.
    #[instrument(skip(self, password), fields(username = %username, role = %role))]
    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
    ) -> Result<User, UserError> {
        // Validate username
        if username.len() < 3 || username.len() > 32 {
            return Err(UserError::Validation {
                reason: format!(
                    "username length {} is outside allowed range 3..=32",
                    username.len()
                ),
            });
        }
        if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(UserError::Validation {
                reason: "username must contain only alphanumeric characters and underscores"
                    .to_owned(),
            });
        }

        // Validate password
        if password.len() < MIN_PASSWORD_LEN {
            return Err(UserError::Validation {
                reason: format!(
                    "password must be at least {} characters",
                    MIN_PASSWORD_LEN
                ),
            });
        }

        let password_hash = self
            .auth
            .hash_password(password)
            .await
            .map_err(UserError::Internal)?;

        let user = User {
            id:            UserId(Uuid::new_v4()),
            username:      username.to_owned(),
            password_hash,
            role,
            is_active:     true,
            created_at:    now_utc(),
        };

        self.user_repo.save(&user).await?;
        info!(user_id = %user.id, username = %user.username, "user created");
        Ok(user)
    }

    /// Log in with a username and password.
    ///
    /// Returns `UserError::InvalidCredentials` if the username does not exist or
    /// the password does not match (deliberate vagueness to prevent enumeration).
    /// Returns `UserError::Deactivated` if the account is inactive.
    ///
    /// On success returns `(Token, Claims)` so callers can inspect expiry without
    /// re-verifying the token.
    #[instrument(skip(self, password), fields(username = %username))]
    pub async fn login(&self, username: &str, password: &str) -> Result<(Token, Claims), UserError> {
        let user = self
            .user_repo
            .find_by_username(username)
            .await
            .map_err(|e| match e {
                DomainError::NotFound { .. } => UserError::InvalidCredentials,
                other => UserError::Internal(other),
            })?;

        if !user.is_active {
            return Err(UserError::Deactivated);
        }

        self.auth
            .verify_password(password, &user.password_hash)
            .await
            .map_err(|_| UserError::InvalidCredentials)?;

        let (token, claims) = self.mint_token_for(&user).await?;
        info!(user_id = %user.id, "user logged in");
        Ok((token, claims))
    }

    /// Issue a refreshed token for an already-authenticated user.
    ///
    /// The caller must have already verified the existing token via the auth
    /// middleware; this method simply mints a new token with a fresh expiry.
    #[instrument(skip(self), fields(user_id = %user_id))]
    pub async fn refresh(&self, user_id: UserId) -> Result<(Token, Claims), UserError> {
        let user = self
            .user_repo
            .find_by_id(user_id)
            .await
            .map_err(|e| match e {
                DomainError::NotFound { .. } => UserError::NotFound { id: user_id.to_string() },
                other => UserError::Internal(other),
            })?;

        if !user.is_active {
            return Err(UserError::Deactivated);
        }

        let (token, claims) = self.mint_token_for(&user).await?;
        info!(user_id = %user_id, "token refreshed");
        Ok((token, claims))
    }

    /// Shared helper: load owned boards + volunteer boards, build claims, sign a token.
    async fn mint_token_for(&self, user: &User) -> Result<(Token, Claims), UserError> {
        let owned_boards = self
            .user_repo
            .find_owned_boards(user.id)
            .await
            .unwrap_or_default();

        let volunteer_boards = self
            .user_repo
            .find_volunteer_boards(user.id)
            .await
            .unwrap_or_default();

        let exp = chrono::Utc::now().timestamp() + self.jwt_ttl_secs as i64;
        let claims = Claims {
            user_id:  user.id,
            username: user.username.clone(),
            role:     user.role,
            owned_boards,
            volunteer_boards,
            exp,
        };

        let token = self
            .auth
            .create_token(&claims)
            .await
            .map_err(UserError::Internal)?;

        Ok((token, claims))
    }

    /// Deactivate a user account (soft delete).
    ///
    /// Returns `UserError::NotFound` if the user does not exist.
    #[instrument(skip(self), fields(user_id = %user_id))]
    pub async fn deactivate(&self, user_id: UserId) -> Result<(), UserError> {
        self.user_repo.deactivate(user_id).await.map_err(|e| match e {
            DomainError::NotFound { .. } => UserError::NotFound {
                id: user_id.to_string(),
            },
            other => UserError::Internal(other),
        })?;
        info!(user_id = %user_id, "user deactivated");
        Ok(())
    }

    /// Fetch a single user by ID.
    ///
    /// Used by handlers that need the full `User` record (e.g. to display `created_at`).
    /// Returns `UserError::NotFound` if the user does not exist.
    pub async fn get_user(&self, user_id: UserId) -> Result<User, UserError> {
        self.user_repo.find_by_id(user_id).await.map_err(|e| match e {
            DomainError::NotFound { .. } => UserError::NotFound {
                id: user_id.to_string(),
            },
            other => UserError::Internal(other),
        })
    }

    /// Paginated list of all user accounts.
    pub async fn list_users(
        &self,
        page: Page,
    ) -> Result<Paginated<User>, UserError> {
        Ok(self.user_repo.find_all(page).await?)
    }

    /// Add a board owner relationship.
    pub async fn add_board_owner(&self, board_id: domains::models::BoardId, user_id: UserId) -> Result<(), UserError> {
        self.user_repo
            .add_board_owner(board_id, user_id)
            .await
            .map_err(UserError::Internal)
    }

    /// Remove a board owner relationship.
    pub async fn remove_board_owner(&self, board_id: domains::models::BoardId, user_id: UserId) -> Result<(), UserError> {
        self.user_repo
            .remove_board_owner(board_id, user_id)
            .await
            .map_err(UserError::Internal)
    }

    /// Public self-registration — creates a `Role::User` account.
    ///
    /// Identical validation rules to `create_user`. The caller is responsible
    /// for checking `Settings.open_registration` before calling this method;
    /// the service does not have access to infrastructure settings.
    ///
    /// Returns the newly created `User` on success.
    #[instrument(skip(self, password), fields(username = %username))]
    pub async fn register(&self, username: &str, password: &str) -> Result<User, UserError> {
        self.create_user(username, password, Role::User).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domains::models::{PasswordHash, Token};
    use domains::ports::{MockAuthProvider, MockUserRepository};

    fn make_service(
        user_repo: MockUserRepository,
        auth: MockAuthProvider,
    ) -> UserService<MockUserRepository, MockAuthProvider> {
        UserService::new(user_repo, auth, 86400)
    }

    #[tokio::test]
    async fn create_user_happy_path() {
        let mut auth = MockAuthProvider::new();
        auth.expect_hash_password()
            .times(1)
            .returning(|_| Ok(PasswordHash::new("$argon2id$v=19$...")));

        let mut repo = MockUserRepository::new();
        repo.expect_save().times(1).returning(|_| Ok(()));

        let svc = make_service(repo, auth);
        let result = svc.create_user("alice_mod", "correct-horse-battery", Role::Janitor).await;
        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.username, "alice_mod");
        assert_eq!(user.role, Role::Janitor);
        assert!(user.is_active);
    }

    #[tokio::test]
    async fn create_user_password_too_short() {
        let svc = make_service(MockUserRepository::new(), MockAuthProvider::new());
        let result = svc.create_user("bob", "short", Role::Janitor).await;
        assert!(matches!(result, Err(UserError::Validation { .. })));
    }

    #[tokio::test]
    async fn create_user_invalid_username() {
        let svc = make_service(MockUserRepository::new(), MockAuthProvider::new());
        let result = svc
            .create_user("bad user name!", "correct-horse-battery", Role::Janitor)
            .await;
        assert!(matches!(result, Err(UserError::Validation { .. })));
    }

    #[tokio::test]
    async fn login_invalid_credentials() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_username()
            .times(1)
            .returning(|_| Err(DomainError::not_found("user")));

        let svc = make_service(repo, MockAuthProvider::new());
        let result = svc.login("nobody", "password123456").await;
        assert!(matches!(result, Err(UserError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn login_deactivated_account() {
        let user = User {
            id:            UserId::new(),
            username:      "alice".to_owned(),
            password_hash: PasswordHash::new("$argon2id$..."),
            role:          Role::Janitor,
            is_active:     false,
            created_at:    now_utc(),
        };

        let mut repo = MockUserRepository::new();
        repo.expect_find_by_username()
            .times(1)
            .returning(move |_| Ok(user.clone()));

        let svc = make_service(repo, MockAuthProvider::new());
        let result = svc.login("alice", "correct-horse-battery").await;
        assert!(matches!(result, Err(UserError::Deactivated)));
    }

    #[tokio::test]
    async fn login_success() {
        let user = User {
            id:            UserId::new(),
            username:      "alice".to_owned(),
            password_hash: PasswordHash::new("$argon2id$..."),
            role:          Role::Janitor,
            is_active:     true,
            created_at:    now_utc(),
        };
        let user_id = user.id;

        let mut repo = MockUserRepository::new();
        repo.expect_find_by_username()
            .times(1)
            .returning(move |_| Ok(user.clone()));
        repo.expect_find_owned_boards()
            .times(1)
            .returning(|_| Ok(vec![]));
        repo.expect_find_volunteer_boards()
            .times(1)
            .returning(|_| Ok(vec![]));

        let mut auth = MockAuthProvider::new();
        auth.expect_verify_password()
            .times(1)
            .returning(|_, _| Ok(()));
        auth.expect_create_token()
            .times(1)
            .returning(|_| Ok(Token::new("eyJhbGci...")));

        let svc = make_service(repo, auth);
        let result = svc.login("alice", "correct-horse-battery").await;
        assert!(result.is_ok());
        let (token, claims) = result.unwrap();
        assert!(!token.0.is_empty());
        assert_eq!(claims.user_id, user_id);
    }

    #[tokio::test]
    async fn register_creates_user_role() {
        let mut repo = MockUserRepository::new();
        repo.expect_find_by_username()
            .times(0);
        repo.expect_save()
            .times(1)
            .returning(|u| {
                assert_eq!(u.role, Role::User, "register() must create Role::User accounts");
                Ok(())
            });

        let mut auth = MockAuthProvider::new();
        auth.expect_hash_password()
            .times(1)
            .returning(|_| Ok(PasswordHash::new("$argon2id$v=19$...")));

        let svc = make_service(repo, auth);
        let result = svc.register("newuser", "strongpassword1234").await;
        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.role, Role::User);
        assert_eq!(user.username, "newuser");
    }
}
