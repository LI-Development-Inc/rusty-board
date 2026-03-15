# `services` — Business Logic

All application business logic lives here. Generic over port traits defined in `domains`. No SQL, no HTTP, no adapter imports of any kind.

---

## What Lives Here

| Module | Service | Responsibilities |
|--------|---------|-----------------|
| `board/` | `BoardService<BR>` | Board CRUD, slug validation, `BoardConfig` load/update |
| `thread/` | `ThreadService<TR>` | Create thread, bump, sticky/close, prune oldest when over limit |
| `post/` | `PostService<PR,TR,BR,MS,RL,MP>` | Validate, spam check, rate limit, process media, insert post, bump thread |
| `moderation/` | `ModerationService<BR,PR,TR,FR,AR,UR>` | Ban, flag, delete post/thread, bulk delete by IP, sticky, close, audit |
| `user/` | `UserService<UR,AP>` | Register (`Role::User`), create staff accounts, login, session management |
| `staff_request/` | `StaffRequestService<SRR,UR>` | Submit, approve, deny staff elevation requests |
| `staff_message/` | `StaffMessageService<SMR>` | Send, list, mark-as-read internal staff messages |
| `common/utils.rs` | — | `slug_validate`, `paginate`, `now_utc`, `hash_ip`, `hash_content`, `parse_quotes`, `score_spam` |
| `common/tripcode.rs` | — | Insecure trip (`!`), secure trip (`!!`), capcode (`####Role`), super trip stub (`###`) |

---

## Key Design Patterns

### Generic over Ports

Services are generic structs — the composition root provides concrete types at compile time:

```rust
pub struct PostService<PR, TR, BR, MS, RL, MP>
where
    PR: PostRepository,
    TR: ThreadRepository,
    BR: BanRepository,
    MS: MediaStorage,
    RL: RateLimiter,
    MP: MediaProcessor,
{ ... }
```

### BoardConfig-driven Behavior

All behavioral branching reads `BoardConfig` fields passed by reference. Feature flags never appear in service code:

```rust
if board_config.spam_filter_enabled {
    self.check_spam(&draft, board_config).await?;
}
if board_config.rate_limit_enabled {
    self.check_rate_limit(&draft.ip_hash, board_config).await?;
}
```

### Audit Trail

Every moderation action writes an `AuditEntry`. `write_audit()` is fire-and-forget — audit failures are logged but never propagate to callers (moderation action succeeds regardless).

---

## Service Summaries

### `PostService::create_post`

Full pipeline in order:
1. Ban check (always)
2. Rate limit check (if `board_config.rate_limit_enabled`)
3. Body length validation
4. Forced anon name erasure (if `board_config.forced_anon`)
5. Duplicate content check (if `board_config.duplicate_check`)
6. Spam heuristic score (if `board_config.spam_filter_enabled`)
7. File count and MIME validation (against `board_config`)
8. Media processing (resize, EXIF strip, thumbnail)
9. Post insert + attachment rows
10. Thread bump (unless sage or past bump limit)
11. Rate limit increment
12. Audit log

> **TODO v1.2**: step 10 will also trigger oldest-post pruning when `thread.cycle == true` and past bump limit. See stub comment in `post/mod.rs`.

### `PostService` read methods

| Method | Description |
|--------|-------------|
| `list_posts(thread_id, page)` | Paginated posts in a thread |
| `list_overboard(page)` | Recent posts across all boards (overboard view) |
| `find_post_attachments(post_ids)` | Bulk-fetch attachment metadata for a set of post IDs |

### `ModerationService`

| Method | Route | Action |
|--------|-------|--------|
| `delete_post` | `POST /mod/posts/:id/delete` | Delete single post |
| `delete_thread` | `POST /mod/threads/:id/delete` | Delete thread + all posts |
| `delete_posts_by_ip_in_thread` | `POST /mod/threads/:id/delete-by-ip` | [D*] bulk delete by IP |
| `ban_ip` | `POST /mod/bans` | Issue IP ban |
| `set_sticky` | `POST /mod/threads/:id/sticky` | Set/clear sticky |
| `set_closed` | `POST /mod/threads/:id/close` | Set/clear closed |
| `file_flag` | `POST /board/:slug/thread/:id/flag` | User submits report |
| `resolve_flag` | `POST /mod/flags/:id/resolve` | Staff resolves report |

### `UserService`

- `register(username, password)` → creates `Role::User` account
- `create_user(username, password, role)` → admin-only account creation
- `login(username, password)` → returns `Token` on success
- `get_user(user_id)` → load a single `User` record by ID (used by dashboards)
- Password hashing delegated to `AuthProvider` (argon2id via `JwtAuthProvider` or `CookieAuthProvider`)

### `common/tripcode.rs`

| Input | Output | Algorithm |
|-------|--------|-----------|
| `name#password` | `!ABCxyz` | SHA-512, base64 truncated to 8 chars |
| `name##password` | `!!ABCxyz` | HMAC-SHA256 with per-instance pepper |
| `name###Role` | `!!!STUB` | ed25519 — **TODO v1.2** (needs `TripkeyRepository`) |
| `####Role` | capcode | JWT-verified staff identity display |

---

## Invariants

1. Depends **only** on `domains`. No `sqlx`, `axum`, `aws_sdk_s3`, `redis`.
2. No `#[cfg(feature)]` anywhere in this crate.
3. All behavioral branching driven by `BoardConfig` fields, never by feature flags.
4. `unwrap()` / `expect()` only in `#[cfg(test)]` blocks and test helpers.
5. All public service methods have `#[instrument]` tracing spans.

---

## Testing

Service unit tests use `mockall`-generated mocks for all port traits. Every significant branch in service code has at least one happy-path and one error-path test.

Run: `cargo test -p services`

Current coverage: **84 tests**, all passing.

---

## v1.1 Status

All services shipped. Open items tracked in `ROADMAP.md`:

| Item | Target |
|------|--------|
| `CaptchaVerifier` wiring in `PostService` | v1.1.1 |
| Super tripcode `###` ed25519 implementation | v1.2 |
| Thread cycle pruning in `PostService::create_post` | v1.2 |
