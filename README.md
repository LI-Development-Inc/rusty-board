# rusty-board

An anonymous imageboard engine written in Rust following hexagonal architecture.

## Architecture

The codebase follows strict hexagonal architecture with compile-time adapter selection via Cargo feature flags. There is a single composition root (`cmd/rusty-board/src/composition.rs`) which is the **only** file containing `#[cfg(feature)]` branches.

```
cmd/rusty-board          — composition root + binary
crates/domains           — innermost core (no external deps except std + serde/uuid/chrono)
crates/services          — business logic (depends only on domains)
crates/storage-adapters  — PostgreSQL repos, Redis rate limiter, S3/local media
crates/auth-adapters     — JWT + argon2id (feature-gated)
crates/api-adapters      — Axum HTTP layer (feature-gated)
crates/configs           — Settings struct (infrastructure configuration only)
```

### Key Invariants

1. `domains/` and `services/` contain **no feature flags, no adapter imports**
2. `BoardConfig` is the **only** path from the dashboard to service behaviour
3. EXIF stripping is **unconditional** — not a toggle
4. Active ban check always runs — not a `BoardConfig` toggle
5. Audit log write failures **never propagate** to the caller
6. Raw IP addresses are **never stored** — only `SHA-256(ip + daily_salt)`
7. `#[cfg(feature)]` branches appear **only** in `composition.rs`

## Prerequisites

- Rust 1.75+ (required for native RPITIT async in traits)
- Docker + Docker Compose
- `sqlx-cli`: `cargo install sqlx-cli --no-default-features --features postgres`
- `cargo-watch` (optional, for hot reload): `cargo install cargo-watch`

## Quick Start

```bash
# 1. Clone and set up environment
cp .env.example .env
# Edit .env — at minimum set a real JWT_SECRET

# 2. Start infrastructure (Postgres + Redis + MinIO)
make infra-up

# 3. Run migrations
make migrate

# 4. Start the application in development mode
make watch
# Or: cargo run --features web-axum,db-postgres,auth-jwt,media-local,redis
```

The application will be available at `http://localhost:8080`.

## Feature Flags

| Flag | Description |
|------|-------------|
| `web-axum` | Axum HTTP server (default) |
| `db-postgres` | PostgreSQL repositories via sqlx (default) |
| `auth-jwt` | JWT bearer token authentication (default) |
| `media-local` | Local filesystem media storage (default) |
| `media-s3` | S3/MinIO/R2 media storage (mutually exclusive with `media-local`) |
| `redis` | Redis rate limiter (default) |
| `video` | Video keyframe extraction via ffmpeg |
| `documents` | PDF first-page rendering via pdfium |

## Development

```bash
make build           # Build release binary
make test            # Run unit tests
make lint            # Run clippy -D warnings
make fmt             # Auto-format code
make audit           # Security advisory check
make sqlx-prepare    # Regenerate sqlx-data.json after query changes
make cover           # Code coverage report
```

## Database Migrations

Migrations live in `crates/storage-adapters/src/migrations/`. They are applied automatically on startup via `sqlx::migrate!()`.

```bash
make migrate                    # Apply pending migrations
make migrate-add NAME=my_thing  # Create a new migration file
make db-reset                   # Drop and recreate DB (destructive!)
```

## Docker

```bash
# Build image with default features
make docker-build

# Build with S3 + video support
docker build --build-arg FEATURES="web-axum,db-postgres,auth-jwt,media-s3,redis,video" -t rusty-board .

# Run full stack
docker compose up
```

## API

See `TECHNICALSPECS.md §5` for the full endpoint table.

Public endpoints require no authentication. Moderator/admin endpoints require a `Bearer` token in the `Authorization` header, obtained via `POST /auth/login`.

## Configuration

All configuration is via environment variables. See `.env.example` for the full documented list.

Per-board behavioural configuration (bump limits, rate limits, spam filters, etc.) lives in `board_configs` in the database and is managed through the dashboard at `/board/:slug/config`. This is intentionally separate from infrastructure `Settings`.

## Security

- Argon2id password hashing (OWASP recommended parameters: m=19456, t=2, p=1)
- JWT HS256 tokens; stateless (v1.1+ will add revocable cookie sessions)
- EXIF metadata stripped from all uploaded images unconditionally
- Raw IP addresses never stored (SHA-256 with rotating daily salt)
- All secrets via environment variables only; `secrecy::Secret<String>` prevents accidental logging
- CSP, X-Frame-Options, X-Content-Type-Options headers on all responses

## Contributing

- Run `make fmt lint test` before submitting a PR
- Maintain the architectural invariants documented in `ARCHITECTURE.md`
- Zero clippy warnings (`-D warnings`) is enforced in CI
- Coverage target: ≥80%
