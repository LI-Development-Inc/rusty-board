//! Service-layer tests using mockall-generated mock repositories.
//!
//! These tests live here (in the integration-tests crate) rather than in the
//! service crate itself because they import `MockBoardRepository` from
//! `domains::ports` which is only generated under `#[cfg(test)]`.

use chrono::Utc;
use domains::{
    errors::DomainError,
    models::*,
    ports::MockBoardRepository,
};
use services::board::{BoardError, BoardService};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn sample_board(slug: &str) -> Board {
    Board {
        id:         BoardId::new(),
        slug:       Slug::new(slug).unwrap(),
        title:      "Sample Board".to_owned(),
        rules:      "Be civil.".to_owned(),
        created_at: Utc::now(),
    }
}

// ─── create_board ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_board_returns_board_with_correct_slug_and_title() {
    let mut mock = MockBoardRepository::new();
    mock.expect_save().times(1).returning(|_| Ok(()));
    // Also need find_config for get_config call? No — create_board only calls save.

    let svc = BoardService::new(mock);
    let board = svc.create_board("tech", "Technology", "No trolling").await.unwrap();
    assert_eq!(board.slug.as_str(), "tech");
    assert_eq!(board.title, "Technology");
    assert_eq!(board.rules, "No trolling");
}

#[tokio::test]
async fn create_board_rejects_invalid_slug() {
    let svc = BoardService::new(MockBoardRepository::new());
    let err = svc.create_board("My Board!", "Title", "").await.unwrap_err();
    assert!(matches!(err, BoardError::InvalidSlug { .. }));
}

#[tokio::test]
async fn create_board_propagates_repo_error() {
    let mut mock = MockBoardRepository::new();
    mock.expect_save()
        .times(1)
        .returning(|_| Err(DomainError::internal("db down")));

    let svc = BoardService::new(mock);
    let err = svc.create_board("tech", "Technology", "").await.unwrap_err();
    assert!(matches!(err, BoardError::Internal(_)));
}

// ─── get_by_slug ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_by_slug_returns_existing_board() {
    let expected = sample_board("tech");
    let returned = expected.clone();

    let mut mock = MockBoardRepository::new();
    mock.expect_find_by_slug()
        .times(1)
        .returning(move |_| Ok(returned.clone()));

    let svc = BoardService::new(mock);
    let board = svc.get_by_slug("tech").await.unwrap();
    assert_eq!(board.slug.as_str(), "tech");
}

#[tokio::test]
async fn get_by_slug_maps_not_found() {
    let mut mock = MockBoardRepository::new();
    mock.expect_find_by_slug()
        .times(1)
        .returning(|_| Err(DomainError::not_found("board")));

    let svc = BoardService::new(mock);
    let err = svc.get_by_slug("nope").await.unwrap_err();
    assert!(matches!(err, BoardError::NotFound { .. }));
}

// ─── get_by_id ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_by_id_returns_existing_board() {
    let board = sample_board("tech");
    let id = board.id;
    let returned = board.clone();

    let mut mock = MockBoardRepository::new();
    mock.expect_find_by_id()
        .times(1)
        .returning(move |_| Ok(returned.clone()));

    let svc = BoardService::new(mock);
    let result = svc.get_by_id(id).await.unwrap();
    assert_eq!(result.id, id);
}

// ─── list_boards ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_boards_returns_empty_paginated_when_no_boards() {
    let mut mock = MockBoardRepository::new();
    mock.expect_find_all()
        .times(1)
        .returning(|p| Ok(Paginated::new(vec![], 0, p, 15)));

    let svc = BoardService::new(mock);
    let result = svc.list_boards(Page::new(1)).await.unwrap();
    assert_eq!(result.total, 0);
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn list_boards_returns_paginated_with_items() {
    let board = sample_board("tech");
    let items = vec![board];

    let mut mock = MockBoardRepository::new();
    mock.expect_find_all()
        .times(1)
        .returning(move |p| Ok(Paginated::new(items.clone(), 1, p, 15)));

    let svc = BoardService::new(mock);
    let result = svc.list_boards(Page::new(1)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items.len(), 1);
}

// ─── delete_board ────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_board_returns_ok_on_success() {
    let mut mock = MockBoardRepository::new();
    mock.expect_delete().times(1).returning(|_| Ok(()));

    let svc = BoardService::new(mock);
    assert!(svc.delete_board(BoardId::new()).await.is_ok());
}

#[tokio::test]
async fn delete_board_returns_not_found_when_missing() {
    let mut mock = MockBoardRepository::new();
    mock.expect_delete()
        .times(1)
        .returning(|_| Err(DomainError::not_found("board")));

    let svc = BoardService::new(mock);
    let err = svc.delete_board(BoardId::new()).await.unwrap_err();
    assert!(matches!(err, BoardError::NotFound { .. }));
}

// ─── get_config ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_config_returns_default_config() {
    let config = BoardConfig::default();

    let mut mock = MockBoardRepository::new();
    mock.expect_find_config()
        .times(1)
        .returning(move |_| Ok(config.clone()));

    let svc = BoardService::new(mock);
    let result = svc.get_config(BoardId::new()).await.unwrap();
    assert_eq!(result.bump_limit, 500); // default
}

// ─── update_board ────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_board_changes_title() {
    let board = sample_board("tech");
    let id = board.id;
    let returned = board.clone();

    let mut mock = MockBoardRepository::new();
    mock.expect_find_by_id()
        .times(1)
        .returning(move |_| Ok(returned.clone()));
    mock.expect_save().times(1).returning(|_| Ok(()));

    let svc = BoardService::new(mock);
    let updated = svc
        .update_board(id, Some("New Title"), None)
        .await
        .unwrap();
    assert_eq!(updated.title, "New Title");
}

#[tokio::test]
async fn update_board_returns_not_found_if_missing() {
    let mut mock = MockBoardRepository::new();
    mock.expect_find_by_id()
        .times(1)
        .returning(|_| Err(DomainError::not_found("board")));

    let svc = BoardService::new(mock);
    let err = svc.update_board(BoardId::new(), Some("X"), None).await.unwrap_err();
    assert!(matches!(err, BoardError::NotFound { .. }));
}
