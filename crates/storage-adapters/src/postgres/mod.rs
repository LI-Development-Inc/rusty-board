//! PostgreSQL adapter implementations for all domain repository ports.
//!
//! All types in this module require the `db-postgres` feature and depend on `sqlx`.
//! The composition root selects these implementations when `db-postgres` is active.

pub mod connection;
pub mod repositories;
