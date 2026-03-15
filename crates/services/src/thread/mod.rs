//! `ThreadService` — business logic for thread lifecycle.
//!
//! Responsibilities:
//! - Create threads (allocates a new `Thread` row; OP post is handled by `PostService`)
//! - Toggle sticky and closed flags
//! - Prune: check whether the board is over capacity; delete oldest non-sticky threads
//!
//! Generic over `ThreadRepository`. Does not interact with any other port.

pub mod errors;
pub use errors::ThreadError;

use async_trait::async_trait;
use chrono::Utc;
use domains::models::{BoardId, Page, Paginated, Post, Thread, ThreadId, ThreadSummary};
use domains::ports::{PostRepository, ThreadRepository};
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Service-level trait abstracting thread operations for handlers.
#[async_trait]
pub trait ThreadRepo: Send + Sync + 'static {
    /// Allocate a new thread row for the given board.
    ///
    /// The OP post is created separately by `PostService` after this returns.
    /// Returns `ThreadError::Internal` if the insert fails.
    async fn create_thread(&self, board_id: BoardId) -> Result<Thread, ThreadError>;

    /// Return a paginated list of threads for the board, ordered by `bumped_at` descending.
    ///
    /// Returns `ThreadError::Internal` on storage failure.
    async fn list_threads(&self, board_id: BoardId, page: Page) -> Result<Paginated<Thread>, ThreadError>;

    /// Return all thread summaries for the board (OP body, thumbnail, reply count).
    ///
    /// Used by catalog views and the board index. Results are ordered by `bumped_at` descending.
    async fn get_catalog(&self, board_id: BoardId) -> Result<Vec<ThreadSummary>, ThreadError>;

    /// Fetch a single thread by ID.
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    async fn get_thread(&self, id: ThreadId) -> Result<Thread, ThreadError>;

    /// List posts in a thread, paginated (oldest first).
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    async fn list_posts(&self, thread_id: ThreadId, page: Page) -> Result<Paginated<Post>, ThreadError>;

    /// All posts in a thread, ordered by `post_number ASC`, up to the bump limit (500).
    ///
    /// Used by the thread HTML view, which shows all posts without pagination.
    async fn list_all_posts(&self, thread_id: ThreadId) -> Result<Vec<Post>, ThreadError>;

    /// Resolve a board-scoped post number to the `ThreadId` that contains it.
    ///
    /// Used by `GET /board/{slug}/post/{N}` redirect handler for cross-board links.
    /// Returns `None` when no post with that number exists on the board.
    async fn find_thread_id_by_post_number(
        &self,
        board_id: BoardId,
        post_number: u64,
    ) -> Result<Option<ThreadId>, ThreadError>;

    /// Bulk-fetch attachments for a slice of post IDs, grouped by post_id.
    ///
    /// Used by the thread view to load images without N+1 queries.
    async fn find_post_attachments(
        &self,
        post_ids: &[domains::models::PostId],
    ) -> Result<std::collections::HashMap<domains::models::PostId, Vec<domains::models::Attachment>>, ThreadError>;

    /// Set or clear the sticky flag on a thread.
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    async fn set_sticky(&self, id: ThreadId, sticky: bool) -> Result<(), ThreadError>;

    /// Open or close a thread to new replies.
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    async fn set_closed(&self, id: ThreadId, closed: bool) -> Result<(), ThreadError>;

    /// Delete the oldest non-sticky threads on the board if the count exceeds `max_threads`.
    ///
    /// Returns the number of threads pruned (0 if the board is within capacity).
    async fn prune_if_needed(&self, board_id: BoardId, max_threads: u32) -> Result<u32, ThreadError>;
}

/// Service handling thread lifecycle operations.
///
/// Generic over `TR: ThreadRepository` and `PR: PostRepository`.
pub struct ThreadService<TR: ThreadRepository, PR: PostRepository> {
    repo:      TR,
    post_repo: PR,
}

impl<TR: ThreadRepository, PR: PostRepository> ThreadService<TR, PR> {
    /// Construct a new `ThreadService`.
    pub fn new(repo: TR, post_repo: PR) -> Self {
        Self { repo, post_repo }
    }

    /// Allocate a new thread row for the given board.
    ///
    /// Returns the new `Thread`. The OP post is inserted separately by `PostService`,
    /// which then calls `ThreadRepository::set_op_post` to link it.
    #[instrument(skip(self), fields(board_id = %board_id))]
    pub async fn create_thread(&self, board_id: BoardId) -> Result<Thread, ThreadError> {
        let now = Utc::now();
        let thread = Thread {
            id: ThreadId(Uuid::new_v4()),
            board_id,
            op_post_id: None,
            reply_count: 0,
            bumped_at: now,
            sticky: false,
            closed: false,
            created_at: now,
        };
        let thread_id = self.repo.save(&thread).await?;
        let thread = Thread { id: thread_id, ..thread };
        info!(thread_id = %thread_id, board_id = %board_id, "thread created");
        Ok(thread)
    }

    /// Paginated thread list for a board, sorted by bump time.
    ///
    /// Sticky threads appear at the top regardless of bump time.
    #[instrument(skip(self), fields(board_id = %board_id))]
    pub async fn list_threads(
        &self,
        board_id: BoardId,
        page: Page,
    ) -> Result<Paginated<Thread>, ThreadError> {
        Ok(self.repo.find_by_board(board_id, page).await?)
    }

    /// Get all threads for catalog view (no pagination).
    #[instrument(skip(self), fields(board_id = %board_id))]
    pub async fn get_catalog(
        &self,
        board_id: BoardId,
    ) -> Result<Vec<ThreadSummary>, ThreadError> {
        Ok(self.repo.find_catalog(board_id).await?)
    }

    /// Get a single thread by ID.
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    #[instrument(skip(self), fields(thread_id = %id))]
    pub async fn get_thread(&self, id: ThreadId) -> Result<Thread, ThreadError> {
        self.repo.find_by_id(id).await.map_err(|e| match e {
            domains::errors::DomainError::NotFound { .. } => ThreadError::NotFound {
                id: id.to_string(),
            },
            other => ThreadError::Internal(other),
        })
    }

    /// Set the sticky flag on a thread.
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    #[instrument(skip(self), fields(thread_id = %id, sticky = sticky))]
    pub async fn set_sticky(&self, id: ThreadId, sticky: bool) -> Result<(), ThreadError> {
        self.repo.set_sticky(id, sticky).await.map_err(|e| match e {
            domains::errors::DomainError::NotFound { .. } => ThreadError::NotFound {
                id: id.to_string(),
            },
            other => ThreadError::Internal(other),
        })?;
        info!(thread_id = %id, sticky, "thread sticky flag set");
        Ok(())
    }

    /// Set the closed flag on a thread.
    ///
    /// Returns `ThreadError::NotFound` if the thread does not exist.
    #[instrument(skip(self), fields(thread_id = %id, closed = closed))]
    pub async fn set_closed(&self, id: ThreadId, closed: bool) -> Result<(), ThreadError> {
        self.repo.set_closed(id, closed).await.map_err(|e| match e {
            domains::errors::DomainError::NotFound { .. } => ThreadError::NotFound {
                id: id.to_string(),
            },
            other => ThreadError::Internal(other),
        })?;
        info!(thread_id = %id, closed, "thread closed flag set");
        Ok(())
    }

    /// Prune the oldest non-sticky threads on a board if over capacity.
    ///
    /// If `thread_count > max_threads`, deletes the oldest non-sticky threads
    /// until exactly `max_threads` remain. Returns the number of threads deleted.
    ///
    /// Called by `PostService` after a new thread is created.
    #[instrument(skip(self), fields(board_id = %board_id, max_threads = max_threads))]
    pub async fn prune_if_needed(
        &self,
        board_id: BoardId,
        max_threads: u32,
    ) -> Result<u32, ThreadError> {
        let count = self.repo.count_by_board(board_id).await?;
        if count > max_threads {
            let deleted = self.repo.prune_oldest(board_id, max_threads).await?;
            if deleted > 0 {
                warn!(
                    board_id = %board_id,
                    pruned = deleted,
                    "pruned old threads to stay within max_threads limit"
                );
            }
            return Ok(deleted);
        }
        Ok(0)
    }
}

#[async_trait]
impl<TR: ThreadRepository, PR: PostRepository> ThreadRepo for ThreadService<TR, PR> {
    async fn create_thread(&self, board_id: BoardId) -> Result<Thread, ThreadError> {
        self.create_thread(board_id).await
    }
    async fn list_threads(&self, board_id: BoardId, page: Page) -> Result<Paginated<Thread>, ThreadError> {
        self.list_threads(board_id, page).await
    }
    async fn get_catalog(&self, board_id: BoardId) -> Result<Vec<ThreadSummary>, ThreadError> {
        self.get_catalog(board_id).await
    }
    async fn get_thread(&self, id: ThreadId) -> Result<Thread, ThreadError> {
        self.get_thread(id).await
    }
    async fn list_posts(&self, thread_id: ThreadId, page: Page) -> Result<Paginated<Post>, ThreadError> {
        self.post_repo.find_by_thread(thread_id, page).await
            .map_err(ThreadError::Internal)
    }
    async fn list_all_posts(&self, thread_id: ThreadId) -> Result<Vec<Post>, ThreadError> {
        self.post_repo.find_all_by_thread(thread_id).await
            .map_err(ThreadError::Internal)
    }
    async fn find_thread_id_by_post_number(
        &self,
        board_id: BoardId,
        post_number: u64,
    ) -> Result<Option<ThreadId>, ThreadError> {
        self.post_repo.find_thread_id_by_post_number(board_id, post_number).await
            .map_err(ThreadError::Internal)
    }
    async fn find_post_attachments(
        &self,
        post_ids: &[domains::models::PostId],
    ) -> Result<std::collections::HashMap<domains::models::PostId, Vec<domains::models::Attachment>>, ThreadError> {
        self.post_repo.find_attachments_by_post_ids(post_ids).await
            .map_err(ThreadError::Internal)
    }
    async fn set_sticky(&self, id: ThreadId, sticky: bool) -> Result<(), ThreadError> {
        self.set_sticky(id, sticky).await
    }
    async fn set_closed(&self, id: ThreadId, closed: bool) -> Result<(), ThreadError> {
        self.set_closed(id, closed).await
    }
    async fn prune_if_needed(&self, board_id: BoardId, max_threads: u32) -> Result<u32, ThreadError> {
        self.prune_if_needed(board_id, max_threads).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domains::ports::{MockPostRepository, MockThreadRepository};

    #[tokio::test]
    async fn create_thread_happy_path() {
        let mut mock = MockThreadRepository::new();
        mock.expect_save()
            .times(1)
            .returning(|t| Ok(t.id));

        let svc = ThreadService::new(mock, MockPostRepository::new());
        let result = svc.create_thread(BoardId::new()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_sticky_not_found() {
        let mut mock = MockThreadRepository::new();
        mock.expect_set_sticky()
            .times(1)
            .returning(|_, _| Err(domains::errors::DomainError::not_found("thread")));

        let svc = ThreadService::new(mock, MockPostRepository::new());
        let result = svc.set_sticky(ThreadId::new(), true).await;
        assert!(matches!(result, Err(ThreadError::NotFound { .. })));
    }

    #[tokio::test]
    async fn prune_if_needed_no_prune() {
        let mut mock = MockThreadRepository::new();
        mock.expect_count_by_board()
            .times(1)
            .returning(|_| Ok(50));
        // prune_oldest should NOT be called

        let svc = ThreadService::new(mock, MockPostRepository::new());
        let deleted = svc.prune_if_needed(BoardId::new(), 100).await.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn prune_if_needed_triggers_prune() {
        let mut mock = MockThreadRepository::new();
        mock.expect_count_by_board()
            .times(1)
            .returning(|_| Ok(110));
        mock.expect_prune_oldest()
            .times(1)
            .returning(|_, _| Ok(10));

        let svc = ThreadService::new(mock, MockPostRepository::new());
        let deleted = svc.prune_if_needed(BoardId::new(), 100).await.unwrap();
        assert_eq!(deleted, 10);
    }
}
