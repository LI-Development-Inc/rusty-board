//! `domains` — the innermost core of rusty-board.
//!
//! This crate defines:
//! - Every domain model, value object, and enum (`models.rs`)
//! - Every port trait — the async boundaries to the outside world (`ports.rs`)
//! - The unified error type and its variants (`errors.rs`)
//!
//! # Invariants
//! - Depends **only** on `std`, `chrono`, `serde`, `uuid`, `thiserror`, `bytes`, `mime`.
//! - Contains **no** I/O, no framework imports, no `#[cfg(feature)]` expressions.
//! - No `unwrap()` or `expect()` anywhere in this crate.

pub mod errors;
pub mod models;
pub mod ports;

pub use errors::{DomainError, ValidationError};
pub use models::*;
pub use ports::*;
