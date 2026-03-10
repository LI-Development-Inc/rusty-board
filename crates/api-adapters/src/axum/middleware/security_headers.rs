//! Security response headers middleware.
//!
//! Applied globally to every response. Sets:
//!
//! | Header | Value |
//! |--------|-------|
//! | `X-Content-Type-Options` | `nosniff` |
//! | `X-Frame-Options` | `DENY` |
//! | `Referrer-Policy` | `strict-origin-when-cross-origin` |
//! | `Permissions-Policy` | `interest-cohort=()` |
//! | `Content-Security-Policy` | see below |
//!
//! CSP: `default-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'`
//!
//! These are conservative defaults appropriate for an imageboard that serves its
//! own static assets. Operators running behind a reverse proxy should also enable HSTS
//! at the proxy layer.

use axum::{body::Body, http::Request, middleware::Next, response::Response};

// 'unsafe-inline' is required because board/thread/login templates embed
// JavaScript in <script> blocks (quick-reply, flag modal, login handler).
// Extracting them to separate .js files is tracked as a v1.2 hardening task.
static CSP: &str =
    "default-src 'self'; img-src 'self' data: blob:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'";

/// Axum middleware that adds security-related HTTP headers to every response.
pub async fn security_headers_middleware(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::HeaderName::from_static("x-content-type-options"),
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("x-frame-options"),
        axum::http::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("referrer-policy"),
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("permissions-policy"),
        axum::http::HeaderValue::from_static("interest-cohort=()"),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("content-security-policy"),
        axum::http::HeaderValue::from_static(CSP),
    );
    response
}
