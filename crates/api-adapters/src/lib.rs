//! `api-adapters` — HTTP transport layer for rusty-board.
//!
//! - `common/` — `ApiError`, DTOs, pagination (not feature-gated)
//! - `axum/` — Axum router, routes, handlers, middleware (`web-axum` feature)
//! - `actix/` — Actix-web app (`web-actix` feature, v1.x+)

pub mod common;

#[cfg(feature = "web-axum")]
pub mod axum;

#[cfg(feature = "web-actix")]
pub mod actix;
