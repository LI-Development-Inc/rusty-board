//! Prometheus metrics endpoint: `GET /metrics`.
//!
//! Exports metrics in Prometheus text format. The registry is populated at
//! startup in `composition.rs` with the counters and histograms listed below,
//! then shared as `Arc<Registry>` via Axum state.
//!
//! ## Registered metrics (v1.0)
//! | Name | Type | Description |
//! |------|------|-------------|
//! | `http_requests_total` | Counter | Total HTTP requests, labelled by method and status |
//! | `http_request_duration_seconds` | Histogram | Request latency per route |
//! | `rate_limit_hits_total` | Counter | Post creations rejected by rate limiter |
//! | `spam_rejections_total` | Counter | Post creations rejected by spam filter |
//! | `ban_checks_total` | Counter | Active ban checks performed |
//! | `thread_prunes_total` | Counter | Threads pruned from boards at capacity |

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, family::Family, histogram::Histogram},
    registry::Registry,
};
use std::sync::Arc;

/// Labels used on the `http_requests_total` counter.
#[derive(Clone, Debug, Hash, PartialEq, Eq, prometheus_client::encoding::EncodeLabelSet)]
pub struct RequestLabels {
    /// HTTP method (e.g. `"GET"`, `"POST"`).
    pub method: String,
    /// HTTP status code as a string (e.g. `"200"`, `"429"`).
    pub status: String,
}

/// All application metrics, grouped for easy passing between functions.
///
/// Constructed once in `composition.rs` and cloned into each service/handler
/// that needs to record observations.
#[derive(Clone)]
pub struct AppMetrics {
    /// Total HTTP requests completed, labelled by HTTP method and status code.
    pub http_requests_total: Family<RequestLabels, Counter>,
    /// HTTP request latency in seconds (pre-bucketed).
    pub http_request_duration_seconds: Histogram,
    /// Post creations rejected by the IP rate limiter.
    pub rate_limit_hits_total: Counter,
    /// Post creations rejected by the spam heuristic.
    pub spam_rejections_total: Counter,
    /// Active-ban checks performed (total, not just hits).
    pub ban_checks_total: Counter,
    /// Threads pruned because the board exceeded its thread capacity.
    pub thread_prunes_total: Counter,
}

impl AppMetrics {
    /// Construct and register all metrics into `registry`.
    pub fn new(registry: &mut Registry) -> Self {
        let http_requests_total = Family::<RequestLabels, Counter>::default();
        registry.register(
            "http_requests",
            "Total HTTP requests completed",
            http_requests_total.clone(),
        );

        let http_request_duration_seconds = Histogram::new(
            // Buckets: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s
            [0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
        );
        registry.register(
            "http_request_duration_seconds",
            "HTTP request latency in seconds",
            http_request_duration_seconds.clone(),
        );

        let rate_limit_hits_total = Counter::default();
        registry.register(
            "rate_limit_hits",
            "Post creations rejected by the IP rate limiter",
            rate_limit_hits_total.clone(),
        );

        let spam_rejections_total = Counter::default();
        registry.register(
            "spam_rejections",
            "Post creations rejected by the spam heuristic",
            spam_rejections_total.clone(),
        );

        let ban_checks_total = Counter::default();
        registry.register(
            "ban_checks",
            "Active-ban checks performed",
            ban_checks_total.clone(),
        );

        let thread_prunes_total = Counter::default();
        registry.register(
            "thread_prunes",
            "Threads pruned due to board capacity",
            thread_prunes_total.clone(),
        );

        Self {
            http_requests_total,
            http_request_duration_seconds,
            rate_limit_hits_total,
            spam_rejections_total,
            ban_checks_total,
            thread_prunes_total,
        }
    }
}

/// `GET /metrics` — returns Prometheus text format metrics.
pub async fn metrics_handler(
    axum::extract::State(registry): axum::extract::State<Arc<Registry>>,
) -> Response {
    let mut buf = String::new();
    match encode(&mut buf, &registry) {
        Ok(()) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
            buf,
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

