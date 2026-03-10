# `cmd/rusty-board` — Composition Root & Binary

The only place in the codebase where concrete adapter types are selected and wired together. Contains no business logic.

---

## What Lives Here

```
src/
├── main.rs          # Load Settings, init tracing, call compose(), start server,
│                    # graceful shutdown (SIGTERM + ctrl-c), log compiled features
└── composition.rs   # THE ONLY FILE with #[cfg(feature)] branches.
                     # Constructs all concrete types. Returns axum::Router.
```

---

## Architectural Role

`composition.rs` is the single point where compile-time adapter selection becomes runtime values:

```rust
#[cfg(feature = "db-postgres")]
let pool = PgPool::connect(&settings.db_url).await?;
let board_repo = PgBoardRepository::new(pool.clone());

#[cfg(feature = "media-local")]
let media_storage = LocalFsMediaStorage::new(&settings.media_path)?;

#[cfg(feature = "auth-jwt")]
let auth_provider = JwtAuthProvider::new(&settings.jwt_secret, settings.jwt_ttl_secs)?;
```

After `compose()` returns, the binary contains exactly the adapters selected by feature flags — nothing else. All generic types are monomorphized.

---

## Feature Matrix

Select **one** from each category:

| Category | Options | Default |
|----------|---------|---------|
| Web framework | `web-axum` | ✅ `web-axum` |
| Database | `db-postgres`, `db-sqlite` (v1.2) | ✅ `db-postgres` |
| Auth | `auth-jwt`, `auth-cookie` | ✅ `auth-jwt` |
| Media storage | `media-local`, `media-s3` | `media-local` (dev), `media-s3` (prod) |
| Rate limiting | `redis` (or none for `InMemoryRateLimiter`) | ✅ `redis` |
| Media processing | `video`, `documents` (additive) | images only |

---

## Build & Run

```sh
# Development (local media, in-memory rate limiter)
cargo run --features web-axum,db-postgres,auth-jwt,media-local

# Production (S3 media, Redis rate limiter)
cargo run --features web-axum,db-postgres,auth-jwt,media-s3,redis

# With video and document processing
cargo run --features web-axum,db-postgres,auth-jwt,media-s3,redis,video,documents

# Via Makefile
make watch   # cargo-watch hot reload for development
```

---

## Startup Sequence

```
1. Load Settings (environment + .env)
2. Init tracing (JSON in prod, pretty in dev)
3. compose()
   a. Connect to PostgreSQL pool
   b. Connect to Redis (if redis feature)
   c. Configure media storage
   d. Configure auth provider
   e. Construct all repositories
   f. Construct all services (fully monomorphized)
   g. Build router
4. Bind TCP listener
5. Log startup: address, compiled features, version
6. Serve until SIGTERM or ctrl-c
7. Graceful shutdown (drain in-flight requests)
```

`unwrap()` / `expect()` in `composition.rs` and `main.rs` are intentional — startup failures are fatal and should crash immediately with a clear message.

---

## Observability

- **Structured logging**: `tracing` + `tracing-subscriber` (JSON in prod via `RUST_LOG`)
- **Metrics**: `prometheus-client` — scraped from `GET /metrics`
- **Request IDs**: `X-Request-Id` header injected by middleware, propagated through tracing spans

---

## Graceful Shutdown

The server listens for both `SIGTERM` (container orchestration) and `ctrl-c` (development). In-flight requests are allowed to complete before the process exits.

---

## `cmd/seed` — Database Seeding

A companion binary at `cmd/seed/` creates the five canonical test accounts and sample boards for development:

| Account | Password | Role |
|---------|----------|------|
| `admin` | `admin123` | Admin |
| `janitor` | `janitor123` | Janitor |
| `board_owner` | `owner123` | BoardOwner |
| `volunteer` | `vol123` | BoardVolunteer |
| `testuser` | `user123` | User |

Also creates boards `/b/`, `/tech/`, `/pol/`, `/art/`, `/mu/` with sample threads and posts.

Run: `make seed` (requires `make watch` + `make migrate` to have completed first).
