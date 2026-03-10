# TECHNICALSPECS.md
# rusty-board — Technical Specifications

> **The concrete technical facts: dependencies, schema, endpoints, performance targets, security model, deployment specs. Cross-referenced with ARCHITECTURE.md for structural context.**

---

## 1. Language & Toolchain

| Item | Specification |
|------|--------------|
| Rust edition | 2021 |
| Minimum Rust version | 1.75.0 (required for native RPITIT async in traits) |
| `rustfmt` | Enforced in CI — `cargo fmt --check` |
| `clippy` | `-D warnings` — zero warnings policy |
| `cargo-audit` | Run in CI — no unresolved high/critical advisories |
| `cargo-watch` | Development hot-reload |
| `sqlx-cli` | Migration management; offline mode required for CI |
| `cargo tarpaulin` | Code coverage — target >80% |

---

## 2. Workspace Dependencies (Pinned)

All versions are pinned at the workspace level in the root `Cargo.toml`. Crate-level `Cargo.toml` files reference workspace versions without specifying their own.

```toml
[workspace.dependencies]

# ── Core ────────────────────────────────────────────────────────────────────
anyhow              = "1.0"
thiserror           = "1.0"
serde               = { version = "1.0", features = ["derive"] }
chrono              = { version = "0.4", features = ["serde"] }
uuid                = { version = "1.10", features = ["v4", "fast-rng", "serde"] }
tokio               = { version = "1", features = ["full"] }
tracing             = "0.1"
tracing-subscriber  = { version = "0.3", features = ["json", "env-filter"] }
bytes               = "1.6"
mime                = "0.3"
mime_guess          = "2.0"
once_cell           = "1.19"
dashmap             = "5.5"    # BoardConfigCache

# ── Config ───────────────────────────────────────────────────────────────────
config              = "0.14"
dotenvy             = "0.15"
secrecy             = "0.10"
zeroize             = "1.8"

# ── Web (feature: web-axum) ──────────────────────────────────────────────────
axum                = { version = "0.7", features = ["multipart"] }
tower-http          = { version = "0.5", features = ["trace", "cors", "compression-gzip", "request-id"] }
tower-governor      = "0.4"

# ── Database (feature: db-postgres) ──────────────────────────────────────────
sqlx                = { version = "0.8", features = ["runtime-tokio", "postgres", "chrono", "uuid", "migrate"] }

# ── Cache / Rate limiting (feature: redis) ───────────────────────────────────
deadpool-redis      = "0.17"

# ── Auth (feature: auth-jwt) ─────────────────────────────────────────────────
argon2              = "0.5"
jsonwebtoken        = "9.3"
rand                = "0.8"     # IP salt generation

# ── Media — always compiled ───────────────────────────────────────────────────
image               = "0.25"
oxipng              = "9.1"

# ── Media — feature-gated ────────────────────────────────────────────────────
ffmpeg-next         = "7.1"       # feature: video
pdfium-render       = "0.8"       # feature: documents
aws-sdk-s3          = "1.45"      # feature: media-s3

# ── Templates ────────────────────────────────────────────────────────────────
askama              = { version = "0.12", features = ["with-axum"] }

# ── Observability ────────────────────────────────────────────────────────────
prometheus-client   = "0.22"

# ── Testing (dev only) ───────────────────────────────────────────────────────
mockall             = "0.13"
testcontainers      = "0.20"
testcontainers-modules = { version = "0.3", features = ["postgres", "redis"] }
reqwest             = { version = "0.12", features = ["json", "multipart"] }
insta               = "1.38"
fake                = "2.9"
```

### Dependency Notes

**No `async-trait`**: All port traits use RPITIT (Rust 1.75+). This crate is intentionally absent.

**`ffmpeg-next` build requirements**: Requires libav* system libraries. Docker builder stage must install: `libavcodec-dev libavformat-dev libavutil-dev libswscale-dev`. Adds ~800MB to the builder image. Plan CI caching around this layer. Validate in the target Docker environment before writing any ffmpeg code.

**`pdfium-render` distribution**: Requires a pre-built PDFium binary (not included in the crate). PDFium is BSD-licensed. Validate the distribution model before shipping. Treat the `documents` feature as experimental for v1.0.

**`sqlx` offline mode**: Required for CI. Run `cargo sqlx prepare` after writing any `sqlx::query!` macros. Commit `sqlx-data.json`. Set `SQLX_OFFLINE=true` in CI environment. CI does not have a live database during the compile/lint steps.

**`secrecy`**: Wraps sensitive values (`Settings.jwt_secret`, S3 credentials) to prevent accidental logging via `Debug`/`Display` impls.

---

## 3. Settings Struct (Infrastructure Configuration)

`Settings` is loaded at startup from environment variables and `.env` file. It contains only infrastructure configuration. Per-board behavioral configuration lives in `BoardConfig` in the database.

```rust
pub struct Settings {
    // Server
    pub host:                  String,        // default: "0.0.0.0"
    pub port:                  u16,           // default: 8080
    pub shutdown_timeout_secs: u64,           // default: 30

    // Database
    pub db_url:                Secret<String>,
    pub db_max_connections:    u32,           // default: 10
    pub db_min_connections:    u32,           // default: 2

    // Redis (feature: redis)
    #[cfg(feature = "redis")]
    pub redis_url:             Secret<String>,

    // Auth (feature: auth-jwt)
    #[cfg(feature = "auth-jwt")]
    pub jwt_secret:            Secret<String>,
    pub jwt_ttl_secs:          u64,           // default: 86400 (24h)

    // Argon2 (shared across all auth features)
    pub argon2_m_cost:         u32,           // default: 19456 (recommended)
    pub argon2_t_cost:         u32,           // default: 2
    pub argon2_p_cost:         u32,           // default: 1

    // Media (feature: media-s3)
    #[cfg(feature = "media-s3")]
    pub s3:                    S3Config,

    // Media (feature: media-local)
    #[cfg(feature = "media-local")]
    pub media_path:            PathBuf,       // default: "./media"

    // Media (all storage backends)
    pub media_url_ttl_secs:    u64,           // default: 86400 (24h presigned URL TTL)

    // Media processing (all processor variants)
    pub thumbnail_width_px:    u32,           // default: 320
    pub thumbnail_quality:     u8,            // default: 85 (oxipng compression level)

    // IP privacy
    pub ip_salt_rotation_secs: u64,           // default: 86400 (24h)

    // BoardConfig cache
    pub config_cache_ttl_secs: u64,           // default: 60
}

pub struct S3Config {
    pub bucket:    String,
    pub region:    String,
    pub endpoint:  Option<String>,  // for MinIO or other S3-compatible
    pub access_key: Secret<String>,
    pub secret_key: Secret<String>,
}
```

---

## 4. Database Schema (v1.0 Migrations)

Migrations use `sqlx migrate`. Files live in `storage-adapters/src/migrations/`. Applied at startup via Docker entrypoint or `scripts/migrate.sh`.

### V001__create_users.sql
```sql
CREATE TABLE users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    username      TEXT        NOT NULL UNIQUE
                              CHECK (length(username) BETWEEN 3 AND 32),
    password_hash TEXT        NOT NULL,
    role          TEXT        NOT NULL
                              CHECK (role IN ('admin', 'janitor', 'board_owner', 'board_volunteer')),
    is_active     BOOLEAN     NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### V002__create_boards.sql
```sql
CREATE TABLE boards (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug       TEXT        NOT NULL UNIQUE
                           CHECK (slug ~ '^[a-z0-9_-]{1,16}$'),
    title      TEXT        NOT NULL CHECK (length(title) BETWEEN 1 AND 64),
    rules      TEXT        NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### V003__create_board_configs.sql
```sql
-- One row per board. Created automatically when a board is created.
-- This is the runtime behavior surface controlled by dashboards.
CREATE TABLE board_configs (
    board_id                UUID    PRIMARY KEY REFERENCES boards(id) ON DELETE CASCADE,

    -- Content rules
    bump_limit              INTEGER NOT NULL DEFAULT 500  CHECK (bump_limit > 0),
    max_files               SMALLINT NOT NULL DEFAULT 4  CHECK (max_files BETWEEN 1 AND 10),
    max_file_size_kb        INTEGER NOT NULL DEFAULT 10240 CHECK (max_file_size_kb > 0),
    allowed_mimes           TEXT[]  NOT NULL DEFAULT ARRAY['image/jpeg','image/png','image/gif','image/webp'],
    max_post_length         INTEGER NOT NULL DEFAULT 4000 CHECK (max_post_length BETWEEN 1 AND 32000),

    -- Rate limiting
    rate_limit_enabled      BOOLEAN NOT NULL DEFAULT true,
    rate_limit_window_secs  INTEGER NOT NULL DEFAULT 60  CHECK (rate_limit_window_secs > 0),
    rate_limit_posts        SMALLINT NOT NULL DEFAULT 3  CHECK (rate_limit_posts > 0),

    -- Spam filtering
    spam_filter_enabled     BOOLEAN NOT NULL DEFAULT true,
    spam_score_threshold    REAL    NOT NULL DEFAULT 0.75 CHECK (spam_score_threshold BETWEEN 0 AND 1),
    duplicate_check         BOOLEAN NOT NULL DEFAULT true,

    -- Posting behavior
    forced_anon             BOOLEAN NOT NULL DEFAULT false,
    allow_sage              BOOLEAN NOT NULL DEFAULT true,
    allow_tripcodes         BOOLEAN NOT NULL DEFAULT false,
    captcha_required        BOOLEAN NOT NULL DEFAULT false,
    nsfw                    BOOLEAN NOT NULL DEFAULT false,

    -- Future capabilities (fields present now; adapters ship later)
    search_enabled          BOOLEAN NOT NULL DEFAULT false,
    archive_enabled         BOOLEAN NOT NULL DEFAULT false,
    federation_enabled      BOOLEAN NOT NULL DEFAULT false
);
```

### V004__create_board_owners.sql
```sql
-- Board ownership: many-to-many between users and boards.
-- Board owners can manage BoardConfig for their boards.
CREATE TABLE board_owners (
    board_id    UUID        NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users(id)  ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (board_id, user_id)
);
CREATE INDEX idx_board_owners_user ON board_owners(user_id);
```

### V005__create_threads.sql
```sql
CREATE TABLE threads (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    board_id     UUID        NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    op_post_id   UUID,       -- set after OP post is inserted (self-reference added in V006)
    reply_count  INTEGER     NOT NULL DEFAULT 0,
    bumped_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    sticky       BOOLEAN     NOT NULL DEFAULT false,
    closed       BOOLEAN     NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_threads_board_bumped ON threads(board_id, bumped_at DESC);
CREATE INDEX idx_threads_board_sticky ON threads(board_id, sticky, bumped_at DESC);
```

### V006__create_posts.sql
```sql
CREATE TABLE posts (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id  UUID        NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    body       TEXT        NOT NULL DEFAULT '',
    ip_hash    TEXT        NOT NULL,   -- SHA-256(ip + daily_salt); never raw IP
    name       TEXT,                   -- NULL = anonymous
    tripcode   TEXT,                   -- NULL unless tripcodes enabled
    email      TEXT,                   -- 'sage' disables bump; otherwise unused
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Add OP post foreign key to threads now that posts table exists
ALTER TABLE threads
    ADD CONSTRAINT fk_threads_op_post
    FOREIGN KEY (op_post_id) REFERENCES posts(id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_posts_thread    ON posts(thread_id, created_at ASC);
CREATE INDEX idx_posts_ip_hash   ON posts(ip_hash);
CREATE INDEX idx_posts_body_fts  ON posts USING gin(to_tsvector('english', body));
```

### V007__create_attachments.sql
```sql
CREATE TABLE attachments (
    id             UUID     PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id        UUID     NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    filename       TEXT     NOT NULL,
    mime           TEXT     NOT NULL,
    hash           TEXT     NOT NULL,  -- SHA-256 of original file bytes
    size_kb        INTEGER  NOT NULL,
    media_key      TEXT     NOT NULL,  -- storage key (S3 path or local path)
    thumbnail_key  TEXT,               -- NULL if no thumbnail generated
    spoiler        BOOLEAN  NOT NULL DEFAULT false
);
CREATE INDEX idx_attachments_post ON attachments(post_id);
CREATE INDEX idx_attachments_hash ON attachments(hash);  -- for dedup (v1.2)
```

### V008__create_bans.sql
```sql
CREATE TABLE bans (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ip_hash     TEXT        NOT NULL,
    user_id     UUID        REFERENCES users(id),   -- NULL if posted anonymously
    reason      TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ,                         -- NULL = permanent
    banned_by   UUID        NOT NULL REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_bans_ip_hash        ON bans(ip_hash);
CREATE INDEX idx_bans_ip_active      ON bans(ip_hash, expires_at)
    WHERE expires_at IS NULL OR expires_at > now();  -- active ban lookup
```

### V009__create_flags.sql
```sql
CREATE TABLE flags (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id           UUID        NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    reason            TEXT        NOT NULL,
    reporter_ip_hash  TEXT        NOT NULL,
    status            TEXT        NOT NULL DEFAULT 'pending'
                                  CHECK (status IN ('pending', 'approved', 'rejected')),
    resolved_by       UUID        REFERENCES users(id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_flags_status  ON flags(status);
CREATE INDEX idx_flags_post    ON flags(post_id);
```

### V010__create_audit_logs.sql
```sql
CREATE TABLE audit_logs (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id      UUID        REFERENCES users(id),   -- NULL for anonymous actions
    actor_ip_hash TEXT,                                -- populated when no actor_id
    action        TEXT        NOT NULL,
    -- Actions: delete_post, delete_thread, sticky_thread, close_thread,
    --          ban_ip, expire_ban, resolve_flag, update_board_config,
    --          create_board, delete_board, create_user, deactivate_user
    target_id     UUID,                                -- post/thread/user/board UUID
    target_type   TEXT,                                -- 'post','thread','user','board','ban','flag'
    details       JSONB,                               -- action-specific details
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_actor  ON audit_logs(actor_id, created_at DESC);
CREATE INDEX idx_audit_target ON audit_logs(target_id, created_at DESC);
CREATE INDEX idx_audit_time   ON audit_logs(created_at DESC);
```

### V014__add_user_role.sql *(v1.1)*
```sql
-- Add 'user' to the role constraint and create the staff_requests table.

ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check
    CHECK (role IN ('admin', 'janitor', 'board_owner', 'board_volunteer', 'user'));

CREATE TYPE staff_request_type   AS ENUM ('board_create', 'become_volunteer', 'become_janitor');
CREATE TYPE staff_request_status AS ENUM ('pending', 'approved', 'denied');

CREATE TABLE staff_requests (
    id            UUID                PRIMARY KEY DEFAULT gen_random_uuid(),
    from_user_id  UUID                NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_type  staff_request_type  NOT NULL,
    target_slug   TEXT,               -- for become_volunteer: the board slug requested
    payload       JSONB NOT NULL DEFAULT '{}',
                  -- board_create: {slug, title, rules, reason}
                  -- become_volunteer / become_janitor: {reason}
    status        staff_request_status NOT NULL DEFAULT 'pending',
    reviewed_by   UUID                REFERENCES users(id) ON DELETE SET NULL,
    review_note   TEXT,
    created_at    TIMESTAMPTZ         NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ         NOT NULL DEFAULT now()
);
CREATE INDEX idx_staff_req_user   ON staff_requests(from_user_id, created_at DESC);
CREATE INDEX idx_staff_req_status ON staff_requests(status, created_at DESC);
CREATE INDEX idx_staff_req_slug   ON staff_requests(target_slug) WHERE target_slug IS NOT NULL;
```

### V015__create_staff_messages.sql *(v1.1)*
```sql
CREATE TABLE staff_messages (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    from_user_id UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    to_user_id   UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body         TEXT        NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    read_at      TIMESTAMPTZ,          -- NULL = unread
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_staff_msg_to   ON staff_messages(to_user_id, created_at DESC);
CREATE INDEX idx_staff_msg_from ON staff_messages(from_user_id, created_at DESC);
-- Cleanup: delete messages older than 14 days (run as periodic task)
```

---

## 5. API Endpoints (v1.0 — Axum Default)

### Public (No Authentication)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/boards` | `list_boards` | All boards, paginated |
| GET | `/board/:slug` | `show_board` | Thread list for board, paginated, sorted by bumped_at |
| GET | `/board/:slug/catalog` | `show_catalog` | Catalog grid view (all threads, thumbnail + summary) |
| GET | `/board/:slug/thread/:id` | `show_thread` | Thread with all posts, paginated |
| POST | `/board/:slug/post` | `create_post` | Create thread (no thread_id) or reply (thread_id in body). Multipart: body + files |
| POST | `/board/:slug/thread/:id/flag` | `create_flag` | Report a post |
| GET | `/overboard` | `show_overboard` | Recent posts across all boards |
| GET | `/healthz` | `health_check` | DB + Redis + media health |
| GET | `/metrics` | `metrics` | Prometheus metrics export |

### Auth (No Authentication Required to Attempt)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/auth/register` | `register` | Self-register a User account (when `open_registration = true`) |
| POST | `/auth/login` | `login` | Submit username + password; receive JWT on success |
| POST | `/auth/refresh` | `refresh_token` | Refresh a valid JWT; receive new JWT |

### Dashboards (all roles)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/admin/dashboard` | `admin_dashboard` | Site stats, audit log, board overview, user management |
| GET | `/janitor/dashboard` | `janitor_dashboard` | Site-wide flag queue, active bans, recent actions |
| GET | `/board-owner/dashboard` | `board_owner_top_dashboard` | Lists all boards the owner manages |
| GET | `/board/:slug/dashboard` | `board_owner_dashboard` | Per-board config + volunteer management |
| GET | `/volunteer/dashboard` | `volunteer_dashboard` | Flag queue for assigned boards, recent actions |
| GET | `/mod/dashboard` | `mod_dashboard_redirect` | Compat shim — `303 See Other` to caller's own dashboard |

### Board Owner (Role: BoardOwner or Admin, or user owns the board)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/board/:slug/config` | `get_board_config` | Retrieve current BoardConfig |
| PUT | `/board/:slug/config` | `update_board_config` | Update BoardConfig fields |
| GET | `/board/:slug/volunteers` | `list_volunteers` | List volunteers assigned to board |
| POST | `/board/:slug/volunteers` | `add_volunteer` | Assign a volunteer to board |
| DELETE | `/board/:slug/volunteers/:user_id` | `remove_volunteer` | Remove volunteer assignment |

### Moderation (Role: Janitor or Admin — site-wide; BoardOwner/BoardVolunteer on their boards)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/mod/flags` | `list_flags` | Pending flag queue, paginated |
| POST | `/mod/flags/:id/resolve` | `resolve_flag` | Approve or reject a flag |
| POST | `/mod/posts/:id/delete` | `delete_post` | Delete a post and record audit entry |
| POST | `/mod/threads/:id/delete` | `delete_thread` | Delete a thread and all posts |
| POST | `/mod/threads/:id/sticky` | `toggle_sticky` | Toggle thread sticky status |
| POST | `/mod/threads/:id/close` | `toggle_closed` | Toggle thread closed status |
| POST | `/mod/bans` | `create_ban` | Issue an IP ban |
| POST | `/mod/bans/:id/expire` | `expire_ban` | Immediately expire a ban |
| GET | `/mod/bans` | `list_bans` | All bans (active and expired), paginated |

### User (Role: User or above)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/user/dashboard` | `user_dashboard` | Request history + new request form |
| GET | `/user/requests` | `list_user_requests` | Paginated request history for current user |
| POST | `/user/requests` | `submit_request` | Submit a new staff request |

### Staff Request Management

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/admin/requests` | `list_all_requests` | All pending requests (Admin only) |
| POST | `/admin/requests/:id/approve` | `admin_approve_request` | Approve any request; board_create body may override slug/title/rules |
| POST | `/admin/requests/:id/deny` | `admin_deny_request` | Deny any request with optional note |
| POST | `/board/:slug/requests/:id/approve` | `board_owner_approve_request` | Approve become_volunteer for owned board |
| POST | `/board/:slug/requests/:id/deny` | `board_owner_deny_request` | Deny become_volunteer for owned board |

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/admin/dashboard` | `admin_dashboard` | Site stats, audit log, board overview |
| POST | `/admin/boards` | `create_board` | Create a board (creates default BoardConfig automatically) |
| PUT | `/admin/boards/:id` | `update_board` | Update board title/rules |
| DELETE | `/admin/boards/:id` | `delete_board` | Delete a board and all content |
| GET | `/admin/users` | `list_users` | All moderator/admin accounts |
| POST | `/admin/users` | `create_user` | Create a moderator/admin account |
| POST | `/admin/users/:id/deactivate` | `deactivate_user` | Deactivate a user account |
| POST | `/admin/boards/:id/owners` | `add_board_owner` | Assign a user as board owner |
| DELETE | `/admin/boards/:id/owners/:user_id` | `remove_board_owner` | Remove board owner |
| GET | `/admin/audit` | `list_audit_log` | Full audit log, paginated |

**Future**: `GET /search` (v1.2), `GET /ws/thread/:id` WebSocket (v1.3).

---

## 6. Performance Targets

| Metric | Target | Measured By |
|--------|--------|------------|
| Post creation p95 latency (text only) | < 50ms | criterion + load test |
| Post creation p95 latency (with image) | < 200ms | criterion + load test |
| Thread listing p95 latency | < 30ms | criterion |
| BoardConfig cache miss (DB read) | < 10ms | criterion |
| Argon2id hash (login) | 200–500ms | intentional — security parameter |
| Thumbnail generation (1MB JPEG) | < 150ms | criterion |
| Concurrent users sustained | 500 on 4vCPU/8GB | k6 load test |
| Thread prune | Sync on post creation | < 5ms per thread deleted |

Benchmarks tracked in `docs/performance.md` via `criterion`. Thumbnail generation is the primary performance risk and must be benchmarked early with representative fixture files.

---

## 7. Security Specifications

### HTTP Security Headers

Applied via `tower-http` middleware to all responses:

| Header | Value |
|--------|-------|
| `Content-Security-Policy` | `default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'` |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains` (set by reverse proxy or app) |
| `X-Request-ID` | UUID generated per request, propagated through logs |

### Secrets Management

- All secrets (JWT secret, DB URL, S3 credentials) sourced from environment variables only.
- `secrecy::Secret<String>` wrapper prevents accidental logging via `Debug`/`Display`.
- No secrets in source code, git history, Docker images, or `BoardConfig` (which is visible in DB plaintext).
- `.env` files are in `.gitignore` and `.dockerignore`.

### IP Privacy

- Raw IP addresses are never stored anywhere in the database.
- `IpHash = SHA-256(raw_ip + daily_salt)` where `daily_salt` is generated at startup and lives only in memory.
- The salt rotates on restart (configurable via `Settings.ip_salt_rotation_secs`). Post-rotation, existing `ip_hash` values cannot be correlated back to an IP.
- Bans are enforced by `ip_hash`. A banned user who obtains a new IP is no longer banned.

### EXIF Stripping

All uploaded images have EXIF metadata stripped in `images.rs` before storage. This is unconditional — not a `BoardConfig` toggle, not a `Settings` field. It is a hard coded business rule enforced at the processor level.

### Input Validation

| Input | Validation | Location |
|-------|-----------|---------|
| Post body length | `≤ board_config.max_post_length` | `PostService` |
| File size | `≤ board_config.max_file_size` | `PostService` |
| MIME type | Against `board_config.allowed_mimes` | `MediaProcessor` |
| Board slug | Regex `^[a-z0-9_-]{1,16}$` | `Slug` value object |
| Username | Length 3–32, alphanumeric + underscore | `User` domain validation |
| Password | Minimum 12 characters | `UserService` |
| HTML template output | Automatic escaping | Askama (compile-time) |

### Authentication Security

- Argon2id parameters: `m=19456` (19MB), `t=2`, `p=1`. These are the OWASP-recommended minimum for 2024.
- JWT tokens are bearer tokens. They are stateless — revocation is not supported in v1.0 (v1.1: session table for cookie auth enables revocation).
- CSRF protection: not required for JWT bearer auth (stateless tokens are not CSRF-vulnerable). Required for cookie session auth (v1.1+) via double-submit pattern.

### Ban Enforcement

Active ban check (`BanRepository::find_active_by_ip`) runs on every post creation inside `PostService`, regardless of `BoardConfig` settings. This is not a behavioral toggle — bans always apply.

---

## 8. Deployment Specifications

### Docker Build

Multi-stage Dockerfile:

**Stage 1: builder** (`rust:1.75-slim`)
- Install system dependencies: `pkg-config libssl-dev` (always)
- If `video` feature: `libavcodec-dev libavformat-dev libavutil-dev libswscale-dev`
- If `documents` feature: download pre-built PDFium binary
- `cargo build --release --features <features>`
- Binary: `target/release/rusty-board`

**Stage 2: runtime** (`debian:bookworm-slim` or `gcr.io/distroless/cc`)
- Copy binary + `templates/` + `static/`
- If `video` feature: copy shared libav* libraries
- If `documents` feature: copy PDFium binary
- `HEALTHCHECK CMD curl -f http://localhost:8080/healthz || exit 1`
- `EXPOSE 8080`
- `ENTRYPOINT ["/rusty-board"]`

### docker-compose Stack

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment: { POSTGRES_DB, POSTGRES_USER, POSTGRES_PASSWORD }
    volumes: [pgdata:/var/lib/postgresql/data]
    healthcheck: { test: pg_isready }

  redis:          # Required if redis feature compiled
    image: redis:7-alpine
    healthcheck: { test: redis-cli ping }

  minio:          # Required if media-s3 feature compiled
    image: minio/minio
    command: server /data --console-address ":9001"
    volumes: [minio_data:/data]

  app:
    build: .
    depends_on: [postgres, redis, minio]
    env_file: .env
    ports: ["8080:8080"]
```

### Kubernetes

Helm chart in `deploy/helm/rusty-board/`:
- `Deployment`: `replicas: 2+`, rolling update strategy, resource limits (request: 256Mi/0.25CPU; limit: 1Gi/1CPU)
- `Service`: ClusterIP on port 8080
- `Ingress`: TLS termination, `nginx.ingress.kubernetes.io/proxy-body-size: 10m` (for file uploads)
- `ConfigMap`: Non-secret settings
- `Secret`: `DB_URL`, `JWT_SECRET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`
- `HorizontalPodAutoscaler`: min 2, max 10, CPU target 70%

### TLS

TLS is terminated at the reverse proxy (NGINX or cloud load balancer). The application binds HTTP on `:8080`. `Strict-Transport-Security` header is set by the application middleware so it propagates correctly even when TLS is upstream.

### Migrations

```bash
# Run at startup (Docker entrypoint) or manually
sqlx migrate run --database-url $DB_URL

# Equivalent via Makefile
make migrate
```

Migrations are in `storage-adapters/src/migrations/`. They are run by the application binary on startup using `sqlx::migrate!()` embedded in the binary. Forward-only in v1.0. Reversible migrations (down.sql) added in v1.1.

### Backup

```bash
# scripts/backup.sh — run as cron job, not from app container
pg_dump $DB_URL | gzip > backup_$(date +%Y%m%d).sql.gz

# For media-s3:
aws s3 sync s3://$S3_BUCKET ./media_backup/

# For media-local:
rsync -av $MEDIA_PATH ./media_backup/
```

Backup files should be stored off-host (e.g., different S3 bucket, different region) and retained for at least 30 days.

### Graceful Shutdown

The binary listens for `SIGTERM` and `Ctrl-C`. On signal:
1. Stop accepting new connections
2. Wait for in-flight requests to complete (up to `Settings.shutdown_timeout_secs`)
3. Flush pending log records
4. Exit 0

This is compatible with Kubernetes rolling deployments.

---

## 9. CI Matrix

```yaml
# .github/workflows/ci.yml — key jobs

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps: [cargo fmt --check]

  clippy:
    runs-on: ubuntu-latest
    steps: [cargo clippy --all-features -- -D warnings]

  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        features:
          - "web-axum,db-postgres,auth-jwt,media-local,redis"
          - "web-axum,db-postgres,auth-jwt,media-s3,redis"
          - "web-axum,db-postgres,auth-jwt,media-local,redis,video"
          - "web-axum,db-postgres,auth-jwt,media-local,redis,documents"
          - "web-axum,db-postgres,auth-jwt,media-local,redis,video,documents"
    services:
      postgres: { image: postgres:16, ... }
      redis:    { image: redis:7, ... }
    env:
      SQLX_OFFLINE: true
    steps:
      - cargo test --features ${{ matrix.features }}

  audit:
    runs-on: ubuntu-latest
    steps: [cargo audit]

  docker:
    runs-on: ubuntu-latest
    steps: [docker build ., docker run --healthcheck]
```
