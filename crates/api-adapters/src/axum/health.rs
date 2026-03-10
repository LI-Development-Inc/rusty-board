//! Health check endpoint: `GET /healthz`.
//!
//! Returns `{"status":"ok","checks":{...}}` when all dependencies are reachable.
//! Returns `503 Service Unavailable` with details when any dependency is unhealthy.
//!
//! Probed dependencies:
//! - PostgreSQL: `SELECT 1`
//! - Redis: `PING`
//!
//! Used by Docker `HEALTHCHECK`, Kubernetes readiness probes, and load balancer
//! health checks.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

/// Per-dependency health status.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Dependency responded successfully.
    Ok,
    /// Dependency failed to respond or returned an error.
    Fail,
}

/// Health check response body.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Overall status: `"ok"` if all checks pass, `"degraded"` otherwise.
    pub status: &'static str,
    /// Per-dependency results.
    pub checks: HealthChecks,
}

/// Breakdown of individual dependency checks.
#[derive(Debug, Serialize)]
pub struct HealthChecks {
    /// Database connectivity status.
    pub database: CheckStatus,
    /// Redis connectivity status.
    pub redis:    CheckStatus,
}

/// Pluggable health probe for the database.
///
/// Implemented by the Postgres adapter. Allows the health handler to probe
/// without importing sqlx directly into `api-adapters`.
#[async_trait::async_trait]
pub trait DatabaseProbe: Send + Sync + 'static {
    /// Attempt a lightweight database round-trip. Returns `true` if the database responded.
    async fn ping(&self) -> bool;
}

/// Pluggable health probe for Redis.
#[async_trait::async_trait]
pub trait RedisProbe: Send + Sync + 'static {
    /// Attempt a lightweight Redis round-trip. Returns `true` if Redis responded.
    async fn ping(&self) -> bool;
}

/// Combined health state injected via Axum `State`.
pub struct HealthState {
    /// Database probe implementation, injected from the Postgres adapter.
    pub db:    Arc<dyn DatabaseProbe>,
    /// Redis probe implementation, injected from the Redis adapter.
    pub redis: Arc<dyn RedisProbe>,
}

impl Clone for HealthState {
    fn clone(&self) -> Self {
        Self {
            db:    self.db.clone(),
            redis: self.redis.clone(),
        }
    }
}

/// `GET /healthz` — probe all dependencies and return health status.
pub async fn health_check(State(state): State<HealthState>) -> Response {
    let db_ok    = state.db.ping().await;
    let redis_ok = state.redis.ping().await;

    let checks = HealthChecks {
        database: if db_ok    { CheckStatus::Ok } else { CheckStatus::Fail },
        redis:    if redis_ok { CheckStatus::Ok } else { CheckStatus::Fail },
    };

    if db_ok && redis_ok {
        (
            StatusCode::OK,
            Json(HealthResponse { status: "ok", checks }),
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse { status: "degraded", checks }),
        )
            .into_response()
    }
}
