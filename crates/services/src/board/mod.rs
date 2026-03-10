//! `BoardService` — business logic for board management.
//!
//! Responsibilities:
//! - Create boards (with slug validation and automatic `BoardConfig` creation)
//! - Update board metadata (title, rules)
//! - Delete boards (delegates cascade to repository)
//! - Retrieve and update `BoardConfig` (with cache invalidation notification)
//! - List boards (paginated)
//!
//! This service is generic over `BoardRepository` and depends on no other port.
//! All tests use `MockBoardRepository` from `mockall`.

pub mod errors;
pub use errors::BoardError;

use async_trait::async_trait;
use chrono::Utc;
use domains::models::{Board, BoardConfig, BoardId, Page, Paginated};
use domains::ports::{BoardRepository, BoardVolunteerRepository};
use tracing::{info, instrument};
use uuid::Uuid;

use crate::common::utils::slug_validate;

/// Service-level trait abstracting board operations.
///
/// Handlers depend on this trait rather than the concrete `BoardService<BR>`.
/// This allows the composition root to pass `Arc<BoardService<BR>>` as state
/// without handlers needing to know the generic parameter.
#[async_trait]
pub trait BoardRepo: Send + Sync + 'static {
    /// Create a new board with the given slug, title, and rules.
    ///
    /// Returns `BoardError::InvalidSlug` if the slug is malformed,
    /// `BoardError::SlugConflict` if a board with that slug already exists.
    async fn create_board(&self, slug: &str, title: &str, rules: &str) -> Result<Board, BoardError>;

    /// Fetch a board by its URL slug.
    ///
    /// Returns `BoardError::NotFound` if no board with that slug exists.
    async fn get_by_slug(&self, slug: &str) -> Result<Board, BoardError>;

    /// Fetch a board by its UUID.
    ///
    /// Returns `BoardError::NotFound` if no board with that ID exists.
    async fn get_by_id(&self, id: BoardId) -> Result<Board, BoardError>;

    /// Update a board's title and/or rules. `None` fields are left unchanged.
    ///
    /// Returns `BoardError::NotFound` if the board does not exist.
    async fn update_board(&self, id: BoardId, title: Option<&str>, rules: Option<&str>) -> Result<Board, BoardError>;

    /// Permanently delete a board and all its threads, posts, and config.
    ///
    /// Returns `BoardError::NotFound` if the board does not exist.
    async fn delete_board(&self, id: BoardId) -> Result<(), BoardError>;

    /// Return a paginated list of all boards, ordered by creation date ascending.
    async fn list_boards(&self, page: Page) -> Result<Paginated<Board>, BoardError>;

    /// Return the `BoardConfig` for the given board.
    ///
    /// Returns `BoardError::NotFound` if the board or its config row does not exist.
    async fn get_config(&self, board_id: BoardId) -> Result<BoardConfig, BoardError>;

    /// Persist an updated `BoardConfig`, replacing all fields atomically.
    ///
    /// Returns the saved config. Returns `BoardError::NotFound` if the board does not exist.
    async fn update_config(&self, board_id: BoardId, config: BoardConfig) -> Result<BoardConfig, BoardError>;


    /// List volunteers for a board. Returns `(UserId, username, assigned_at)`.
    async fn list_volunteers(&self, board_id: BoardId)
        -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError>;

    /// Add a user as volunteer by username.
    async fn add_volunteer_by_username(
        &self,
        board_id:    BoardId,
        username:    &str,
        assigned_by: domains::models::UserId,
    ) -> Result<(), BoardError>;

    /// Remove a volunteer from a board.
    async fn remove_volunteer(&self, board_id: BoardId, user_id: domains::models::UserId)
        -> Result<(), BoardError>;
}

/// Service handling all board-level operations.
///
/// Generic over `BR: BoardRepository` so tests can inject a mock repository
/// without any concrete storage dependency.
pub struct BoardService<BR: BoardRepository> {
    repo: BR,
}

impl<BR: BoardRepository> BoardService<BR> {
    /// Construct a new `BoardService` with the given repository.
    pub fn new(repo: BR) -> Self {
        Self { repo }
    }

    /// Create a new board with the given slug and title.
    ///
    /// - Validates the slug format (`^[a-z0-9_-]{1,16}$`)
    /// - Persists the board via the repository
    /// - The repository is responsible for inserting a default `BoardConfig` row
    ///
    /// Returns `BoardError::InvalidSlug` for an invalid slug.
    /// Returns `BoardError::Internal` if persistence fails.
    #[instrument(skip(self), fields(slug = %slug, title = %title))]
    pub async fn create_board(
        &self,
        slug: &str,
        title: &str,
        rules: &str,
    ) -> Result<Board, BoardError> {
        let slug = slug_validate(slug).map_err(|_| BoardError::InvalidSlug {
            slug: slug.to_owned(),
        })?;

        let board = Board {
            id: BoardId(Uuid::new_v4()),
            slug,
            title: title.to_owned(),
            rules: rules.to_owned(),
            created_at: Utc::now(),
        };

        self.repo.save(&board).await?;
        info!(board_id = %board.id, "board created");
        Ok(board)
    }

    /// Retrieve a board by its URL slug.
    ///
    /// Returns `BoardError::NotFound` if no board with the given slug exists.
    #[instrument(skip(self), fields(slug = %slug))]
    pub async fn get_by_slug(&self, slug: &str) -> Result<Board, BoardError> {
        let slug = slug_validate(slug).map_err(|_| BoardError::InvalidSlug {
            slug: slug.to_owned(),
        })?;
        self.repo
            .find_by_slug(&slug)
            .await
            .map_err(|e| match e {
                domains::errors::DomainError::NotFound { .. } => BoardError::NotFound {
                    slug: slug.to_string(),
                },
                other => BoardError::Internal(other),
            })
    }

    /// Retrieve a board by its UUID.
    ///
    /// Returns `BoardError::NotFound` if no board with the given id exists.
    #[instrument(skip(self), fields(board_id = %id))]
    pub async fn get_by_id(&self, id: BoardId) -> Result<Board, BoardError> {
        self.repo.find_by_id(id).await.map_err(|e| match e {
            domains::errors::DomainError::NotFound { .. } => BoardError::NotFound {
                slug: id.to_string(),
            },
            other => BoardError::Internal(other),
        })
    }

    /// Update the title and/or rules of an existing board.
    ///
    /// Returns `BoardError::NotFound` if the board does not exist.
    #[instrument(skip(self), fields(board_id = %id))]
    pub async fn update_board(
        &self,
        id: BoardId,
        title: Option<&str>,
        rules: Option<&str>,
    ) -> Result<Board, BoardError> {
        let mut board = self.get_by_id(id).await?;
        if let Some(t) = title {
            board.title = t.to_owned();
        }
        if let Some(r) = rules {
            board.rules = r.to_owned();
        }
        self.repo.save(&board).await?;
        info!(board_id = %id, "board updated");
        Ok(board)
    }

    /// Delete a board and all its content.
    ///
    /// Returns `BoardError::NotFound` if the board does not exist.
    #[instrument(skip(self), fields(board_id = %id))]
    pub async fn delete_board(&self, id: BoardId) -> Result<(), BoardError> {
        self.repo.delete(id).await.map_err(|e| match e {
            domains::errors::DomainError::NotFound { .. } => BoardError::NotFound {
                slug: id.to_string(),
            },
            other => BoardError::Internal(other),
        })?;
        info!(board_id = %id, "board deleted");
        Ok(())
    }

    /// Paginated list of all boards.
    #[instrument(skip(self), fields(page = page.0))]
    pub async fn list_boards(&self, page: Page) -> Result<Paginated<Board>, BoardError> {
        Ok(self.repo.find_all(page).await?)
    }

    /// Retrieve the `BoardConfig` for a board.
    ///
    /// Returns `BoardError::NotFound` if the board does not exist.
    ///
    /// # Note on caching
    /// The cache layer (`BoardConfigCache`) is applied in the API middleware layer,
    /// not here in the service. The service always reads from the repository.
    #[instrument(skip(self), fields(board_id = %board_id))]
    pub async fn get_config(&self, board_id: BoardId) -> Result<BoardConfig, BoardError> {
        Ok(self.repo.find_config(board_id).await?)
    }

    /// Update the `BoardConfig` for a board.
    ///
    /// The caller is responsible for merging partial updates with the existing config
    /// before passing the complete updated config here.
    ///
    /// # Side effects
    /// The API layer must invalidate the in-process `BoardConfigCache` entry for this
    /// board after calling this method.
    ///
    /// Returns `BoardError::NotFound` if the board does not exist.
    #[instrument(skip(self, config), fields(board_id = %board_id))]
    pub async fn update_config(
        &self,
        board_id: BoardId,
        config: BoardConfig,
    ) -> Result<BoardConfig, BoardError> {
        self.repo.save_config(board_id, &config).await?;
        info!(board_id = %board_id, "board config updated");
        Ok(config)
    }

}

impl<BR: BoardRepository + BoardVolunteerRepository> BoardService<BR> {
    /// List board volunteers.
    pub async fn list_volunteers(
        &self,
        board_id: BoardId,
    ) -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> {
        self.repo.list_volunteers(board_id).await
            .map_err(|e: domains::errors::DomainError| BoardError::Internal(e))
    }

    /// Add a volunteer by username.
    pub async fn add_volunteer_by_username(
        &self,
        board_id:    BoardId,
        username:    &str,
        assigned_by: domains::models::UserId,
    ) -> Result<(), BoardError> {
        self.repo.add_volunteer_by_username(board_id, username, assigned_by).await
            .map_err(|e: domains::errors::DomainError| BoardError::Internal(e))
    }

    /// Remove a volunteer.
    pub async fn remove_volunteer(
        &self,
        board_id: BoardId,
        user_id:  domains::models::UserId,
    ) -> Result<(), BoardError> {
        self.repo.remove_volunteer(board_id, user_id).await
            .map_err(|e: domains::errors::DomainError| BoardError::Internal(e))
    }
}

/// Blanket implementation of `BoardRepo` for `BoardService<BR>`.
///
/// Handlers depend on the trait; the composition root injects the concrete service.
#[async_trait]
impl<BR: BoardRepository + BoardVolunteerRepository> BoardRepo for BoardService<BR> {
    async fn create_board(&self, slug: &str, title: &str, rules: &str) -> Result<Board, BoardError> {
        self.create_board(slug, title, rules).await
    }
    async fn get_by_slug(&self, slug: &str) -> Result<Board, BoardError> {
        self.get_by_slug(slug).await
    }
    async fn get_by_id(&self, id: BoardId) -> Result<Board, BoardError> {
        self.get_by_id(id).await
    }
    async fn update_board(&self, id: BoardId, title: Option<&str>, rules: Option<&str>) -> Result<Board, BoardError> {
        self.update_board(id, title, rules).await
    }
    async fn delete_board(&self, id: BoardId) -> Result<(), BoardError> {
        self.delete_board(id).await
    }
    async fn list_boards(&self, page: Page) -> Result<Paginated<Board>, BoardError> {
        self.list_boards(page).await
    }
    async fn get_config(&self, board_id: BoardId) -> Result<BoardConfig, BoardError> {
        self.get_config(board_id).await
    }
    async fn update_config(&self, board_id: BoardId, config: BoardConfig) -> Result<BoardConfig, BoardError> {
        self.update_config(board_id, config).await
    }
    async fn list_volunteers(&self, board_id: BoardId)
        -> Result<Vec<(domains::models::UserId, String, chrono::DateTime<chrono::Utc>)>, BoardError> {
        self.list_volunteers(board_id).await
    }
    async fn add_volunteer_by_username(
        &self, board_id: BoardId, username: &str, assigned_by: domains::models::UserId,
    ) -> Result<(), BoardError> {
        self.add_volunteer_by_username(board_id, username, assigned_by).await
    }
    async fn remove_volunteer(&self, board_id: BoardId, user_id: domains::models::UserId)
        -> Result<(), BoardError> {
        self.remove_volunteer(board_id, user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domains::models::Slug;
    use domains::ports::MockBoardRepository;

    #[allow(dead_code)]
    fn sample_board(slug: &str) -> Board {
        Board {
            id: BoardId(Uuid::new_v4()),
            slug: Slug::new(slug).unwrap(),
            title: "Test Board".to_owned(),
            rules: "".to_owned(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn create_board_happy_path() {
        let mut mock = MockBoardRepository::new();
        mock.expect_save()
            .times(1)
            .returning(|_| Ok(()));

        let svc = BoardService::new(mock);
        let result = svc.create_board("tech", "Technology", "Be nice").await;
        assert!(result.is_ok());
        let board = result.unwrap();
        assert_eq!(board.slug.as_str(), "tech");
        assert_eq!(board.title, "Technology");
    }

    #[tokio::test]
    async fn create_board_invalid_slug() {
        let mock = MockBoardRepository::new();
        let svc = BoardService::new(mock);
        let result = svc.create_board("INVALID SLUG!", "Title", "").await;
        assert!(matches!(result, Err(BoardError::InvalidSlug { .. })));
    }

    #[tokio::test]
    async fn get_by_slug_not_found() {
        let mut mock = MockBoardRepository::new();
        mock.expect_find_by_slug()
            .times(1)
            .returning(|_| Err(domains::errors::DomainError::not_found("board")));

        let svc = BoardService::new(mock);
        let result = svc.get_by_slug("tech").await;
        assert!(matches!(result, Err(BoardError::NotFound { .. })));
    }

    #[tokio::test]
    async fn list_boards_returns_paginated() {
        let mut mock = MockBoardRepository::new();
        mock.expect_find_all()
            .times(1)
            .returning(|_| Ok(Paginated::new(vec![], 0, Page::new(1), 15)));

        let svc = BoardService::new(mock);
        let result = svc.list_boards(Page::new(1)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().total, 0);
    }

    #[tokio::test]
    async fn delete_board_propagates_not_found() {
        let mut mock = MockBoardRepository::new();
        mock.expect_delete()
            .times(1)
            .returning(|_| Err(domains::errors::DomainError::not_found("board")));

        let svc = BoardService::new(mock);
        let result = svc.delete_board(BoardId::new()).await;
        assert!(matches!(result, Err(BoardError::NotFound { .. })));
    }
}
