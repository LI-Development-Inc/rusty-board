# setup.md
# rusty-board — Development Setup

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.75+ | `rustup toolchain install stable` |
| Docker | 24+ | https://docs.docker.com/get-docker/ |
| Docker Compose | v2 | included with Docker Desktop |
| sqlx-cli | 0.8+ | `cargo install sqlx-cli --no-default-features --features postgres` |
| cargo-watch | latest | `cargo install cargo-watch` (optional, for hot-reload) |

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/your-org/rusty-board
cd rusty-board

# 2. Copy and configure environment
cp .env.example .env
# Edit .env — at minimum change JWT_SECRET to a random 32+ character string

# 3. Start infrastructure (Postgres, Redis, MinIO)
make infra-up

# 4. Wait for services to be healthy (usually 10–15 seconds)
docker compose ps

# 5. Run database migrations
make migrate

# 6. Build and run the application
make run
# OR for hot-reload on file changes:
make watch

# 7. Verify the application is running
curl http://localhost:8080/healthz
```

## Environment Configuration

See `.env.example` for all available variables. Key ones:

| Variable | Default | Description |
|----------|---------|-------------|
| `APP_HOST` | `0.0.0.0` | Bind address |
| `APP_PORT` | `8080` | Bind port |
| `DATABASE_URL` | `postgres://...` | PostgreSQL connection string |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection string |
| `JWT_SECRET` | *(required)* | HS256 signing secret — minimum 32 chars |
| `S3_ENDPOINT` | `http://localhost:9000` | MinIO/S3 endpoint |
| `MEDIA_BUCKET` | `rusty-board-media` | S3 bucket name |

## Creating the First Admin Account

After starting, create an admin account via the CLI:

```bash
# Currently: insert directly into the database (admin CLI planned for v1.1)
psql "$DATABASE_URL" <<'SQL'
INSERT INTO users (id, username, password_hash, role, is_active)
VALUES (
    gen_random_uuid(),
    'admin',
    -- Generate this hash with: cargo run --bin hash-password -- yourpassword
    '$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>',
    'admin',
    true
);
SQL
```

Then log in at `http://localhost:8080/auth/login`.

## Running Tests

```bash
# All unit tests (no infrastructure required)
make test

# Integration tests (requires infrastructure: make infra-up first)
cargo test -p integration-tests

# Run a specific test
cargo test -p services test_name

# With coverage
make cover
```

## Offline SQLX Mode

rusty-board uses `sqlx` with offline mode for CI. If you add new `sqlx::query!` macros:

```bash
# Regenerate the offline query metadata
cargo sqlx prepare --workspace

# Commit the generated .sqlx/ directory
git add .sqlx/
```

Never add `sqlx::query!` calls without running `cargo sqlx prepare` first — CI will fail.

## Feature Flags

Build with specific features:

```bash
# Default (all production features)
cargo build --features web-axum,db-postgres,auth-jwt,media-local,redis

# With video support (requires libav* system libraries)
cargo build --features web-axum,db-postgres,auth-jwt,media-local,redis,video

# Minimal (for testing domains/services only)
cargo build -p domains -p services
```

See `TECHNICALSPECS.md §9` for the full feature matrix.
