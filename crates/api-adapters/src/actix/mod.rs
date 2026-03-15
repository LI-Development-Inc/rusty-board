//! Actix-web adapter (planned for v2.0, feature `web-actix`).
//!
//! This module will provide an alternative HTTP layer implementation using
//! `actix-web` 4.x. The goal is to prove that all port-based handler logic
//! is truly framework-agnostic: every integration test that passes under
//! `web-axum` must also pass under `web-actix` without modifying the
//! `services/` or `domains/` crates.
//!
//! **Nothing is implemented here yet.** The module is declared so the crate
//! compiles with `#[cfg(feature = "web-actix")]` gates in place and the
//! module tree does not need restructuring when work begins in v2.0.
