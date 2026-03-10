//! PostgreSQL connection pool construction.
//!
//! Creates a `sqlx::PgPool` from the database URL and connection limits
//! specified in `Settings`. The pool is passed to all Pg repository adapters.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Create a `PgPool` from a database URL with the given min/max connection counts.
///
/// This function is called once in `composition.rs` at startup.
/// Returns an error if the pool cannot be established (e.g. DB is unreachable).
pub async fn create_pool(
    db_url: &str,
    max_connections: u32,
    min_connections: u32,
) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(min_connections)
        .connect(db_url)
        .await
}
