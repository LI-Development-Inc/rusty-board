# rusty-board — Development Roadmap

> **Public-facing view of planned development direction. Timelines are approximate and depend on contributor availability.**
> **For internal implementation details see `docs/PROJECTPLAN.md`. For port definitions see `docs/PORTS.md`.**

The roadmap is organized around a core principle: **v1.0 establishes the stable foundation; all subsequent versions validate and expand the adapter ecosystem without touching the core.** Every post-v1.0 release completes at least one real adapter swap to prove the architecture works in practice, not just in theory.

---

## v1.0 — Core Functional Imageboard

**Status**: ✅ Complete. Build clean. All integration tests passing.

v1.0 is the minimum viable imageboard: fully anonymous posting, media uploads, moderation tools, and operator-controlled board configuration. The foundation that all future versions build on.

### Posting & Content

- Anonymous posting by default — no account required to post (invariant of the system)
- Threads and replies with bump order, bump limits, and automatic thread pruning
- Sage (no-bump replies)
- Greentext (`>`) and quote links (`>>PostId`)
- Multiple file attachments per post (up to `board_config.max_files`)
- Image thumbnail generation (320px PNG, EXIF stripped unconditionally)
- Video thumbnails via `ffmpeg-next` — optional `video` feature *(stub, see v1.0 stubs)*
- PDF thumbnails via `pdfium-render` — optional `documents` feature, experimental *(stub)*
- Spoiler attachments

### Views

- Board thread list (paginated, bump order, sticky-first)
- Catalog view (grid layout, all active threads)
- Thread view (all posts with attachments inline)
- Board list and Overboard (cross-board recent posts)
- Fully functional without JavaScript

### Moderation & Roles

Five roles, each with a dedicated dashboard:

| Role | Scope | Dashboard |
|------|-------|-----------|
| `admin` | Full site control | `/admin/dashboard` |
| `janitor` | Global moderation, all boards | `/janitor/dashboard` |
| `board_owner` | Config + volunteers for owned boards | `/board-owner/dashboard` |
| `board_volunteer` | Post deletion + bans on assigned boards | `/volunteer/dashboard` |
| `user` | Registered tier; can submit staff requests | `/auth/login` (no dashboard) |

Single unified dashboard template — `role_label` drives the heading. Posting remains 100% anonymous regardless of account status.

Moderation actions: delete post/thread, sticky/close threads, IP bans with expiry, flag/report queue with approve/reject workflow, full paginated audit log (site-wide and per-board views).

### Anti-Spam

- Per-IP, per-board rate limiting — Redis-backed (multi-instance) or InMemory (single-instance)
- Heuristic spam scoring: body length, link density, duplicate content detection
- Link blacklist: configurable per-board domain rejection list
- Name rate limiting: same-name posting frequency throttle
- All parameters tunable per board via `BoardConfig` — no recompile required

### Infrastructure

- PostgreSQL 16, Axum 0.8, Redis 7, JWT auth, argon2id (OWASP 2024 params)
- S3/MinIO media storage and local filesystem option
- Prometheus metrics, structured JSON logging (`tracing-subscriber`), health check endpoint
- Docker + docker-compose deployment, graceful shutdown (SIGTERM)
- 13 clean reversible migrations (001–013), all with `.down.sql`
- `make seed` creates five accounts (one per role) + five boards + realistic post content

### Feature Flags

| Flag | Purpose | Default |
|------|---------|---------| 
| `web-axum` | Axum HTTP framework | ✅ |
| `db-postgres` | PostgreSQL storage | ✅ |
| `auth-jwt` | JWT bearer authentication | ✅ |
| `media-s3` | S3/MinIO media storage | ✅ |
| `media-local` | Local filesystem media storage | optional |
| `redis` | Redis rate limiter | ✅ |
| `video` | Video thumbnail generation | optional |
| `documents` | PDF thumbnail generation | optional, experimental |

### Known Stubs (v1.0)

| Location | Description | Target |
|----------|-------------|--------|
| `storage-adapters/src/media/videos.rs` | `VideoMediaProcessor` — ffmpeg-next keyframe extraction | v1.0 patch |
| `storage-adapters/src/media/documents.rs` | `DocumentMediaProcessor` — pdfium-render first page | v1.0 patch |
| `api-adapters/.../board_owner_handlers.rs` | Staff list not yet loaded via `UserRepository` | v1.1.1 |
| `api-adapters/.../user_handlers.rs` | `joined_at` not yet surfaced from `User` model | v1.1.1 |
| `domains/src/models.rs` | `CaptchaVerifier` port not yet wired (schema field exists) | v1.1.1 |
| `domains/src/models.rs` | `SearchIndex` port not yet wired (schema field exists) | v1.2 |
| `domains/src/models.rs` | Archive adapter not yet wired (schema field exists) | v1.2 |
| `domains/src/models.rs` | `FederationSync` port not yet wired (schema field exists) | v2.0 |

---

## v1.1 — Security Hardening & Quality of Life

**Status**: ✅ Complete. Build clean. All unit and integration tests passing.

### Delivered

**Tripcodes & Capcodes**
- `#password` — insecure tripcode: `SHA-256(password)[0..5]` displayed as `!{10hex}`
- `##password` — secure tripcode: `SHA-256(pepper || "::" || password)[0..5]` displayed as `!!{10hex}`
- `###password` — super tripcode: currently a stub (`!!!STUB`); full ed25519 impl in v1.2
- `### Role` — capcode: verifies poster's server-side role, displayed as `!!!! {Role}`
- Five capcode CSS variants with dark-mode overrides (admin, janitor, board-owner, volunteer, developer)
- `tripcode_pepper` config key in `Settings` for signing key

**Cookie Session Auth** (`auth-cookie` feature)
- `CookieAuthProvider` implementing the `AuthProvider` port — first real port swap
- Sessions backed by `user_sessions` table (migration 013)
- Enables immediate token revocation (impossible with stateless JWT)

**In-Memory Rate Limiter**
- `InMemoryRateLimiter` implementing the `RateLimiter` port
- Sliding window, no Redis dependency — appropriate for single-instance / hobby deployments

**Staff Messaging**
- `StaffMessageRepository` port + `PgStaffMessageRepository`
- `StaffMessageService`: send, inbox, unread_count, mark_read, purge_expired
- Routes: `GET /staff/messages`, `POST /staff/messages`, `POST /staff/messages/{id}/read`, `POST /staff/messages/purge`

**Staff Request Pipeline**
- `StaffRequestRepository` port + `PgStaffRequestRepository` (replaces Noop adapter)
- Approval guarded by `WHERE status = 'pending'` to prevent double-review races

**Audit Log UI**
- `AuditRepository::find_all` and `find_by_board` on port + Pg adapter
- Role-scoped paginated views: `/janitor/logs`, `/board-owner/logs`, `/volunteer/logs`

**Basic Search**
- `PostRepository::search_fulltext` using PostgreSQL GIN index on `posts.body`
- `GET /boards/{slug}/search` gated by `board_config.search_enabled`

**Self-Registration**
- `POST /auth/register` creates `Role::User` accounts (gated by `open_registration` setting)
- `GET /auth/register` renders registration page
- `testuser / user123` account seeded as the canonical `user` role example

### v1.1 Open Items

| Item | Description | Target |
|------|-------------|--------|
| Super tripcode `###` | Stub returns `!!!STUB`; full ed25519 needs `TripkeyRepository` port and two-step post flow | v1.2 |
| `CaptchaVerifier` wiring | Port not yet connected in `PostService` even though `captcha_required` schema field exists | v1.1.1 |
| Staff message badge | `unread_count` not yet in `DashboardTemplate` nav | v1.1.1 |
| CSP / inline scripts | Templates contain inline JS; extract to `/static/js/` + nonce loading for strict CSP | v1.1.1 |
| `auth-tripcode` gate | `parse_name_field` is always-on; add `#[cfg(feature = "auth-tripcode")]` if optional gating desired | v1.1.1 |
| Thread cycle mode | `[C]` button in mod toolbar is wired but `cycle` DB column and oldest-post pruning logic are not yet implemented | v1.2 |
| Thread pin-in-cycle | `[Pin]` button for cycle threads — pins a post so it is never pruned; requires `pinned` column on `posts` | v1.2 |
| (You) post-number capture | Reply success response should return `X-Post-Number` header so the number is stored immediately rather than after reload | v1.1.1 |

---

## v1.2 — Search, Deduplication & Adapter Expansion

**Focus**: Pluggable full-text search. File deduplication. First alternative database adapter. First alternative media storage adapter. Super tripcode ed25519.

### Planned

**Super Tripcodes** — `TripkeyRepository` port + ed25519 two-step post flow

**Thread Cycle Mode** — `[C]` mod action + `[Pin]` (deferred from v1.1)
- Add `cycle BOOLEAN NOT NULL DEFAULT FALSE` column to `threads` (migration 014)
- Add `pinned BOOLEAN NOT NULL DEFAULT FALSE` column to `posts` (migration 014)
- `ThreadRepository` gains `set_cycle(id, bool)` port method
- `PostRepository` gains `set_pinned(id, bool)` port method
- `PostService::create_post` prunes oldest unpinned post when `cycle=true` and reply count ≥ bump limit
- `toggle_cycle` and `pin_post` mod handlers + routes
- Thread template: `[C+/-]` calls new `/mod/threads/:id/cycle` endpoint; `[Pin]` calls `/mod/posts/:id/pin`

**Pluggable Search** (`search-meilisearch` / `search-postgres-fts`)
- `SearchIndex` port implemented with `MeiliSearchIndex` and `PgFullTextIndex` adapters

**File Deduplication**
- SHA-256 already in `attachments`; `MediaStorage` updated with hash-lookup
- Identical files share one stored copy

**SQLite Backend** (`db-sqlite`)
- All repository ports implemented — first real `*Repository` port swap
- Contract tests must pass for SQLite exactly as they do for Postgres

**Alternative Media Storage**
- `R2MediaStorage` (`media-r2`) — Cloudflare R2
- `BackblazeMediaStorage` (`media-backblaze`) — Backblaze B2

**DNSBL Checking** (`spam-dnsbl`)
- `DnsblChecker` port; `SpamhausDnsblChecker` adapter; fail-open on lookup failure

**Archive**
- `board_config.archive_enabled` schema field already present
- Pruned threads moved to read-only archive store instead of deleted

---

## v1.3 — Modern UX & Advanced Auth

**Focus**: Live updates. Client-side quality of life. Two-factor authentication.

**WebSocket Live Updates** (`web-websockets`) — new posts without reload; non-JS clients unaffected

**Thread Watcher / Quick Reply / Post Hiding** — client-side localStorage features

**Two-Factor Authentication** (`auth-2fa`) — TOTP for moderator/admin accounts via `TwoFactorProvider` port

**i18n / Localization** — template string extraction; English base; community translations

---

## v2.0 — Extensibility & Federation

**Focus**: ActivityPub federation. Alternative web framework. Advanced anti-spam.

**ActivityPub Federation** (`federation-activitypub`) — `FederationSync` port; boards opt in via `board_config.federation_enabled`

**Alternative Web Framework** (`web-actix`) — all integration tests must pass under both `web-axum` and `web-actix`

**SurrealDB Backend** (`db-surrealdb`) — full repository port set; third database option

**ML Spam Scoring** — `SpamScorer` port with local ONNX model inference adapter

---

## Adapter Expansion Summary

| Port | v1.0 | v1.1 | v1.2 | v1.3 | v2.0 |
|------|------|------|------|------|------|
| `BoardRepository` | Postgres ✅ | — | **SQLite** | — | **SurrealDB** |
| `ThreadRepository` | Postgres ✅ | — | **SQLite** | — | — |
| `PostRepository` | Postgres ✅ | — | **SQLite** | — | — |
| `BanRepository` | Postgres ✅ | — | **SQLite** | — | — |
| `FlagRepository` | Postgres ✅ | — | **SQLite** | — | — |
| `AuditRepository` | Postgres ✅ | ✅ find_all/find_by_board | **SQLite** | — | — |
| `UserRepository` | Postgres ✅ | — | **SQLite** | — | — |
| `StaffRequestRepository` | Noop | ✅ Postgres | — | — | — |
| `StaffMessageRepository` | — | ✅ Postgres | — | — | — |
| `SessionRepository` | — | ✅ Postgres (auth-cookie) | — | — | — |
| `MediaStorage` | S3 ✅, LocalFs ✅ | — | **R2, Backblaze** | — | **IPFS** |
| `MediaProcessor` | Image ✅ (+Video stub, +Docs stub) | — | — | — | — |
| `AuthProvider` | JWT ✅ | ✅ Cookie | — | — | **OIDC** |
| `RateLimiter` | Redis ✅, Noop | ✅ InMemory | — | — | — |
| `CaptchaVerifier` | *(schema ready)* | *(v1.1.1)* | — | — | — |
| `SearchIndex` | *(schema ready)* | ✅ Postgres FTS (basic) | **MeiliSearch, PgFTS** | — | — |
| `DnsblChecker` | *(planned)* | — | **Spamhaus** | — | — |
| `TripkeyRepository` | — | *(stub)* | **Postgres** | — | — |
| `FederationSync` | *(planned)* | — | — | — | **ActivityPub** |

---

## Contribution Priorities

1. **Tests and coverage** — unit tests for uncovered service branches, adapter contract tests
2. **Documentation** — setup guides, deployment examples, operator docs
3. **Bug reports** — especially from real deployment scenarios
4. **New adapter implementations** — following the established patterns
5. **Alternative theme CSS** — imageboard aesthetics are subjective and community-driven
6. **Performance improvements** — with benchmark evidence via `criterion`

See `docs/contributing.md` for the contribution process and coding standards.
