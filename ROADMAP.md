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
| `domains/src/models.rs` | `CaptchaVerifier` port not yet wired (schema field exists) | v1.1.1 |
| `domains/src/models.rs` | ~~`SearchIndex` port not yet wired~~ | ✅ v1.2 — HTML front-end + route |
| `domains/src/models.rs` | ~~Archive adapter not yet wired~~ | ✅ v1.2 — `ArchiveRepository` + `PgArchiveRepository` |
| `domains/src/models.rs` | `FederationSync` port not yet wired (schema field exists) | v2.0 |
| `services/src/common/tripcode.rs` | ~~Super tripcode `###` returns `!!!STUB`~~ | ✅ HMAC-SHA256 implemented |

---

## v1.1 — Security Hardening & Quality of Life

**Status**: ✅ Complete. Build clean. All unit and integration tests passing.

### Delivered

**Tripcodes & Capcodes**
- `#password` — insecure tripcode: `SHA-256(password)[0..5]` displayed as `!{10hex}`
- `##password` — secure tripcode: `SHA-256(pepper || "::" || password)[0..5]` displayed as `!!{10hex}`
- `###password` — super tripcode: HMAC-SHA256(key=pepper, msg="###"||password) → `!!!{10hex}`
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

**UX & Thread View Fixes** (delivered alongside v1.1)
- Cross-board `>>>/{slug}/{N}` links resolve via `GET /board/{slug}/post/{N}` server-side redirect (`PostRepository::find_thread_id_by_post_number`)
- Thread view shows all posts up to bump limit (500) without pagination — `PostRepository::find_all_by_thread`
- (You) post tracking: reply form sends `Accept: application/json`; server returns `{post_number}` in 201 JSON; stored in `localStorage`; shown as green `(You)` badge and `>>{N} (You)` on quote links
- Click-to-quote binds directly to each `No.{N}` anchor — no event delegation; hover-preview popup clicks are guarded
- Shared reply form: one `<form id="shared-reply-form">` element physically moves between the top position and the Quick Reply draggable box — zero sync needed
- Quote insertion appends `>>{N}` at the end of the textarea on a new line; duplicate guard prevents double-insert
- Post timestamps: user-selectable format (relative / MM/DD/YY HH:MM:SS / ISO 8601) in the Settings panel; shared `window.rbApplyTimeFormat` runs on every page with `time.post-date[data-ts]` elements; relative mode refreshes every 60 s
- Auto-update with exponential back-off (10 s → 5 min cap); pauses automatically if any reply form has unsaved content; moved to thread bottom nav
- Images rendered in overboard view (bulk attachment fetch via `PostRepository::find_attachments_by_post_ids`)
- OP images displayed on board index thread list (thumbnail from `ThreadSummary::thumbnail_key`)
- Unified post header across thread, overboard, and board index: `Name · Tripcode · (You) · time · ID · IP(10) · No.N`; tripcode colour-coded by security level (`insecure`=amber, `secure`=blue, `super`=orange-red)
- Catalog view truncates OP body to 200 characters
- `ThreadSummary` enriched with OP post fields (`op_name`, `op_tripcode`, `op_created_at`, `op_post_number`, `op_ip_hash`) so the board index can render a full post header without an extra query
- User dashboard displays real `joined_at` date from `User::created_at`
- Board-owner dashboard lists actual volunteers for owned boards
- Staff request approve/deny handlers (`POST /admin/requests/{id}/approve|deny`) wired; board owners can approve volunteer requests via the same endpoint (service-level permission check)
- `DefaultBodyLimit::max(12 MB)` added globally so image uploads up to the 10 MB board limit work correctly
- IP hash in mod view truncated to first 10 characters (full hash preserved in `data-ip-hash` for ban operations)

**Post Body Formatting** (client-side, `window.rbFormatPostBody` in `base.html` — runs on every page)
- Line-scoped: `>greentext`, `<pinktext`, `==REDTEXT==`, `(((bluetext)))`
- Inline: `` `code` ``, `**bold**`, `__underline__`, `~~strike~~`
- Multi-line: `` ``` ``fenced code block`` ``` `` → `<pre class="code-block">`, `[spoiler]…[/spoiler]` → hover-reveal span
- All formatting also applies to overboard post bodies (shared via `base.html`)

**Theme System**
- Three themes: Futaba (default), Yotsuba B, Dark — selectable in the Settings panel
- Dark theme: complete CSS rewrite using `--color-*` variable overrides so `style.css` rules cascade correctly; all post elements, tripcodes, settings panel, mod toolbar styled
- Settings panel appears as fixed popup (top-right) on all themes; contains Theme and Timestamp Format selectors on thread pages

### v1.1 Open Items

| Item | Description | Target |
|------|-------------|--------|
| Super tripcode `###` | ✅ HMAC-SHA256 implemented; ed25519 proof-of-identity upgrade still possible via `TripkeyRepository` | v1.2 done |
| `CaptchaVerifier` wiring | Port not yet connected in `PostService` even though `captcha_required` schema field exists | v1.1.1 |
| CSP / inline scripts | Templates contain inline `<script>` blocks; extract to `/static/js/` + nonce CSP | v1.1.1 |
| Thread cycle mode | ✅ Completed v1.2 — migration 014, `set_cycle`, `find_oldest_unpinned_reply`, cycle pruning in `PostService` |
| Thread pin-in-cycle | ✅ Completed v1.2 — `set_pinned`, `[PIN+/-]` mod button on reply posts |

### Completed in v1.1.1

| Item | Resolution |
|------|------------|
| Staff message nav badge | ✅ `GET /staff/messages/unread` + nav badge in `base.html` |
| `auth-tripcode` feature gate | ✅ `#[cfg(feature = "auth-tripcode")]` on `parse_name_field`; enabled by default in all dependent crates |
| Login brute-force protection | ✅ `LoginGuard` (`middleware/login_guard.rs`) — 5 failures → 10-min lockout per username; injected as `axum::Extension` |
| `X-Request-Id` middleware | ✅ Already present — `SetRequestIdLayer` + `PropagateRequestIdLayer` in `composition.rs` |
---

## v1.2 — Search, Deduplication & Adapter Expansion

**Focus**: Pluggable full-text search. File deduplication. First alternative database adapter. First alternative media storage adapter. Super tripcode ed25519.

**Status**: ✅ Complete. Build clean.

### Completed in v1.2 Session

| Item | Notes |
|------|-------|
| Thread Cycle Mode | Migration 014; `Thread::cycle`; `Post::pinned`; `toggle_cycle` + `set_post_pinned` handlers; `[CY]`/`[PIN]` mod toolbar |
| File Deduplication | `find_attachment_by_hash` on `PostRepository`; SHA-256 index; dedup check in `PostService::create_post` |
| Super Tripcode `###` | HMAC-SHA256(key=pepper, msg="###"\|\|password) — server-bound, no `!!!STUB` |
| `DnsblChecker` port | Defined in `domains::ports`; `SpamhausDnsblChecker` adapter implementation next |

### Planned

**Super Tripcodes** ✅ — HMAC-SHA256 implementation (v1.2 session)
- `###password` now computes `HMAC-SHA256(key=pepper, msg="###"||password)` → `!!!{10hex}`
- Server-bound: identity is tied to the pepper secret; cannot be precomputed without it
- The full ed25519 challenge-response flow (proof-of-identity even against a compromised server) remains a future upgrade path via `TripkeyRepository` port

**Thread Cycle Mode** ✅ — implemented in v1.2 session
- Migration 014: `cycle BOOLEAN DEFAULT FALSE` on `threads`, `pinned BOOLEAN DEFAULT FALSE` on `posts`
- `ThreadRepository::set_cycle`, `PostRepository::set_pinned`, `find_oldest_unpinned_reply`, `delete_by_id`
- `PostService::create_post` prunes oldest unpinned reply when `cycle=true && past_bump_limit`
- `toggle_cycle` (`POST /mod/threads/:id/cycle`) and `set_post_pinned` (`POST /mod/posts/:id/pin`) handlers
- Thread toolbar: `[CL+/-]` close, `[CY+/-]` cycle, `[PIN+/-]` per-post pin (reply posts only)
- `AuditAction::CycleThread` and `AuditAction::PinPost` audit trail

**Pluggable Search** ✅
- HTML front-end: `search_results.html` template, `GET /boards/:slug/search` renders paginated post results
- Search form in board nav when `search_enabled = true`; gated by `403` when disabled

**File Deduplication** ✅ — implemented in v1.2 session
- `PostRepository::find_attachment_by_hash` added (uses migration 014 index on `attachments.hash`)
- `PostService::create_post` checks hash before uploading; reuses existing `media_key` + `thumbnail_key`
- No re-upload overhead for identical images posted across threads or boards

~~**SQLite Backend**~~ — pushed to v2.0 (same scope as SurrealDB)

**Alternative Media Storage**
- `R2MediaStorage` (`media-r2`) — Cloudflare R2
- `BackblazeMediaStorage` (`media-backblaze`) — Backblaze B2

**DNSBL Checking** ✅
- `DnsblChecker` port; `SpamhausDnsblChecker` (reverse-IP DNS vs `zen.spamhaus.org`); fail-open
- Step 1b in `PostService::create_post`; gated by `spam_filter_enabled`; injected via `with_dnsbl()`

**`max_threads` in BoardConfig** ✅ — migration 016; `BoardConfigUpdate` DTO; dashboard config form

**Archive** ✅
- Migration 015: `archived_threads` table
- `ArchiveRepository` port + `PgArchiveRepository`; `find_oldest_for_archive` on `ThreadRepository`
- `ThreadService::with_archive()` + `prune_with_archive(archive_enabled)` archives before hard-delete
- `PostService::with_archive_repo()` — board-capacity prune at Step 11c
- `GET /board/:slug/archive` — read-only archived thread list; `[Archive]` link in board nav

---

## v1.3 — Modern UX, Advanced Auth & Alternative Storage

**Focus**: Live updates. Client-side quality of life. Two-factor authentication. Alternative media backends.

**WebSocket Live Updates** (`web-websockets`) — new posts without reload; non-JS clients unaffected

**Thread Watcher / Quick Reply / Post Hiding** — client-side localStorage features

**Two-Factor Authentication** (`auth-2fa`) — TOTP for moderator/admin accounts via `TwoFactorProvider` port

**i18n / Localization** — template string extraction; English base; community translations

**Alternative Media Storage**
- `R2MediaStorage` (`media-r2`) — Cloudflare R2
- `BackblazeMediaStorage` (`media-backblaze`) — Backblaze B2

---

## v2.0 — Extensibility & Federation

**Focus**: ActivityPub federation. Alternative web framework. Advanced anti-spam.

**ActivityPub Federation** (`federation-activitypub`) — `FederationSync` port; boards opt in via `board_config.federation_enabled`

**Alternative Web Framework** (`web-actix`) — all integration tests must pass under both `web-axum` and `web-actix`

**SQLite Backend** (`db-sqlite`) — all repository ports; embedded/serverless deployments

**SurrealDB Backend** (`db-surrealdb`) — full repository port set; third database option

**ML Spam Scoring** — `SpamScorer` port with local ONNX model inference adapter

---

## Adapter Expansion Summary

| Port | v1.0 | v1.1 | v1.2 | v1.3 | v2.0 |
|------|------|------|------|------|------|
| `BoardRepository` | Postgres ✅ | — | — | — | **SurrealDB** |
| `ThreadRepository` | Postgres ✅ | — | — | — | — |
| `PostRepository` | Postgres ✅ | `find_all_by_thread`, `find_thread_id_by_post_number`, `search_fulltext` ✅ | — | — | — |
| `BanRepository` | Postgres ✅ | — | — | — | — |
| `FlagRepository` | Postgres ✅ | — | — | — | — |
| `AuditRepository` | Postgres ✅ | ✅ find_all/find_by_board | — | — | — |
| `UserRepository` | Postgres ✅ | — | — | — | — |
| `StaffRequestRepository` | Noop | ✅ Postgres | — | — | — |
| `StaffMessageRepository` | — | ✅ Postgres | — | — | — |
| `SessionRepository` | — | ✅ Postgres (auth-cookie) | — | — | — |
| `MediaStorage` | S3 ✅, LocalFs ✅ | — | **R2, Backblaze** | — | **IPFS** |
| `MediaProcessor` | Image ✅ (+Video stub, +Docs stub) | — | — | — | — |
| `AuthProvider` | JWT ✅ | ✅ Cookie | — | — | **OIDC** |
| `RateLimiter` | Redis ✅, Noop | ✅ InMemory | — | — | — |
| `CaptchaVerifier` | *(schema ready)* | *(v1.1.1)* | — | — | — |
| `SearchIndex` | *(schema ready)* | ✅ Postgres FTS | ✅ HTML view + search form | — | — |
| `DnsblChecker` | *(planned)* | — | ✅ SpamhausDnsblChecker | — | — |
| `TripkeyRepository` | — | *(stub)* | **Postgres** | — | — |
| `ArchiveRepository` | — | — | ✅ PgArchiveRepository | — | — |
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
