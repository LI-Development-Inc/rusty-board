//! Shared utilities used across multiple service modules.
//!
//! These functions are deterministic, have no I/O, and carry no state.
//! They are the only place in `services` where low-level operations like
//! hashing, regex matching, and score computation live — keeping individual
//! service modules focused on business logic.

use chrono::{DateTime, Utc};
use domains::models::{ContentHash, IpHash, Page, Paginated, Slug};
use domains::errors::ValidationError;
use sha2::{Digest, Sha256};

/// Return the current UTC timestamp.
///
/// Centralised so tests can see a consistent call point and production code
/// avoids scattered `Utc::now()` calls.
pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

/// Validate a slug candidate string.
///
/// Returns `Ok(Slug)` if valid or `Err(ValidationError::InvalidSlug)` if not.
pub fn slug_validate(value: &str) -> Result<Slug, ValidationError> {
    Slug::new(value)
}

/// Compute the hash of a page query.
///
/// Returns the (offset, limit) pair for use in SQL queries.
pub fn paginate(page: Page, page_size: u32) -> (u32, u32) {
    (page.offset(page_size), page_size)
}

/// Build a `Paginated<T>` from raw rows and a total count.
pub fn into_paginated<T>(items: Vec<T>, total: u64, page: Page, page_size: u32) -> Paginated<T> {
    Paginated::new(items, total, page, page_size)
}

/// Hash `raw_ip` with `daily_salt` using SHA-256.
///
/// The raw IP address is **never** stored anywhere. Only its daily-salted hash
/// is persisted. The salt rotates on restart, preventing correlation after rotation.
///
/// # INVARIANT
/// This is the single authoritative implementation of IP hashing. All parts of
/// the system that need to compare IPs must go through this function.
pub fn hash_ip(raw_ip: &str, daily_salt: &str) -> IpHash {
    let mut hasher = Sha256::new();
    hasher.update(raw_ip.as_bytes());
    hasher.update(b":");
    hasher.update(daily_salt.as_bytes());
    let result = hasher.finalize();
    IpHash::new(hex::encode(result))
}

/// Compute the SHA-256 content hash of raw bytes.
///
/// Used for duplicate post detection and deduplication.
pub fn hash_content(data: &[u8]) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    ContentHash::new(hex::encode(hasher.finalize()))
}

/// Parse `>>postid` quote references from a post body.
///
/// Returns the quoted post ID strings found in the body.
/// This is used to render cross-post links in templates.
pub fn parse_quotes(body: &str) -> Vec<String> {
    let mut quotes = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(">>") {
            // Collect digits (and hyphens for UUID format)
            let id: String = rest
                .chars()
                .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
                .collect();
            if !id.is_empty() {
                quotes.push(id);
            }
        }
    }
    quotes
}

/// Compute a spam probability score for a post body.
///
/// Returns a value in `[0.0, 1.0]` where 1.0 is maximum spam likelihood.
/// The current heuristics are intentionally simple and conservative for v1.0:
/// - High ratio of URLs
/// - Excessive caps
/// - Repeated character sequences
/// - Short body with many special characters
/// - URL hostname present in `link_blacklist` (returns 1.0 immediately)
///
/// A richer ML-based approach can replace this function in v1.2+ without
/// changing any service interface.
pub fn score_spam(body: &str, link_blacklist: &[String]) -> f32 {
    if body.is_empty() {
        return 0.0;
    }

    // Link blacklist check — immediate rejection score
    if !link_blacklist.is_empty() {
        for url_start in body.match_indices("http://").chain(body.match_indices("https://")) {
            let rest = &body[url_start.0..];
            // Extract hostname: everything after "://" up to '/', '?', '#', or whitespace
            if let Some(after_scheme) = rest.find("://") {
                let host_start = &rest[after_scheme + 3..];
                let host_end = host_start
                    .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
                    .unwrap_or(host_start.len());
                let host = host_start[..host_end].to_lowercase();
                if link_blacklist.iter().any(|b| b.to_lowercase() == host) {
                    return 1.0;
                }
            }
        }
    }

    let mut score: f32 = 0.0;
    let len = body.len();

    // URL density heuristic
    let url_count = body.matches("http://").count() + body.matches("https://").count();
    if url_count > 0 {
        score += (url_count as f32 / (len as f32 / 80.0)).min(0.4);
    }

    // Excessive capitals heuristic
    let alpha_chars: Vec<char> = body.chars().filter(|c| c.is_alphabetic()).collect();
    if !alpha_chars.is_empty() {
        let caps_ratio = alpha_chars.iter().filter(|c| c.is_uppercase()).count() as f32
            / alpha_chars.len() as f32;
        if caps_ratio > 0.6 {
            score += (caps_ratio - 0.6) * 0.5;
        }
    }

    // Repeated character sequence (e.g. "aaaaa", "!!!!!!")
    let repeated = count_max_run(body);
    if repeated > 4 {
        score += ((repeated - 4) as f32 * 0.05).min(0.3);
    }

    // Very short body with mostly special characters
    if len < 20 {
        let special_ratio = body.chars().filter(|c| !c.is_alphanumeric() && !c.is_whitespace()).count() as f32
            / len as f32;
        score += special_ratio * 0.2;
    }

    score.min(1.0)
}

/// Count the length of the longest run of identical characters in `s`.
fn count_max_run(s: &str) -> usize {
    let mut max_run = 0;
    let mut current_run = 0;
    let mut last_char: Option<char> = None;
    for c in s.chars() {
        if Some(c) == last_char {
            current_run += 1;
        } else {
            current_run = 1;
            last_char = Some(c);
        }
        if current_run > max_run {
            max_run = current_run;
        }
    }
    max_run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ip_is_deterministic() {
        let h1 = hash_ip("192.168.1.1", "salt_abc");
        let h2 = hash_ip("192.168.1.1", "salt_abc");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_ip_differs_by_salt() {
        let h1 = hash_ip("192.168.1.1", "salt_1");
        let h2 = hash_ip("192.168.1.1", "salt_2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_ip_differs_by_ip() {
        let h1 = hash_ip("192.168.1.1", "salt");
        let h2 = hash_ip("192.168.1.2", "salt");
        assert_ne!(h1, h2);
    }

    #[test]
    fn parse_quotes_finds_references() {
        let body = "Hello\n>>abc123\nsome text\n>>def456";
        let quotes = parse_quotes(body);
        assert_eq!(quotes, vec!["abc123", "def456"]);
    }

    #[test]
    fn parse_quotes_empty_body() {
        assert!(parse_quotes("").is_empty());
    }

    #[test]
    fn score_spam_low_for_normal_post() {
        let score = score_spam("This is a normal post about things I like.", &[]);
        assert!(score < 0.3, "score was {score}");
    }

    #[test]
    fn score_spam_higher_for_url_spam() {
        let score = score_spam("Buy now at https://spam.example.com https://spam2.example.com", &[]);
        assert!(score > 0.2, "score was {score}");
    }

    #[test]
    fn score_spam_blacklisted_url_returns_max() {
        let blacklist = vec!["spam.example.com".to_owned()];
        let score = score_spam("Check this out https://spam.example.com/deal", &blacklist);
        assert_eq!(score, 1.0, "blacklisted URL should score 1.0");
    }

    #[test]
    fn score_spam_non_blacklisted_url_unaffected() {
        let blacklist = vec!["spam.example.com".to_owned()];
        let score = score_spam("Visit https://safe.example.com for more info", &blacklist);
        assert!(score < 1.0, "non-blacklisted URL should not score 1.0");
    }

    #[test]
    fn score_spam_empty_blacklist_does_not_affect_score() {
        let without = score_spam("Check https://example.com now", &[]);
        let with_empty: &[String] = &[];
        let also_without = score_spam("Check https://example.com now", with_empty);
        assert_eq!(without, also_without);
    }

    #[test]
    fn slug_validate_accepts_valid() {
        assert!(slug_validate("tech").is_ok());
        assert!(slug_validate("my-board").is_ok());
    }

    #[test]
    fn slug_validate_rejects_invalid() {
        assert!(slug_validate("CAPS").is_err());
        assert!(slug_validate("has space").is_err());
        assert!(slug_validate("").is_err());
    }

    #[test]
    fn paginate_first_page() {
        let (offset, limit) = paginate(Page::new(1), 15);
        assert_eq!(offset, 0);
        assert_eq!(limit, 15);
    }

    #[test]
    fn paginate_second_page() {
        let (offset, limit) = paginate(Page::new(2), 15);
        assert_eq!(offset, 15);
        assert_eq!(limit, 15);
    }
}
