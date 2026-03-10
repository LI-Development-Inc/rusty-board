//! Unit tests for domain model construction and validation.
//!
//! These tests verify the invariants enforced by value objects and domain structs.
//! No I/O. No mocks needed.

use domains::models::*;
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

#[test]
fn ip_hash_equality() {
    let a = IpHash::new("abc123");
    let b = IpHash::new("abc123");
    assert_eq!(a, b);

    let c = IpHash::new("xyz789");
    assert_ne!(a, c);
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
    assert_eq!(p.0, 1);
}

#[test]
fn page_new_keeps_positive_value() {
    let p = Page::new(5);
    assert_eq!(p.0, 5);
}

#[test]
fn page_offset_first_page() {
    let p = Page::new(1);
    assert_eq!(p.offset(15), 0);
}

#[test]
fn page_offset_second_page() {
    let p = Page::new(2);
    assert_eq!(p.offset(15), 15);
}

#[test]
fn page_offset_third_page() {
    let p = Page::new(3);
    assert_eq!(p.offset(10), 20);
}

// ─── Paginated ───────────────────────────────────────────────────────────────

#[test]
fn paginated_computes_total_pages_ceiling() {
    let p: Paginated<i32> = Paginated::new(vec![], 50, Page::new(1), 15);
    assert_eq!(p.total_pages(), 4); // ceil(50/15)
}

#[test]
fn paginated_empty_has_zero_pages() {
    // total=0 → (0 + 15 - 1) / 15 = 0 pages
    let p: Paginated<i32> = Paginated::new(vec![], 0, Page::new(1), 15);
    assert_eq!(p.total_pages(), 0);
}

#[test]
fn paginated_has_next_false_on_last_page() {
    // has_next checks fetched = (page-1)*page_size + items.len() < total
    // So items must reflect the actual items returned on this page.
    let items: Vec<i32> = (1..=15).collect(); // 15 items on page 1
    let p: Paginated<i32> = Paginated::new(items, 15, Page::new(1), 15);
    // fetched = 0 + 15 = 15, 15 < 15 → false
    assert!(!p.has_next());
}

#[test]
fn paginated_has_next_true_when_more_pages() {
    let p: Paginated<i32> = Paginated::new(vec![], 30, Page::new(1), 15);
    // total=30, page_size=15, page=1 → 2 pages → has next
    assert!(p.has_next());
}

#[test]
fn paginated_has_prev_false_on_first_page() {
    let p: Paginated<i32> = Paginated::new(vec![], 100, Page::new(1), 15);
    assert!(!p.has_prev());
}

#[test]
fn paginated_has_prev_true_after_first_page() {
    let p: Paginated<i32> = Paginated::new(vec![], 100, Page::new(2), 15);
    assert!(p.has_prev());
}

// ─── BoardConfig ─────────────────────────────────────────────────────────────

#[test]
fn board_config_default_bump_limit_500() {
    assert_eq!(BoardConfig::default().bump_limit, 500);
}

#[test]
fn board_config_default_allows_standard_image_types() {
    let cfg = BoardConfig::default();
    let mimes: Vec<&str> = cfg.allowed_mimes.iter().map(|s| s.as_str()).collect();
    assert!(mimes.contains(&"image/jpeg"));
    assert!(mimes.contains(&"image/png"));
    assert!(mimes.contains(&"image/gif"));
    assert!(mimes.contains(&"image/webp"));
}

#[test]
fn board_config_default_rate_limiting_on() {
    let cfg = BoardConfig::default();
    assert!(cfg.rate_limit_enabled);
    assert_eq!(cfg.rate_limit_posts, 3);
    assert_eq!(cfg.rate_limit_window_secs, 60);
}

#[test]
fn board_config_default_spam_filter_on() {
    let cfg = BoardConfig::default();
    assert!(cfg.spam_filter_enabled);
    assert!(cfg.duplicate_check);
}

#[test]
fn board_config_default_not_nsfw_not_captcha() {
    let cfg = BoardConfig::default();
    assert!(!cfg.nsfw);
    assert!(!cfg.captcha_required);
}

#[test]
fn board_config_default_future_flags_off() {
    let cfg = BoardConfig::default();
    assert!(!cfg.search_enabled);
    assert!(!cfg.archive_enabled);
    assert!(!cfg.federation_enabled);
}

// ─── CurrentUser ─────────────────────────────────────────────────────────────

#[test]
fn current_user_from_claims_roundtrip() {
    let board_id = BoardId(Uuid::new_v4());
    let user_id = UserId(Uuid::new_v4());
    let claims = Claims {
        user_id,
        username:         "testmod".into(),
        role:             Role::Janitor,
        owned_boards:     vec![board_id],
        volunteer_boards: vec![],
        exp:              9999999999,
    };
    let user = CurrentUser::from_claims(claims.clone());
    assert_eq!(user.id, user_id);
    assert_eq!(user.role, Role::Janitor);
    assert_eq!(user.owned_boards, vec![board_id]);
}

#[test]
fn current_user_user_id_accessor() {
    let id = UserId(Uuid::new_v4());
    let user = CurrentUser { id, role: Role::Admin, owned_boards: vec![], username: String::from("test"), volunteer_boards: vec![] };
    assert_eq!(user.user_id(), id);
}

#[test]
fn current_user_admin_is_moderator_or_above_and_admin() {
    let user = CurrentUser { id: UserId::new(), role: Role::Admin, owned_boards: vec![], username: String::from("test"), volunteer_boards: vec![] };
    assert!(user.is_admin());
    assert!(user.is_moderator_or_above());
    assert!(user.can_moderate());
    assert!(user.can_delete());
}

#[test]
fn current_user_janitor_is_moderator_or_above_not_admin() {
    let user = CurrentUser { id: UserId::new(), role: Role::Janitor, owned_boards: vec![], username: String::from("test"), volunteer_boards: vec![] };
    assert!(!user.is_admin());
    assert!(user.is_moderator_or_above());
    assert!(user.can_moderate());
    assert!(user.can_delete());
}

#[test]
fn current_user_board_volunteer_cannot_moderate_globally_but_can_delete() {
    let user = CurrentUser { id: UserId::new(), role: Role::BoardVolunteer, owned_boards: vec![], username: String::from("test"), volunteer_boards: vec![] };
    assert!(!user.is_admin());
    assert!(!user.is_moderator_or_above());
    assert!(!user.can_moderate());
    assert!(user.can_delete());
}

#[test]
fn current_user_can_manage_board_config_if_owner() {
    let owned = BoardId::new();
    let other = BoardId::new();
    let user = CurrentUser {
        id: UserId::new(),
        role: Role::BoardOwner,
        owned_boards: vec![owned],
        username: String::from("test"),
        volunteer_boards: vec![],
    };
    assert!(user.can_manage_board_config(owned));
    assert!(!user.can_manage_board_config(other));
}

#[test]
fn current_user_admin_can_manage_any_board() {
    let user = CurrentUser { id: UserId::new(), role: Role::Admin, owned_boards: vec![], username: String::from("test"), volunteer_boards: vec![] };
    assert!(user.can_manage_board_config(BoardId::new()));
    assert!(user.can_manage_board_config(BoardId::new()));
}

// ─── Role ────────────────────────────────────────────────────────────────────

#[test]
fn role_display_values() {
    assert_eq!(Role::Admin.to_string(),           "admin");
    assert_eq!(Role::Janitor.to_string(),          "janitor");
    assert_eq!(Role::BoardOwner.to_string(),       "board_owner");
    assert_eq!(Role::BoardVolunteer.to_string(),   "board_volunteer");
}

#[test]
fn role_from_str_roundtrip() {
    use std::str::FromStr;
    for (s, expected) in &[
        ("admin",            Role::Admin),
        ("janitor",          Role::Janitor),
        ("board_owner",      Role::BoardOwner),
        ("board_volunteer",  Role::BoardVolunteer),
    ] {
        let parsed = Role::from_str(s).unwrap();
        assert_eq!(parsed, *expected);
    }
}

#[test]
fn role_from_str_rejects_unknown() {
    use std::str::FromStr;
    assert!(Role::from_str("superuser").is_err());
    assert!(Role::from_str("").is_err());
    assert!(Role::from_str("Admin").is_err()); // case-sensitive
}

// ─── FlagResolution ──────────────────────────────────────────────────────────

#[test]
fn flag_resolution_variants_are_distinct() {
    assert_ne!(FlagResolution::Approved, FlagResolution::Rejected);
}

// ─── ID type new() constructors ──────────────────────────────────────────────

#[test]
fn all_id_types_have_new_constructor_that_generates_unique_ids() {
    let a = BoardId::new();
    let b = BoardId::new();
    assert_ne!(a, b, "BoardId::new() must generate unique IDs");

    let a = ThreadId::new();
    let b = ThreadId::new();
    assert_ne!(a, b, "ThreadId::new() must generate unique IDs");

    let a = PostId::new();
    let b = PostId::new();
    assert_ne!(a, b, "PostId::new() must generate unique IDs");

    let a = UserId::new();
    let b = UserId::new();
    assert_ne!(a, b, "UserId::new() must generate unique IDs");
}
