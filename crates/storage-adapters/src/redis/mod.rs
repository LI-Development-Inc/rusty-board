//! Redis rate limiter adapter (`redis` feature).
//!
//! Implements `RateLimiter` using a sliding window counter stored in Redis.
//! Each key is `rl:{ip_hash}:{board_id}` with a TTL equal to the window.

pub mod connection;

use async_trait::async_trait;
use deadpool_redis::Pool;
use domains::errors::DomainError;
use domains::ports::{RateLimitKey, RateLimitStatus, RateLimiter};
use tracing::instrument;

/// Redis-backed `RateLimiter` using a sliding window counter.
///
/// Keys follow the format `rl:{ip_hash}:{board_id}`.
pub struct RedisRateLimiter {
    pool:      Pool,
    max_posts: u32,
}

impl RedisRateLimiter {
    /// Create a new rate limiter.
    ///
    /// `max_posts` is the default maximum posts per window. The per-board limit
    /// from `BoardConfig.rate_limit_posts` is passed at the call site in `PostService`.
    pub fn new(pool: Pool, max_posts: u32) -> Self {
        Self { pool, max_posts }
    }

    fn redis_key(key: &RateLimitKey) -> String {
        format!("rl:{}:{}", key.ip_hash, key.board_id)
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    #[instrument(skip(self))]
    async fn check(&self, key: &RateLimitKey) -> Result<RateLimitStatus, DomainError> {
        use deadpool_redis::redis::AsyncCommands;
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| DomainError::internal(format!("redis pool error: {e}")))?;

        let rk = Self::redis_key(key);
        let count: Option<u32> = conn
            .get(&rk)
            .await
            .map_err(|e| DomainError::internal(format!("redis get error: {e}")))?;

        let current = count.unwrap_or(0);
        if current >= self.max_posts {
            let ttl: i64 = conn
                .ttl(&rk)
                .await
                .map_err(|e| DomainError::internal(format!("redis ttl error: {e}")))?;
            Ok(RateLimitStatus::Exceeded {
                retry_after_secs: ttl.max(0) as u32,
            })
        } else {
            Ok(RateLimitStatus::Allowed {
                remaining: self.max_posts.saturating_sub(current),
            })
        }
    }

    #[instrument(skip(self))]
    async fn increment(&self, key: &RateLimitKey, window_secs: u32) -> Result<(), DomainError> {
        use deadpool_redis::redis::pipe;
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| DomainError::internal(format!("redis pool error: {e}")))?;

        let rk = Self::redis_key(key);
        // Increment and set TTL atomically using a pipeline
        let _: () = pipe()
            .incr(&rk, 1)
            .expire(&rk, window_secs as i64)
            .query_async(&mut *conn)
            .await
            .map_err(|e| DomainError::internal(format!("redis incr error: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn reset(&self, key: &RateLimitKey) -> Result<(), DomainError> {
        use deadpool_redis::redis::AsyncCommands;
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| DomainError::internal(format!("redis pool error: {e}")))?;

        let rk = Self::redis_key(key);
        let _: () = conn
            .del(&rk)
            .await
            .map_err(|e| DomainError::internal(format!("redis del error: {e}")))?;

        Ok(())
    }
}
