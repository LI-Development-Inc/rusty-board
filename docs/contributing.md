# contributing.md
# Contributing to rusty-board

Thank you for your interest in contributing. This document explains how to get started, the development workflow, and what we expect from contributors.

---

## Getting Started

### Prerequisites

- Rust 1.75+ (`rustup toolchain install stable`)
- Docker + Docker Compose (for local infrastructure)
- `cargo-audit`: `cargo install cargo-audit`
- `sqlx-cli`: `cargo install sqlx-cli --no-default-features --features postgres`

### Setup

```bash
git clone https://github.com/your-org/rusty-board
cd rusty-board
cp .env.example .env
# Edit .env — at minimum set JWT_SECRET to any 32+ char random string

# Start Postgres, Redis, and MinIO
make infra-up

# Run migrations
make migrate

# Build and run in dev mode
make watch
```

---

## Architecture Rules

Read `ARCHITECTURE.md` and `CONVENTIONS.md` before writing any code. The most important invariants:

1. **`domains/` and `services/` must never import adapter crates.** No `sqlx`, no `axum`, no `redis` in these crates.
2. **`#[cfg(feature)]` branches belong exclusively in `composition.rs`.** Nowhere else.
3. **`BoardConfig` is the only runtime behaviour toggle.** If you want to add a per-board switch, add a field to `BoardConfig`, add a migration, add a `BoardService` branch, add a dashboard UI control. Do not add env vars or feature flags for per-board behaviour.
4. **Handlers are thin.** Extract from request, call service, map error. No business logic in handlers.

Violating any of these is grounds for rejection regardless of test coverage.

---

## Development Workflow

### Running tests

```bash
# All tests
make test

# Specific crate
cargo test -p services

# Integration tests (requires infrastructure up)
cargo test -p integration-tests
```

### Linting

```bash
make lint          # cargo fmt + cargo clippy -D warnings
cargo audit        # dependency vulnerability check
```

### Adding a new port

See `PORTS.md §Adding a new port` — the steps must be followed in order. A port trait must be defined in `domains/ports.rs` and documented in `PORTS.md` **before** any adapter implementation is written.

### Adding a `BoardConfig` field

1. Add field to `BoardConfig` struct in `domains/models.rs` with a `Default` implementation
2. Add a migration: `crates/storage-adapters/src/migrations/V0XX__add_board_config_field.sql`
3. Add service branch in the relevant service method
4. Update `board_owner_dashboard.html` to expose the field
5. Update `CHANGELOG.md`

### Adding an HTTP endpoint

1. Add to `TECHNICALSPECS.md §5` first (design before code)
2. Add handler in the relevant `handlers/` file — thin: extract, call service, map error
3. Add route in the relevant `routes/` file
4. Add integration test in `crates/integration-tests/tests/`
5. Update `docs/api.md`

---

## Commit Style

```
type(scope): short description

Longer explanation if needed. Wrap at 72 chars.

BREAKING CHANGE: if applicable
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`

No "TODO", "FIXME", "WIP", or "hack" in commit messages on `main`.

---

## Pull Request Requirements

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-features -- -D warnings` passes (zero warnings)
- [ ] `cargo test --all-features` passes
- [ ] `cargo audit` — no new high/critical advisories
- [ ] New public API documented in `docs/api.md`
- [ ] New ports documented in `docs/PORTS.md`
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`
- [ ] No `#[cfg(feature)]` outside `composition.rs`
- [ ] No business logic in handlers
- [ ] Contract test added if a new adapter is introduced

---

## Questions

Open a GitHub Discussion or reach out via the issue tracker.
