# REQUIREMENTS.md
# rusty-board — Requirements

> **The authoritative statement of what the system must do (functional) and how it must do it (non-functional). All development work is bounded by this document. Scope disputes are resolved here first.**

Requirements are grouped by concern. Each requirement has a unique identifier for traceability. Status: **v1.0** means in scope for the initial release. **v1.1**, **v1.2**, **v1.3**, **v2.0** means deferred to the indicated milestone. Anything not listed is out of scope until explicitly added.

---

## 1. Posting & Content

### Anonymous Posting

**REQ-POST-001** (v1.0): Any visitor can create a post without an account, login, or registration of any kind. Anonymous posting is the default and primary mode of operation.

**REQ-POST-002** (v1.0): A post consists of an optional body text and zero or more file attachments. At least one of body or attachment must be present — empty posts are rejected.

**REQ-POST-003** (v1.0): Posters may optionally provide a display name. If `board_config.forced_anon` is true, the name field is ignored and all posts display as "Anonymous".

**REQ-POST-004** (v1.0): The `email` field accepts the value "sage". A post with `email=sage` does not bump its thread. All other email values are discarded and not stored.

**REQ-POST-005** (v1.1): When `board_config.allow_tripcodes` is true, a poster may include a tripcode in their name field using the `name##password` syntax. The tripcode is displayed as a short hash derived from the password. It is not authentication — it is identity signaling. The name portion is still subject to `forced_anon`.

**REQ-POST-006** (v1.0): Post body length is limited to `board_config.max_post_length` characters (default 4000). Exceeding this limit returns a validation error.

**REQ-POST-007** (v1.0): Greentext: lines beginning with `>` are rendered with a distinct style.

**REQ-POST-008** (v1.0): Quote links: `>>PostId` syntax in post body is rendered as a clickable link to the referenced post within the same thread. Invalid or cross-thread quote references are rendered as plain text.

### Threads

**REQ-THREAD-001** (v1.0): A thread is created by making a post with no `thread_id`. The first post in a thread is the OP (original post). A thread requires an attachment on the OP post (board-wide rule, not a `BoardConfig` toggle).

**REQ-THREAD-002** (v1.0): Replies are made by posting with a valid `thread_id`. Replies do not require an attachment unless `board_config.max_files` is set to require one (future toggle).

**REQ-THREAD-003** (v1.0): Each reply (unless saged) bumps the thread: sets `bumped_at` to the current time and increments `reply_count`.

**REQ-THREAD-004** (v1.0): Once a thread's `reply_count` reaches `board_config.bump_limit`, further replies are accepted but no longer bump the thread.

**REQ-THREAD-005** (v1.0): Closed threads (`closed = true`) reject new replies with a clear error message.

**REQ-THREAD-006** (v1.0): After any successful post creation, the system checks whether the board's thread count exceeds `board_config.bump_limit` and prunes the oldest non-sticky threads to bring the count within limits. This prune happens synchronously within the post creation request.

### File Attachments

**REQ-FILE-001** (v1.0): Each post may include up to `board_config.max_files` file attachments (default 4).

**REQ-FILE-002** (v1.0): Each file must not exceed `board_config.max_file_size` in size (default 10MB).

**REQ-FILE-003** (v1.0): Each file's MIME type must appear in `board_config.allowed_mimes`. Files with disallowed MIME types are rejected before any processing occurs.

**REQ-FILE-004** (v1.0): EXIF metadata is stripped from all uploaded images unconditionally. This is not a board-level toggle.

**REQ-FILE-005** (v1.0): A thumbnail is generated for every accepted attachment. Image thumbnails are 320px wide PNGs. Video and document thumbnails are feature-dependent (see REQ-FILE-006, REQ-FILE-007).

**REQ-FILE-006** (`video` feature): Video files (MIME: `video/mp4`, `video/webm`) produce a thumbnail extracted from the first keyframe.

**REQ-FILE-007** (`documents` feature): PDF files (MIME: `application/pdf`) produce a thumbnail rendered from the first page.

**REQ-FILE-008** (v1.0): Attachments may be marked as spoilers by the poster. Spoiler thumbnails are hidden by default in the UI and revealed on click.

---

## 2. Views & Navigation

**REQ-VIEW-001** (v1.0): Board index (`/board/:slug`): paginated list of threads sorted by `bumped_at` DESC, sticky threads first. Shows OP post, thumbnail, reply count, and the N most recent reply previews.

**REQ-VIEW-002** (v1.0): Catalog view (`/board/:slug/catalog`): grid of all threads showing OP thumbnail, subject (first line of body), and reply count. No post body previews. Useful for quickly scanning a board's active discussions.

**REQ-VIEW-003** (v1.0): Thread view (`/board/:slug/thread/:id`): all posts in chronological order with attachments, quote links rendered, reply form at the bottom.

**REQ-VIEW-004** (v1.0): Board list (`/boards`): list of all boards with slug, title, and post count.

**REQ-VIEW-005** (v1.0): Overboard (`/overboard`): recent posts across all boards, paginated by creation time. Shows board, thread context, post body preview, and thumbnail if any.

**REQ-VIEW-006** (v1.0): Pagination: all paginated views provide previous/next navigation and page number indicators. Page size is fixed per view type (configurable in Settings, not BoardConfig).

**REQ-VIEW-007** (v1.0): All views render correctly without JavaScript. JavaScript adds lazy image loading, inline reply form toggling, and quote-click post highlighting — all additive.

**REQ-VIEW-008** (v1.0): JSON API: every view that renders HTML has a parallel JSON endpoint returning equivalent structured data. Path convention: same path with `Accept: application/json` header, or `/api/` prefix (TBD before Phase 7).

---

## 3. Moderation

### Reports / Flags

**REQ-MOD-001** (v1.0): Any visitor can report a post by submitting a reason. The report is stored as a `Flag` record with the reporter's IP hash. No account required.

**REQ-MOD-002** (v1.0): Flags appear in the moderator queue (`/mod/flags`) sorted by creation time. Moderators can approve (confirming the post violates rules) or reject (dismissing the report) each flag.

**REQ-MOD-003** (v1.0): A post may accumulate multiple flags. The moderator queue shows the count.

### Moderation Actions

**REQ-MOD-004** (v1.0): Moderators can delete any individual post on boards they are assigned to. Deleting the OP post of a thread deletes the entire thread.

**REQ-MOD-005** (v1.0): Moderators can delete an entire thread directly.

**REQ-MOD-006** (v1.0): Moderators can sticky a thread (it appears at the top of the board regardless of bump order) and un-sticky it.

**REQ-MOD-007** (v1.0): Moderators can close a thread (no further replies accepted) and re-open it.

**REQ-MOD-008** (v1.0): Moderators can issue IP bans. A ban has a reason, an optional expiry time (NULL = permanent), and is associated with the poster's `ip_hash`.

**REQ-MOD-009** (v1.0): Moderators can expire (immediately lift) any active ban.

**REQ-MOD-010** (v1.0): All moderation actions are recorded in the audit log with: actor identity, action type, target ID and type, timestamp, and relevant details.

### Roles & Access

**REQ-MOD-011** (v1.0, updated v1.1): Five staff roles in order of decreasing privilege:
- **Admin** — full site access: CRUD boards, manage all users, moderate everywhere, view all logs.
- **Janitor** — global moderation on all boards: delete posts/threads, issue/expire bans, resolve flags, sticky/close threads. Assigned globally (not per-board).
- **Board Owner** — owns one or more specific boards via `board_owners` join table: manages `BoardConfig`, assigns board volunteers, moderates their own boards.
- **Board Volunteer** — assigned by a board owner to help moderate specific boards: can delete posts and issue bans on their assigned boards only.
- **User** — registered account with no moderation powers. Can submit staff requests (board creation, volunteer assignment, janitor nomination). Does not interact with the posting pipeline in any way.

**REQ-MOD-011a** (v1.0): Moderation actions (delete, ban, flag) are not a role — they are actions that any sufficiently-privileged role may perform. A Board Volunteer can moderate their assigned board; a Janitor can moderate any board; an Admin can moderate everywhere.

**REQ-MOD-012** (v1.0): Board ownership and board volunteer assignment are stored in join tables (`board_owners`, `board_volunteers`). A Board Owner has full config and moderation permissions on their boards, plus the ability to assign/remove Board Volunteers.

**REQ-MOD-013** (v1.0, updated v1.1): When `Settings.open_registration` is `true` (the default), any visitor may self-register a User account via `POST /auth/register`. When `false`, only Admins can create accounts. Accounts above `User` level are always created or promoted by Admin action (or board owner action for volunteer requests).

**REQ-MOD-013a** (v1.1): Posting is permanently anonymous regardless of account status. A logged-in User posting on a board is indistinguishable from an unauthenticated visitor — same `IpHash`, same anonymous identity, same rate limits. No account information is ever stored in `Post`, `Thread`, `Attachment`, `Ban`, or `Flag` records.

**REQ-MOD-013b** (v1.1): Staff requests are stored in a `staff_requests` table. A User may submit: `board_create` (preferred slug, title, rules, reason), `become_volunteer` (target board slug, reason), or `become_janitor` (reason). Each request has a status of `pending`, `approved`, or `denied`, plus an optional reviewer note visible to the requester.

**REQ-MOD-013c** (v1.1): Approval authority: `board_create` and `become_janitor` require Admin. `become_volunteer` may be approved by Admin or by the board owner of the target board. On approval, the user's role is promoted to the minimum role required (no demotion — a BoardOwner approved as a volunteer is not affected).

**REQ-MOD-013d** (v1.1): Board create requests include preferred `slug`, `title`, `rules`, and a free-text reason/pitch. Admin reviews and may override any field before confirming. On approval the board and its default `BoardConfig` are created atomically and the requester is added to `board_owners`.

### Dashboards

**REQ-MOD-014** (v1.0): Admin dashboard (`/admin/dashboard`): site-wide statistics, recent audit log, board management (CRUD), user account management, all moderation tools.

**REQ-MOD-015** (v1.0): Janitor dashboard (`/janitor/dashboard`): site-wide pending flag queue, active ban count, recent moderation actions log.

**REQ-MOD-015a** (v1.0): Board Owner dashboard (`/board-owner/dashboard`): lists all boards the owner manages. Per-board page (`/board/:slug/dashboard`) provides `BoardConfig` management UI and board volunteer management.

**REQ-MOD-016** (v1.0): Board Owner per-board dashboard: every field in `BoardConfig` is exposed with label, description, current value, and valid range. Changes take effect immediately (within cache TTL).

**REQ-MOD-016a** (v1.0): Board Volunteer dashboard (`/volunteer/dashboard`): pending flags for assigned boards, recent moderation actions.

**REQ-MOD-016b** (v1.1): User dashboard (`/user/dashboard`): current role and status, list of submitted requests with status and reviewer notes, form to submit a new request. Does not display post history or any moderation controls. Accessible only to authenticated users.

---

## 4. Anti-Spam

**REQ-SPAM-001** (v1.0): Rate limiting is applied per IP hash per board. When `board_config.rate_limit_enabled` is true, a poster may make at most `board_config.rate_limit_posts` posts within any `board_config.rate_limit_window_secs` second window. Exceeding the limit returns HTTP 429 with a `Retry-After` header.

**REQ-SPAM-002** (v1.0): Spam heuristics run when `board_config.spam_filter_enabled` is true. Posts scoring above `board_config.spam_score_threshold` are rejected. Heuristics include: body length extremes, link density, duplicate content hash matching against recent posts.

**REQ-SPAM-003** (v1.0): When `board_config.duplicate_check` is true, posts whose body content hash matches a recent post on the same board are rejected with a "duplicate post" error.

**REQ-SPAM-004** (v1.1): When `board_config.captcha_required` is true, posts must include a valid CAPTCHA token. The CAPTCHA provider is selected at compile time (`spam-hcaptcha` or `spam-recaptcha` feature).

**REQ-SPAM-005** (v1.2): DNSBL checking: when enabled, IPs listed on configured DNS blocklists are rejected. Fail-open (lookup failure allows the post).

---

## 5. Configuration & Operations

### Global Configuration

**REQ-OPS-001** (v1.0): All infrastructure configuration (DB URL, Redis URL, JWT secret, S3 credentials, server port) is sourced from environment variables. No hard-coded values. No secrets in source code or Docker images.

**REQ-OPS-002** (v1.0): A `.env.example` file documents every required and optional environment variable with description, type, default value, and which feature flag it applies to.

**REQ-OPS-003** (v1.0): Missing required environment variables cause immediate startup failure with a clear error message identifying the missing variable.

### Per-Board Configuration

**REQ-OPS-004** (v1.0): Every board has a `BoardConfig` row created automatically when the board is created. Default values are conservative (rate limiting on, spam filtering on, no tripcodes, no NSFW).

**REQ-OPS-005** (v1.0): Board configuration changes made through dashboards take effect within one cache TTL cycle (≤60 seconds by default) on all instances.

**REQ-OPS-006** (v1.0): `BoardConfig` changes are recorded in the audit log.

### Health & Observability

**REQ-OPS-007** (v1.0): `GET /healthz` checks database connectivity and Redis connectivity (if compiled), and returns HTTP 200 with `{"status":"ok"}` or HTTP 503 with degraded component details.

**REQ-OPS-008** (v1.0): `GET /metrics` exposes Prometheus-compatible metrics including request count/latency by route, DB query latency, thumbnail generation time, rate limit hits, and spam rejections.

**REQ-OPS-009** (v1.0): All log output is structured JSON in production. Each log line includes at minimum: `timestamp`, `level`, `message`, `request_id` (within a request span), `target` (module path).

**REQ-OPS-010** (v1.0): Graceful shutdown on SIGTERM or SIGINT: stop accepting connections, complete in-flight requests (up to configurable timeout), exit cleanly.

### Backup

**REQ-OPS-011** (v1.0): `scripts/backup.sh` performs a consistent database dump (`pg_dump`) and media file sync (S3 sync or rsync). Intended to run as an external cron job.

**REQ-OPS-012** (v1.0): `scripts/restore.sh` restores from the backup artifacts produced by `scripts/backup.sh`.

---

## 6. Non-Functional Requirements

### Security

**REQ-NFR-001**: Raw IP addresses are never stored. IP hashing uses SHA-256 with a daily-rotating in-memory salt.

**REQ-NFR-002**: EXIF metadata is stripped from all image uploads unconditionally.

**REQ-NFR-003**: All template output is HTML-escaped. XSS via template rendering is structurally prevented by Askama's compile-time checking.

**REQ-NFR-004**: HTTP security headers (CSP, X-Content-Type-Options, X-Frame-Options, Referrer-Policy) are applied to all responses.

**REQ-NFR-005**: Password hashing uses Argon2id with OWASP-recommended parameters (m=19456, t=2, p=1).

**REQ-NFR-006**: CSRF protection is required for all state-changing requests made via cookie-based session auth (v1.1+). JWT bearer auth is inherently CSRF-safe.

**REQ-NFR-007**: CORS policy restricts allowed origins to configured domains only.

### Performance

**REQ-NFR-008**: Post creation p95 latency under < 50ms (text only), < 200ms (with image attachment).

**REQ-NFR-009**: Thread listing p95 latency < 30ms.

**REQ-NFR-010**: The system sustains 500 concurrent users on 4vCPU/8GB without degradation (rate-limited appropriately).

**REQ-NFR-011**: `BoardConfig` reads add no more than 10ms to any operation when the cache is warm (cache miss acceptable up to 10ms DB round trip).

### Reliability

**REQ-NFR-012**: The application layer is stateless. Any instance can handle any request. Horizontal scaling is achieved by adding instances with a shared DB and Redis.

**REQ-NFR-013**: Database migrations are forward-only in v1.0. No migration may break a deployed running instance. Reversible migrations are added in v1.1.

**REQ-NFR-014**: Media processing failures (thumbnail generation errors) are non-fatal — the post is accepted with the original file but without a thumbnail. The failure is logged.

### Usability

**REQ-NFR-015**: All pages function correctly without JavaScript. JavaScript is additive enhancement only.

**REQ-NFR-016**: All pages render correctly on mobile viewports (320px minimum width).

**REQ-NFR-017**: The system is usable over Tor Browser with JavaScript disabled.

### Maintainability

**REQ-NFR-018**: Test coverage ≥ 80% as measured by `cargo tarpaulin`.

**REQ-NFR-019**: CI pipeline passes on every commit to main: `cargo fmt`, `cargo clippy -D warnings`, full test matrix across feature combinations, `cargo audit`.

**REQ-NFR-020**: No `unwrap()` or `expect()` outside `composition.rs` and test fixtures. All error paths are explicit.

**REQ-NFR-021**: Every public type and function in `domains` and `services` has a doc comment.

### Extensibility

**REQ-NFR-022**: Swapping the database adapter requires only: a new feature flag, a new adapter crate module, and a new `#[cfg]` branch in `composition.rs`. Zero changes to `domains` or `services`.

**REQ-NFR-023**: Adding a new per-board behavioral toggle requires only: a `BoardConfig` field, a migration, a service branch, and a dashboard UI control. No recompile, no redeploy for the behavior to take effect.

### Compatibility

**REQ-NFR-024**: Minimum Rust version: 1.75.0 (required for RPITIT).

**REQ-NFR-025**: Deployable via Docker and docker-compose on any Linux host. Kubernetes Helm chart provided for production deployments.

**REQ-NFR-026**: Compatible with PostgreSQL 14+ and Redis 6+.

---

## 7. Explicit Out-of-Scope (v1.0)

These are not requirements gaps — they are deliberate deferrals. Attempting to implement them in v1.0 is scope creep.

| Feature | Deferred To | Notes |
|---------|------------|-------|
| Tripcodes | v1.1 | `BoardConfig.allow_tripcodes` field present; adapter not yet built |
| CAPTCHA | v1.1 | `BoardConfig.captcha_required` field present; adapter not yet built |
| Cookie session auth | v1.1 | JWT is sufficient for v1.0 |
| Two-factor authentication | v1.3 | — |
| User self-registration | Out of scope | Staff accounts only; created by Admin |
| SQLite backend | v1.2 | Ports are defined; adapter not yet built |
| Full-text search | v1.2 | `BoardConfig.search_enabled` field present; adapter not yet built |
| File deduplication | v1.2 | Hash stored; dedup logic not yet built |
| DNSBL checking | v1.2 | Port planned; adapter not yet built |
| WebSockets / live updates | v1.3 | — |
| Thread watcher | v1.3 | — |
| Post hiding (client-side) | v1.3 | — |
| i18n / localization | v1.3 | — |
| ActivityPub federation | v2.0 | `BoardConfig.federation_enabled` field present; adapter not yet built |
| ML spam scoring | v2.0 | — |
| Alternative web framework (Actix) | v1.x | Ports defined; adapter not yet built |
| Cloudflare R2 / Backblaze storage | v1.2 | Ports defined; adapter not yet built |
| Archive / thread history | v1.2 | `BoardConfig.archive_enabled` field present |
| Reactions / post ratings | Not planned | Out of scope indefinitely |
| Private messaging | Not planned | Out of scope indefinitely; contradicts anonymous-first design |

---

## 8. Architecture Scope Invariants

These requirements apply to every version, not just v1.0. They cannot be relaxed without revisiting the foundational architecture decisions in `DECISIONS.md`.

**REQ-ARCH-001**: `domains` and `services` must never depend on adapter crates. Any PR that introduces such a dependency is rejected.

**REQ-ARCH-002**: All external boundaries must be represented as port traits in `domains/ports.rs` before any concrete implementation is written.

**REQ-ARCH-003**: `BoardConfig` is the only mechanism by which an admin or board owner can influence service behavior at runtime. No bypass via environment variables, global state, or feature flags is acceptable.

**REQ-ARCH-004**: Feature flag selection (`#[cfg(feature)]`) occurs exclusively in `composition.rs` and adapter crate modules.

**REQ-ARCH-005**: The `async-trait` macro must not be used. All async traits use RPITIT.
