//! Service-layer tests for `ThreadService` using mock repositories.

use chrono::Utc;
use domains::{
    errors::DomainError,
    models::*,
    ports::{MockPostRepository, MockThreadRepository},
};
use services::thread::{ThreadError, ThreadService};

fn sample_thread(board_id: BoardId) -> Thread {
    Thread {
        id:          ThreadId::new(),
        board_id,
        op_post_id:  None,
        reply_count: 0,
        bumped_at:   Utc::now(),
        sticky:      false,
        closed:      false, cycle: false,
        created_at:  Utc::now(),
    }
}

// ─── create_thread ───────────────────────────────────────────────────────────

#[tokio::test]
async fn create_thread_happy_path() {
    let board_id = BoardId::new();
    let thread = sample_thread(board_id);
    let _id = thread.id;

    let mut mock = MockThreadRepository::new();
    mock.expect_save()
        .times(1)
        .returning(move |t| Ok(t.id));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let result = svc.create_thread(board_id).await;
    assert!(result.is_ok());
    let returned = result.unwrap();
    assert_eq!(returned.board_id, board_id);
    assert!(!returned.sticky);
    assert!(!returned.closed);
}

#[tokio::test]
async fn create_thread_propagates_repo_error() {
    let mut mock = MockThreadRepository::new();
    mock.expect_save()
        .times(1)
        .returning(|_| Err(DomainError::internal("constraint violation")));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let err = svc.create_thread(BoardId::new()).await.unwrap_err();
    assert!(matches!(err, ThreadError::Internal(_)));
}

// ─── set_sticky ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn set_sticky_to_true_succeeds() {
    let mut mock = MockThreadRepository::new();
    mock.expect_set_sticky()
        .times(1)
        .returning(|_, _| Ok(()));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    assert!(svc.set_sticky(ThreadId::new(), true).await.is_ok());
}

#[tokio::test]
async fn set_sticky_propagates_not_found() {
    let mut mock = MockThreadRepository::new();
    mock.expect_set_sticky()
        .times(1)
        .returning(|_, _| Err(DomainError::not_found("thread")));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let err = svc.set_sticky(ThreadId::new(), true).await.unwrap_err();
    assert!(matches!(err, ThreadError::NotFound { .. }));
}

// ─── set_closed ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn set_closed_to_true_succeeds() {
    let mut mock = MockThreadRepository::new();
    mock.expect_set_closed()
        .times(1)
        .returning(|_, _| Ok(()));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    assert!(svc.set_closed(ThreadId::new(), true).await.is_ok());
}

// ─── prune_if_needed ─────────────────────────────────────────────────────────

#[tokio::test]
async fn prune_if_needed_does_not_prune_below_limit() {
    let mut mock = MockThreadRepository::new();
    mock.expect_count_by_board()
        .times(1)
        .returning(|_| Ok(50));
    // prune_oldest must NOT be called when count < limit

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let pruned = svc.prune_if_needed(BoardId::new(), 100).await.unwrap();
    assert_eq!(pruned, 0);
}

#[tokio::test]
async fn prune_if_needed_prunes_when_over_limit() {
    let mut mock = MockThreadRepository::new();
    mock.expect_count_by_board()
        .times(1)
        .returning(|_| Ok(110));
    mock.expect_prune_oldest()
        .times(1)
        .returning(|_, _| Ok(10));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let pruned = svc.prune_if_needed(BoardId::new(), 100).await.unwrap();
    assert_eq!(pruned, 10);
}

#[tokio::test]
async fn prune_if_needed_does_not_prune_at_exactly_limit() {
    // count == max_threads → no pruning (uses strict >, not >=)
    let mut mock = MockThreadRepository::new();
    mock.expect_count_by_board()
        .times(1)
        .returning(|_| Ok(100));
    // prune_oldest must NOT be called

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let pruned = svc.prune_if_needed(BoardId::new(), 100).await.unwrap();
    assert_eq!(pruned, 0);
}

// ─── list_threads ────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_threads_returns_paginated() {
    let board_id = BoardId::new();
    let thread = sample_thread(board_id);

    let mut mock = MockThreadRepository::new();
    mock.expect_find_by_board()
        .times(1)
        .returning(move |_, p| Ok(Paginated::new(vec![thread.clone()], 1, p, 15)));

    let svc = ThreadService::new(mock, MockPostRepository::new());
    let result = svc.list_threads(board_id, Page::new(1)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items.len(), 1);
}
