//! `auth-adapters` — authentication port implementations.
//!
//! - `common/` — argon2id hashing and claims helpers (always compiled)
//! - `jwt_bearer/` — JWT token provider (`auth-jwt` feature)
//! - `cookie_session/` — cookie session provider (`auth-cookie` feature, v1.1+)

pub mod common;

#[cfg(feature = "auth-jwt")]
pub mod jwt_bearer;

#[cfg(feature = "auth-cookie")]
pub mod cookie_session;
