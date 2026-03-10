# `storage-adapters` — Persistence & Media Adapters

Concrete implementations of every storage-related port trait from `domains`. Feature-gated — only the selected adapters are compiled into the binary.

---

## Feature Flags

| Flag | Adapter(s) enabled |
|------|--------------------|
| `db-postgres` | All `Pg*Repository` types + migrations runner |
| `db-sqlite` | `Sqlite*Repository` types — **v1.2, not yet implemented** |
| `media-local` | `LocalFsMediaStorage` |
| `media-s3` | `S3MediaStorage` |
| `video` | `VideoMediaProcessor` (ffmpeg-next, optional) |
| `documents` | `DocumentMediaProcessor` (pdfium-render, optional) |
| `redis` | `RedisRateLimiter` |

Default build uses `db-postgres`, `media-local`, `redis`.

---

## Module Map

```
src/
├── postgres/                   # feature: db-postgres
│   ├── connection.rs           # PgPool constructor from Settings
│   └── repositories/
│       ├── board_repository.rs         # PgBoardRepository
│       ├── thread_repository.rs        # PgThreadRepository
│       ├── post_repository.rs          # PgPostRepository
│       ├── ban_repository.rs           # PgBanRepository
│       ├── flag_repository.rs          # PgFlagRepository
│       ├── audit_repository.rs         # PgAuditRepository
│       ├── user_repository.rs          # PgUserRepository
│       ├── staff_request_repository.rs # PgStaffRequestRepository (v1.1)
│       ├── staff_message_repository.rs # PgStaffMessageRepository (v1.1)
│       └── session_repository.rs       # PgSessionRepository (v1.1)
├── sqlite/                     # feature: db-sqlite — TODO v1.2
├── media/
│   ├── images.rs               # ImageMediaProcessor — always compiled
│   ├── videos.rs               # VideoMediaProcessor — feature: video
│   ├── documents.rs            # DocumentMediaProcessor — feature: documents
│   ├── s3.rs                   # S3MediaStorage — feature: media-s3
│   └── local_fs.rs             # LocalFsMediaStorage — feature: media-local
├── cache/
│   └── board_config.rs         # BoardConfigCache (DashMap + 60s TTL)
├── redis/                      # feature: redis
│   └── mod.rs                  # RedisRateLimiter
├── in_memory/                  # No deps — always compiled
│   ├── rate_limiter.rs         # InMemoryRateLimiter (DashMap sliding window)
│   └── session_repository.rs   # InMemorySessionRepository (DashMap)
├── stubs/
│   └── mod.rs                  # NoopRateLimiter, NoopMediaStorage etc. for tests
└── migrations/                 # 13 consolidated final-state migrations
    ├── 001_create_users.sql
    ├── 002_create_boards.sql
    ├── 003_create_board_configs.sql
    ├── 004_create_board_ownership.sql
    ├── 005_create_threads.sql
    ├── 006_create_posts.sql
    ├── 007_create_attachments.sql
    ├── 008_create_bans.sql
    ├── 009_create_flags.sql
    ├── 010_create_audit_logs.sql
    ├── 011_create_staff_requests.sql
    ├── 012_create_staff_messages.sql
    └── 013_create_user_sessions.sql
```

Every migration has a matching `.down.sql`. All `CREATE TABLE` / `CREATE INDEX` statements use `IF NOT EXISTS` for idempotency.

---

## PostgreSQL Repositories

All `Pg*Repository` types wrap a `PgPool` (cloned cheaply — Arc internally). They implement the port trait from `domains::ports` and return `DomainError` — never `sqlx::Error`.

### `PgPostRepository` notable methods

| Method | SQL |
|--------|-----|
| `save` | `INSERT INTO posts` with `RETURNING post_number` (counter driven by board-level sequence) |
| `delete` | `DELETE FROM posts WHERE id = $1` |
| `delete_by_ip_in_thread` | `DELETE FROM posts WHERE ip_hash = $1 AND thread_id = $2` — returns row count for [D*] mod action |
| `find_by_ip_hash` | Cross-thread IP history for moderation use |
| `search_fulltext` | `plainto_tsquery` + `ts_rank` — only called when `board_config.search_enabled` |

### `PgThreadRepository` notable methods

| Method | SQL |
|--------|-----|
| `set_sticky` | `UPDATE threads SET sticky = $2 WHERE id = $1` |
| `set_closed` | `UPDATE threads SET closed = $2 WHERE id = $1` |
| `bump` | `UPDATE threads SET bumped_at = now() WHERE id = $1` |
| `prune_oldest` | `DELETE FROM threads WHERE board_id = $1 ORDER BY bumped_at ASC LIMIT 1` |

---

## In-Memory Adapters

`InMemoryRateLimiter` and `InMemorySessionRepository` are production-capable for single-instance deployments and are the default for development. They use `DashMap` for concurrent access with no external dependencies.

- `InMemoryRateLimiter` — sliding window counter per `(ip_hash, board_id)` key
- `InMemorySessionRepository` — session store with TTL-based expiry; `purge_expired()` called periodically

---

## Migrations

Run with:

```sh
make migrate
# or directly:
sqlx migrate run --database-url "${DB_URL}" --source crates/storage-adapters/src/migrations
```

Rollback a single step:

```sh
sqlx migrate revert --database-url "${DB_URL}" --source crates/storage-adapters/src/migrations
```

**Do not add incremental patch migrations** for schema changes that can be expressed as final-state rewrites of an existing migration during development. Only add a new numbered migration when the change must be applied to a live database with existing data.

---

## v1.1 Status

All PostgreSQL repositories shipped and wired. Open items:

| Item | Target |
|------|--------|
| `SqlitePostRepository` and all Sqlite variants | v1.2 |
| `VideoMediaProcessor` (ffmpeg-next integration) | v1.0 patch (stub shipped) |
| `DocumentMediaProcessor` (pdfium-render integration) | v1.0 patch (stub shipped) |
| `RedisSessionRepository` for multi-instance deployments | v1.2 |
