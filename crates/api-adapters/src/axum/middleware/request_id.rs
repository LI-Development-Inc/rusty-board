//! Request-ID middleware.
//!
//! Assigns a unique `X-Request-Id` header to every inbound request so that
//! log lines can be correlated across the application and reverse-proxy layers.
//!
//! **Status — v1.1.1 planned.**  The `tracing-subscriber` structured logger
//! already emits per-request spans; a dedicated `X-Request-Id` header is the
//! next step. Implementation will use `tower-http::request_id::SetRequestId`
//! with a `uuid::Uuid::new_v4()` generator and echo the value back in the
//! response so callers can include it in bug reports.
//!
//! The module is declared now so the middleware stack wiring does not need a
//! module-tree restructure when the feature lands.
