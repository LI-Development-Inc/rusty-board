//! Redis connection pool construction.

use anyhow::Context;
use deadpool_redis::{Config, Pool, Runtime};

/// Create a `deadpool_redis::Pool` from a Redis URL.
pub fn create_pool(redis_url: &str) -> anyhow::Result<Pool> {
    let connection_info: deadpool_redis::redis::ConnectionInfo = redis_url
        .parse()
        .context("invalid Redis URL")?;
    let cfg = Config {
        connection: Some(connection_info.into()),
        url: None,
        pool: None,
    };
    cfg.create_pool(Some(Runtime::Tokio1))
        .context("failed to build deadpool-redis pool")
}
