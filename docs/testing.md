# testing.md
# rusty-board — Testing Guide

## Test Architecture

rusty-board has four layers of tests:

| Layer | Location | Tooling | Requires infra? |
|-------|----------|---------|----------------|
| Unit tests | Inline in `mod.rs` files | `#[test]` / `#[tokio::test]` | No |
| Service tests | `crates/integration-tests/tests/` | `mockall` | No |
| Handler tests | `crates/integration-tests/tests/api_*.rs` | `axum::test`, stub traits | No |
| Adapter contract tests | `crates/integration-tests/tests/port_contracts.rs` | `testcontainers` | Yes |

All tests in the first three layers run without a live database, Redis, or S3. They use mock or stub implementations of port traits.

## Running Tests

```bash
# All unit + service + handler tests (no infrastructure required)
make test

# Integration tests against real Postgres, Redis, MinIO (requires Docker)
make test-integration

# Single crate
cargo test -p domains --features web-axum,db-postgres,auth-jwt,media-local,redis
cargo test -p services --features web-axum,db-postgres,auth-jwt,media-local,redis
cargo test -p integration-tests --features web-axum,db-postgres,auth-jwt,media-local,redis

# Single test by name (substring match)
cargo test -p integration-tests --features web-axum,db-postgres,auth-jwt,media-local,redis text_only_post

# With output (don't suppress println!/tracing)
make test -- --nocapture

# Code coverage
cargo tarpaulin --features web-axum,db-postgres,auth-jwt,media-local,redis --out Html
open tarpaulin-report.html
```

## Test Conventions

### Unit tests

Inline in the source file they test:

```rust
// services/src/post/mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    use domains::ports::MockPostRepository;

    #[tokio::test]
    async fn create_post_returns_rate_limited_when_limiter_rejects() {
        // ...
    }
}
```

### Service tests with mocks

Use `mockall`-generated mocks from `domains::ports::Mock*` types:

```rust
use domains::ports::MockBoardRepository;

let mut mock = MockBoardRepository::new();
mock.expect_save()
    .times(1)
    .returning(|_| Ok(()));

let svc = BoardService::new(mock);
let board = svc.create_board("tech", "Technology", "").await.unwrap();
```

### Handler tests (integration)

Build a minimal router with a hand-rolled trait stub, fire requests with `tower::ServiceExt::oneshot`:

```rust
let app = board_public_router(OkBoardRepo::for_slug("tech"));

let req = Request::builder()
    .method(Method::GET)
    .uri("/boards")
    .header(header::ACCEPT, "application/json")
    .body(Body::empty())
    .unwrap();

let resp = app.oneshot(req).await.unwrap();
assert_eq!(resp.status(), StatusCode::OK);
```

### Fixtures

Use named fixtures from `tests/fixtures.rs` rather than constructing structs inline:

```rust
// Good
let config = fixtures::board_configs::permissive();

// Avoid — hides what matters about the config
let config = BoardConfig { rate_limit_enabled: false, ..BoardConfig::default() };
```

Available fixtures:
- `board_configs::permissive()` — rate limit off, spam filter off
- `board_configs::strict()` — rate limit 1/60s, spam filter on
- `board_configs::nsfw()` — NSFW flag set
- `board_configs::no_media()` — max_files=0
- `board_configs::forced_anon()` — forced_anon=true
- `boards::tech_board()`, `boards::thread()`, etc.
- `users::admin_claims()`, `users::mod_claims()`, etc.

### BoardConfig branch coverage

Every `if config.some_flag` branch in a service method must have two tests:

```rust
#[tokio::test]
async fn create_post_skips_spam_check_when_spam_filter_disabled() {
    let config = board_configs::permissive(); // spam_filter_enabled: false
    // ...
}

#[tokio::test]
async fn create_post_rejects_spam_when_filter_enabled() {
    let config = board_configs::strict(); // spam_filter_enabled: true
    // ...
}
```

## What NOT to Test

- The `Default` implementation of `BoardConfig` (covered by the model tests)
- Routing registration (the routes are tested by hitting them in handler tests)
- Template rendering (Askama checks at compile time; snapshot tests are in the Phase 8 backlog)
- SQL query correctness (covered by adapter contract tests against real Postgres)

## CI

The CI pipeline runs on every pull request:

```yaml
- cargo fmt --check
- cargo clippy --all-features -- -D warnings
- cargo test --all-features
- cargo audit
```

Adapter contract tests run in CI via `make test-integration` which spins up Docker services first. They are not tagged `#[ignore]` — they are in a separate Makefile target that requires Docker Compose to be available:

```bash
make test-integration
```

## Live Endpoint Smoke Tests

`make live-test` runs `scripts/live_test.sh` against a running server to smoke-test
all public and staff-authenticated endpoints.

### Prerequisites

```bash
make db-up        # start Postgres (Docker)
make migrate      # apply all migrations
make watch        # run the app with live reload (separate terminal)
make seed         # create boards, threads, posts, and staff accounts
make live-test    # smoke-test all endpoints
```

### Default Staff Accounts (seed)

| Username | Password | Role | Dashboard |
|---|---|---|---|
| `admin` | `admin123` | Admin | `/admin/dashboard` |
| `janitor` | `janitor123` | Janitor | `/janitor/dashboard` |
| `board_owner` | `owner123` | Board Owner | `/board-owner/dashboard` |
| `volunteer` | `vol123` | Board Volunteer | `/volunteer/dashboard` |

### Full Reset

```bash
make clean-all          # destroy Docker volumes and build artifacts
make db-up              # fresh Postgres
make migrate            # apply all 13 migrations
make watch              # start app
make seed               # seed all test data
make live-test          # confirm green
```
