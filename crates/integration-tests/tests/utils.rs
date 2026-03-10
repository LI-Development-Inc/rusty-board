//! Tests for the IP hashing and spam scoring utilities in `services::common::utils`.

use services::common::utils::*;
use domains::models::Page;

// ─── hash_ip ─────────────────────────────────────────────────────────────────

#[test]
fn hash_ip_is_deterministic() {
    let h1 = hash_ip("192.168.1.1", "2024-01-15");
    let h2 = hash_ip("192.168.1.1", "2024-01-15");
    assert_eq!(h1, h2);
}

#[test]
fn hash_ip_differs_for_different_salts() {
    let h1 = hash_ip("192.168.1.1", "2024-01-14");
    let h2 = hash_ip("192.168.1.1", "2024-01-15");
    assert_ne!(h1, h2, "daily rotation must change the hash");
}

#[test]
fn hash_ip_differs_for_different_ips() {
    let h1 = hash_ip("192.168.1.1", "salt");
    let h2 = hash_ip("192.168.1.2", "salt");
    assert_ne!(h1, h2);
}

#[test]
fn hash_ip_produces_64_char_hex_string() {
    let h = hash_ip("10.0.0.1", "test-salt");
    assert_eq!(h.as_str().len(), 64, "SHA-256 hex is 64 chars");
    assert!(h.as_str().chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn hash_ip_ipv6_address_supported() {
    let h = hash_ip("::1", "salt");
    assert_eq!(h.as_str().len(), 64);
}

// ─── hash_content ────────────────────────────────────────────────────────────

#[test]
fn hash_content_deterministic() {
    let data = b"the quick brown fox";
    let h1 = hash_content(data);
    let h2 = hash_content(data);
    assert_eq!(h1, h2);
}

#[test]
fn hash_content_differs_for_different_data() {
    let h1 = hash_content(b"hello");
    let h2 = hash_content(b"world");
    assert_ne!(h1, h2);
}

#[test]
fn hash_content_is_64_char_hex() {
    let h = hash_content(b"anything");
    assert_eq!(h.as_str().len(), 64);
}

// ─── parse_quotes ────────────────────────────────────────────────────────────

#[test]
fn parse_quotes_finds_uuid_references() {
    let body = "cool post\n>>550e8400-e29b-41d4-a716-446655440000\nmore text";
    let quotes = parse_quotes(body);
    assert_eq!(quotes, vec!["550e8400-e29b-41d4-a716-446655440000"]);
}

#[test]
fn parse_quotes_finds_multiple() {
    let body = ">>abc123\nsome stuff\n>>def456";
    let quotes = parse_quotes(body);
    assert_eq!(quotes, vec!["abc123", "def456"]);
}

#[test]
fn parse_quotes_empty_body() {
    assert!(parse_quotes("").is_empty());
}

#[test]
fn parse_quotes_no_quotes_in_body() {
    assert!(parse_quotes("just normal text without any references").is_empty());
}

#[test]
fn parse_quotes_ignores_single_gt() {
    // `>` is greentext, `>>` is a reference
    let quotes = parse_quotes(">implying this is a reference");
    assert!(quotes.is_empty());
}

// ─── score_spam ──────────────────────────────────────────────────────────────

#[test]
fn score_spam_is_low_for_normal_post() {
    let score = score_spam("Just a normal post about my favourite TV shows.", &[]);
    assert!(score < 0.3, "score was {score}");
}

#[test]
fn score_spam_is_zero_for_empty_body() {
    assert_eq!(score_spam("", &[]), 0.0);
}

#[test]
fn score_spam_is_higher_for_multiple_urls() {
    let spammy = "VISIT https://buy-stuff.example.com AND https://more-spam.example.com NOW!!!";
    let score = score_spam(spammy, &[]);
    assert!(score > 0.2, "score was {score}");
}

#[test]
fn score_spam_caps_at_1_0() {
    // Extremely spammy content should not exceed 1.0
    let all_urls = (0..20).map(|i| format!("https://spam{i}.example.com")).collect::<Vec<_>>().join(" ");
    let score = score_spam(&all_urls, &[]);
    assert!(score <= 1.0, "score exceeded 1.0: {score}");
}

#[test]
fn score_spam_higher_for_excessive_caps() {
    let capslock = "THIS IS VERY IMPORTANT BUY NOW AMAZING DEAL GREAT OFFER";
    let normal   = "This is very important. Buy now. Amazing deal. Great offer.";
    assert!(
        score_spam(capslock, &[]) > score_spam(normal, &[]),
        "excessive caps should score higher"
    );
}

// ─── paginate ────────────────────────────────────────────────────────────────

#[test]
fn paginate_first_page_offset_is_zero() {
    let (offset, limit) = paginate(Page::new(1), 15);
    assert_eq!(offset, 0);
    assert_eq!(limit, 15);
}

#[test]
fn paginate_second_page_offset_is_one_page_size() {
    let (offset, limit) = paginate(Page::new(2), 15);
    assert_eq!(offset, 15);
    assert_eq!(limit, 15);
}

#[test]
fn paginate_n_th_page() {
    let (offset, _) = paginate(Page::new(5), 20);
    assert_eq!(offset, 80); // (5-1) * 20
}
