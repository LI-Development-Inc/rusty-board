//! Unit tests for domain model construction and validation.
//!
//! These tests verify the invariants enforced by value objects and domain structs.
//! No I/O. No mocks needed.

use domains::models::*;
use domains::errors::ValidationError;
use uuid::Uuid;

// ─── Slug ────────────────────────────────────────────────────────────────────

#[test]
fn slug_valid_lowercase_alphanumeric() {
    let s = Slug::new("tech").unwrap();
    assert_eq!(s.as_str(), "tech");
}

#[test]
fn slug_valid_with_hyphen_and_underscore() {
    let s = Slug::new("random-b_board").unwrap();
    assert_eq!(s.as_str(), "random-b_board");
}

#[test]
fn slug_rejects_uppercase() {
    assert!(Slug::new("Tech").is_err());
}

#[test]
fn slug_rejects_space() {
    assert!(Slug::new("my board").is_err());
}

#[test]
fn slug_rejects_empty() {
    assert!(Slug::new("").is_err());
}

#[test]
fn slug_rejects_too_long() {
    let s = "a".repeat(17);
    assert!(Slug::new(s).is_err());
}

#[test]
fn slug_accepts_max_length_16() {
    let s = "a".repeat(16);
    assert!(Slug::new(s).is_ok());
}

// ─── IpHash ──────────────────────────────────────────────────────────────────

#[test]
fn ip_hash_stores_raw_value() {
    let hash = IpHash::new("deadbeef");
    assert_eq!(hash.as_str(), "deadbeef");
}

// ─── FileSizeKb ──────────────────────────────────────────────────────────────

#[test]
fn file_size_kb_stores_value() {
    let sz = FileSizeKb(1024);
    assert_eq!(sz.0, 1024);
}

// ─── Page ────────────────────────────────────────────────────────────────────

#[test]
fn page_new_normalises_zero_to_one() {
    let p = Page::new(0);
    assert_eq!(p.number(), 1);
}

#[test]
fn page_new_keeps_positive_value() {
    let p = Page::new(5);
    assert_eq!(p.number(), 5);
}

// ─── Paginated ───────────────────────────────────────────────────────────────

#[test]
fn paginated_computes_total_pages() {
    let p: Paginated<i32> = Paginated::new(vec![1, 2, 3], 50, Page::new(1), 15);
    assert_eq!(p.total, 50);
    // total_pages = ceil(50 / 15) = 4
    assert_eq!(p.total_pages(), 4);
}

#[test]
fn paginated_empty_has_one_page() {
    let p: Paginated<i32> = Paginated::new(vec![], 0, Page::new(1), 15);
    assert_eq!(p.total_pages(), 1);
}

// ─── BoardConfig defaults ────────────────────────────────────────────────────

#[test]
fn board_config_default_bump_limit_500() {
    assert_eq!(BoardConfig::default().bump_limit, 500);
}

#[test]
fn board_config_default_allows_common_mime_types() {
    let cfg = BoardConfig::default();
    assert!(cfg.allowed_mimes.contains(&"image/jpeg".to_owned()));
    assert!(cfg.allowed_mimes.contains(&"image/png".to_owned()));
    assert!(cfg.allowed_mimes.contains(&"image/gif".to_owned()));
    assert!(cfg.allowed_mimes.contains(&"image/webp".to_owned()));
}

#[test]
fn board_config_default_rate_limiting_on() {
    let cfg = BoardConfig::default();
    assert!(cfg.rate_limit_enabled);
    assert_eq!(cfg.rate_limit_posts, 3);
}

#[test]
fn board_config_default_not_nsfw() {
    assert!(!BoardConfig::default().nsfw);
}

// ─── CurrentUser ─────────────────────────────────────────────────────────────

#[test]
fn current_user_from_claims_roundtrip() {
    let board_id = BoardId(Uuid::new_v4());
    let claims = Claims {
        user_id:      UserId(Uuid::new_v4()),
        role:         Role::Moderator,
        owned_boards: vec![board_id],
        exp:          9999999999,
    };
    let user = CurrentUser::from_claims(claims.clone());
    assert_eq!(user.id, claims.user_id);
    assert_eq!(user.role, Role::Moderator);
    assert_eq!(user.owned_boards, vec![board_id]);
}

#[test]
fn current_user_admin_is_moderator_or_above() {
    let user = CurrentUser {
        id: UserId(Uuid::new_v4()),
        role: Role::Admin,
        owned_boards: vec![],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    assert!(user.is_moderator_or_above());
    assert!(user.is_admin());
}

#[test]
fn current_user_moderator_is_moderator_or_above() {
    let user = CurrentUser {
        id: UserId(Uuid::new_v4()),
        role: Role::Moderator,
        owned_boards: vec![],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    assert!(user.is_moderator_or_above());
    assert!(!user.is_admin());
}

#[test]
fn current_user_janitor_is_not_moderator_or_above() {
    let user = CurrentUser {
        id: UserId(Uuid::new_v4()),
        role: Role::Janitor,
        owned_boards: vec![],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    assert!(!user.is_moderator_or_above());
    assert!(!user.is_admin());
}

#[test]
fn current_user_can_manage_board_config_if_owner() {
    let board_id = BoardId(Uuid::new_v4());
    let user = CurrentUser {
        id: UserId(Uuid::new_v4()),
        role: Role::Janitor,
        owned_boards: vec![board_id],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    assert!(user.can_manage_board_config(board_id));
    assert!(!user.can_manage_board_config(BoardId(Uuid::new_v4())));
}

#[test]
fn current_user_admin_can_manage_any_board_config() {
    let user = CurrentUser {
        id: UserId(Uuid::new_v4()),
        role: Role::Admin,
        owned_boards: vec![],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    // Admin can manage any board even if not in owned_boards
    assert!(user.can_manage_board_config(BoardId(Uuid::new_v4())));
}

#[test]
fn current_user_user_id_accessor_matches_id_field() {
    let id = UserId(Uuid::new_v4());
    let user = CurrentUser {
        id,
        role: Role::Admin,
        owned_boards: vec![],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    assert_eq!(user.user_id(), id);
}

// ─── Role ────────────────────────────────────────────────────────────────────

#[test]
fn role_display() {
    assert_eq!(Role::Janitor.to_string(),   "janitor");
    assert_eq!(Role::Moderator.to_string(), "moderator");
    assert_eq!(Role::Admin.to_string(),     "admin");
}

#[test]
fn role_from_str() {
    use std::str::FromStr;
    assert_eq!(Role::from_str("janitor").unwrap(),   Role::Janitor);
    assert_eq!(Role::from_str("moderator").unwrap(), Role::Moderator);
    assert_eq!(Role::from_str("admin").unwrap(),     Role::Admin);
    assert!(Role::from_str("superuser").is_err());
}

// ─── FlagResolution ──────────────────────────────────────────────────────────

#[test]
fn flag_resolution_variants_distinct() {
    assert_ne!(FlagResolution::Approved, FlagResolution::Rejected);
}
