//! In-memory adapters — no external service dependencies.
//!
//! These are the "no-Redis, no-Postgres" adapters for development,
//! single-instance hobby deployments, and CI environments without services.
//!
//! | Adapter | Port | Notes |
//! |---------|------|-------|
//! | `InMemoryRateLimiter` | `RateLimiter` | DashMap sliding window; single-instance only |
//! | `InMemorySessionRepository` | `SessionRepository` | DashMap; single-instance only |

pub mod rate_limiter;
pub mod session_repository;

pub use rate_limiter::InMemoryRateLimiter;
pub use session_repository::InMemorySessionRepository;
