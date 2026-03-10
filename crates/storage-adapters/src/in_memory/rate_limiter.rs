//! In-memory rate limiter adapter.
//!
//! Implements `RateLimiter` using a `DashMap`-backed sliding window counter.
//! Suitable for single-instance deployments where Redis is unavailable or
//! undesired (e.g. hobby instances, development, CI without services).
//!
//! # Limitations
//! - **Single-instance only.** Counts are not shared across processes.
//!   Do not use behind a load balancer unless sticky sessions are in effect.
//! - Entries are expired lazily on access and eagerly via a periodic sweep.
//!   Memory use is bounded to `O(active keys)` at any given time.
//!
//! # Usage
//! Enable by omitting the `redis` feature and using this adapter in the
//! composition root instead of `RedisRateLimiter`.

use async_trait::async_trait;
use dashmap::DashMap;
use domains::errors::DomainError;
use domains::ports::{RateLimitKey, RateLimitStatus, RateLimiter};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A single counter bucket for one rate-limit key.
#[derive(Debug, Clone)]
struct Bucket {
    /// Number of posts recorded in the current window.
    count: u32,
    /// When the current window expires.
    expires_at: Instant,
    /// Duration of the current window (needed to reset TTL on increment).
    #[allow(dead_code)]
    window: Duration,
}

impl Bucket {
    fn new(window_secs: u32) -> Self {
        let window = Duration::from_secs(window_secs as u64);
        Self {
            count:      1,
            expires_at: Instant::now() + window,
            window,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    fn remaining_secs(&self) -> u32 {
        self.expires_at
            .saturating_duration_since(Instant::now())
            .as_secs() as u32
    }
}

/// In-memory sliding window rate limiter.
///
/// Keys follow the same logical structure as the Redis adapter:
/// IP hash + board ID pair. The map is wrapped in `Arc` so the limiter
/// can be cheaply cloned for use across multiple Axum route handlers.
pub struct InMemoryRateLimiter {
    buckets:   Arc<DashMap<String, Bucket>>,
    max_posts: u32,
}

impl InMemoryRateLimiter {
    /// Create a new in-memory rate limiter.
    ///
    /// `max_posts` is the *default* per-window limit passed to `check()`.
    /// The actual per-board limit comes from `BoardConfig.rate_limit_posts` and
    /// is passed at the `check()` call site in `PostService`.
    pub fn new(max_posts: u32) -> Self {
        Self {
            buckets:   Arc::new(DashMap::new()),
            max_posts,
        }
    }

    fn map_key(key: &RateLimitKey) -> String {
        format!("rl:{}:{}", key.ip_hash, key.board_id)
    }

    /// Remove expired entries from the map.
    ///
    /// Called lazily on each `check()` / `increment()`. In production this
    /// keeps memory bounded without a background task.
    fn sweep_expired(&self) {
        self.buckets.retain(|_, b| !b.is_expired());
    }
}

impl Clone for InMemoryRateLimiter {
    fn clone(&self) -> Self {
        Self {
            buckets:   Arc::clone(&self.buckets),
            max_posts: self.max_posts,
        }
    }
}

#[async_trait]
impl RateLimiter for InMemoryRateLimiter {
    async fn check(&self, key: &RateLimitKey) -> Result<RateLimitStatus, DomainError> {
        self.sweep_expired();
        let k = Self::map_key(key);

        match self.buckets.get(&k) {
            None => {
                // No bucket yet → this key is not rate-limited.
                Ok(RateLimitStatus::Allowed {
                    remaining: self.max_posts,
                })
            }
            Some(bucket) if bucket.is_expired() => {
                // Bucket exists but has expired; treat as fresh.
                Ok(RateLimitStatus::Allowed {
                    remaining: self.max_posts,
                })
            }
            Some(bucket) => {
                if bucket.count >= self.max_posts {
                    Ok(RateLimitStatus::Exceeded {
                        retry_after_secs: bucket.remaining_secs(),
                    })
                } else {
                    Ok(RateLimitStatus::Allowed {
                        remaining: self.max_posts.saturating_sub(bucket.count),
                    })
                }
            }
        }
    }

    async fn increment(&self, key: &RateLimitKey, window_secs: u32) -> Result<(), DomainError> {
        self.sweep_expired();
        let k = Self::map_key(key);

        self.buckets
            .entry(k)
            .and_modify(|b| {
                if b.is_expired() {
                    // Window has elapsed — start a new one.
                    *b = Bucket::new(window_secs);
                } else {
                    b.count = b.count.saturating_add(1);
                }
            })
            .or_insert_with(|| Bucket::new(window_secs));

        Ok(())
    }

    async fn reset(&self, key: &RateLimitKey) -> Result<(), DomainError> {
        let k = Self::map_key(key);
        self.buckets.remove(&k);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domains::models::{BoardId, IpHash};

    fn make_key() -> RateLimitKey {
        RateLimitKey {
            ip_hash:  IpHash::new("abc123".to_owned()),
            board_id: BoardId::new(),
        }
    }

    #[tokio::test]
    async fn fresh_key_is_allowed_with_full_remaining() {
        let rl = InMemoryRateLimiter::new(5);
        let k = make_key();
        let status = rl.check(&k).await.unwrap();
        assert!(matches!(status, RateLimitStatus::Allowed { remaining: 5 }));
    }

    #[tokio::test]
    async fn increment_then_check_decrements_remaining() {
        let rl = InMemoryRateLimiter::new(5);
        let k = make_key();
        rl.increment(&k, 60).await.unwrap();
        rl.increment(&k, 60).await.unwrap();
        let status = rl.check(&k).await.unwrap();
        assert!(matches!(status, RateLimitStatus::Allowed { remaining: 3 }));
    }

    #[tokio::test]
    async fn exceeds_limit_after_max_increments() {
        let rl = InMemoryRateLimiter::new(3);
        let k = make_key();
        for _ in 0..3 {
            rl.increment(&k, 60).await.unwrap();
        }
        let status = rl.check(&k).await.unwrap();
        assert!(matches!(status, RateLimitStatus::Exceeded { .. }));
    }

    #[tokio::test]
    async fn reset_clears_count() {
        let rl = InMemoryRateLimiter::new(3);
        let k = make_key();
        for _ in 0..3 {
            rl.increment(&k, 60).await.unwrap();
        }
        rl.reset(&k).await.unwrap();
        let status = rl.check(&k).await.unwrap();
        assert!(matches!(status, RateLimitStatus::Allowed { remaining: 3 }));
    }

    #[tokio::test]
    async fn different_keys_are_tracked_independently() {
        let rl = InMemoryRateLimiter::new(2);
        let k1 = make_key();
        let k2 = make_key();
        rl.increment(&k1, 60).await.unwrap();
        rl.increment(&k1, 60).await.unwrap();
        // k1 is exceeded, k2 is fresh
        assert!(matches!(rl.check(&k1).await.unwrap(), RateLimitStatus::Exceeded { .. }));
        assert!(matches!(rl.check(&k2).await.unwrap(), RateLimitStatus::Allowed { .. }));
    }
}
