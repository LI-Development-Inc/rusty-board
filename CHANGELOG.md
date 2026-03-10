# Changelog

All notable changes to rusty-board are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Version numbers follow [Semantic Versioning](https://semver.org/).

---

## [1.0.0] — 2026-02-25

### Added

**Core architecture**
- Hexagonal (ports-and-adapters) architecture with a single composition root at `cmd/rusty-board/src/composition.rs`
- Compile-time adapter selection via Cargo feature flags — `web-axum`, `db-postgres`, `auth-jwt`, `media-local`, `media-s3`, `redis`, `video`, `documents`
- `domains` crate: all domain models (`Board`, `Thread`, `Post`, `Attachment`, `Ban`, `Flag`, `User`, `AuditEntry`), port traits, and `BoardConfig`
- `services` crate: `BoardService`, `ThreadService`, `PostService`, `ModerationService`, `UserService` — pure business logic with zero adapter imports
- `storage-adapters` crate: PostgreSQL repositories (sqlx), Redis rate limiter, local-filesystem media storage, S3/MinIO media storage
- `auth-adapters` crate: JWT HS256 token issuance and verification, Argon2id password hashing
- `api-adapters` crate: Axum HTTP handlers, middleware, routes, Askama HTML templates
- `configs` crate: `Settings` struct loaded from environment variables at startup

**HTTP API (all routes, see TECHNICALSPECS.md §5)**
- Public: board list, board view, catalog, thread view, post creation (multipart), overboard
- Auth: `POST /auth/login`, `POST /auth/refresh`
- Board owner dashboard: `GET /board/:slug/dashboard`, `GET/PATCH /board/:slug/config`
- Moderation: flag queue, flag resolution, post/thread delete, sticky/close toggle, ban management
- Admin: board CRUD, user management
- Health check: `GET /healthz`

**Post creation pipeline**
- Multipart form data parsing (body + up to N file attachments)
- Ban check (always runs; not configurable)
- Rate limit check (configurable via `BoardConfig.rate_limit_enabled`)
- Post body validation (length, spam score)
- Duplicate content detection (configurable)
- MIME-type allowlist enforcement
- File size limit enforcement
- EXIF stripping (unconditional)
- Thumbnail generation
- S3/local media storage
- Quote extraction and cross-linking (`>>postId`)
- Thread bumping (sage, bump limit, allow_sage config)
- Thread pruning when board exceeds `max_threads`

**Moderation**
- Flag creation (visitor-initiated), flag queue, flag resolution (approved/rejected)
- Post and thread deletion
- Thread sticky / closed toggles
- IP ban issuance (permanent or time-limited)
- Ban expiration
- Audit log (every moderation action recorded; write failures silently logged, never propagated)

**Security**
- Argon2id password hashing (OWASP recommended parameters: m=19456, t=2, p=1)
- JWT HS256 tokens; stored in `secrecy::Secret<String>`, never logged
- Raw IP addresses never stored — SHA-256(ip + UTC date salt) only
- EXIF stripping unconditional on all uploaded images
- CSP, X-Frame-Options, X-Content-Type-Options response headers
- Role-based access control: `admin > janitor > board_owner > board_volunteer > anonymous`

**Ops (Phase 11)**
- Multi-stage Dockerfile (builder + slim runtime); `HEALTHCHECK` via `curl /healthz`
- `docker-compose.yml`: postgres, redis, minio, minio-setup, app — all with health checks
- `scripts/backup.sh`: timestamped pg_dump + media tar archive; auto-prunes backups older than 30 days
- `scripts/restore.sh`: interactive restore with safety confirmation; handles DB drop/recreate + media extraction
- `scripts/migrate.sh`: wrapper around `sqlx migrate` for pre-deploy migration runs
- GitHub Actions CI matrix: fmt, clippy, tests across 4 feature combinations, security audit, Docker build

**Testing**
- `crates/integration-tests`: 1 000+ lines of mock-based unit tests
  - `domain_models.rs`: model invariants, `BoardConfig` defaults, `CurrentUser` permission matrix
  - `utils.rs`: IP hashing, content hashing, quote parsing, spam scoring, pagination
  - `board_service.rs`: board CRUD happy paths and error paths
  - `thread_service.rs`: thread creation, sticky, closed, pruning
  - `post_service.rs`: 14 tests covering every `BoardConfig` boolean branch (spam filter, rate limit, sage, bump limit, forced anon, banned IP)
  - `port_contracts.rs`: compile-time trait contract verification for all 7 repository ports
- In-crate service unit tests (`post/mod.rs`, `moderation/mod.rs`) covering error paths and all `BoardConfig` branches
- HTTP integration tests in `api-adapters/tests/board_handlers.rs` using `tower::ServiceExt::oneshot`
- `domains` `testing` feature flag exposes `MockXxx` types to external test crates via `mockall::automock`

**Static assets**
- `static/css/style.css`: retro imageboard aesthetic (beige/brown palette), responsive layout, post form, thread list, catalog grid, pagination, admin tables
- `static/js/app.js`: progressive enhancement — quote highlighting, image expand-in-place, reply form prefill, textarea auto-resize, spoiler reveal

### Changed

- `BoardConfig` is the single source of truth for all per-board behaviour (bump limit, rate limit, spam filter, NSFW flag, file type allowlist, forced anon, etc.)
- All port traits use native RPITIT async (`async fn` in traits, Rust 1.75+); no `#[async_trait]` macro required

### Security

- All secrets are environment variables only; `secrecy::Secret<String>` wrapper prevents accidental debug logging
- `cargo audit` integration in CI; zero high/critical advisories at release

---

## [Unreleased]

_Nothing yet._

---

[1.0.0]: https://github.com/your-org/rusty-board/releases/tag/v1.0.0
