//! `WantsJson` extractor — reads `Accept: application/json` from request parts.
//!
//! Used by handlers that serve both browser (HTML/redirect) and API (JSON) clients.
//!
//! axum 0.8: FromRequestParts uses RPITIT — plain async fn in impl, no #[async_trait].

use axum::http::{header, request::Parts};

/// Indicates whether the request's `Accept` header includes `application/json`.
#[derive(Debug, Clone, Copy)]
pub struct WantsJson(pub bool);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for WantsJson {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let wants = parts
            .headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("application/json"))
            .unwrap_or(false);
        Ok(WantsJson(wants))
    }
}
