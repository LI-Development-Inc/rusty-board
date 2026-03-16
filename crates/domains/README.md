# `domains` — Core Domain Models & Port Traits

The innermost crate. Everything else depends on this. Nothing here depends on anything outside `std`.

---

## What Lives Here

| Module | Contents |
|--------|----------|
| `models.rs` | All domain structs, enums, value objects, and `BoardConfig` |
| `ports.rs` | Every external boundary as an async trait |
| `errors.rs` | `DomainError` and its variants |

### Domain Models

| Type | Description |
|------|-------------|
| `Board` | An imageboard board (`/b/`, `/tech/`, etc.) |
| `Thread` | A thread within a board; sticky/closed/cycle state |
| `Post` | An anonymous post; contains `IpHash`, never raw IP; `pinned` flag for cycle threads |
| `Attachment` | Media file metadata (stored separately in `MediaStorage`) |
| `Ban` | IP ban with optional expiry |
| `Flag` | User report against a post, pending staff review |
| `AuditEntry` | Immutable log of every moderation action |
| `StaffRequest` | A `Role::User` account requesting elevated access |
| `StaffMessage` | Internal message between staff accounts |
| `User` | Staff accounts only — anonymous posters have no model |
| `BoardConfig` | Complete runtime behavior surface for a board |
| `CurrentUser` | Parsed JWT/session context available in handlers |

### Value Objects

`BoardId`, `ThreadId`, `PostId`, `UserId`, `BanId`, `FlagId`, `StaffRequestId`, `IpHash`, `MediaKey`, `ContentHash`, `Slug`, `FileSizeKb`, `Page`, `Paginated<T>`

All ID types wrap `Uuid` with a `::new()` constructor and implement `Display`, `Serialize`, `Deserialize`.

### Port Traits (summary — see [`docs/PORTS.md`](../../docs/PORTS.md) for full signatures)

| Trait | Used by | Shipped adapter(s) |
|-------|---------|-------------------|
| `BoardRepository` | `BoardService` | `PgBoardRepository` |
| `ThreadRepository` | `ThreadService`, `PostService` | `PgThreadRepository` |
| `PostRepository` | `PostService`, `ModerationService` | `PgPostRepository` |
| `BanRepository` | `PostService`, `ModerationService` | `PgBanRepository` |
| `FlagRepository` | `ModerationService` | `PgFlagRepository` |
| `AuditRepository` | `ModerationService` | `PgAuditRepository` |
| `UserRepository` | `UserService`, `ModerationService` | `PgUserRepository` |
| `MediaStorage` | `PostService` | `LocalFsMediaStorage`, `S3MediaStorage` |
| `MediaProcessor` | `PostService` | `ImageMediaProcessor` |
| `RateLimiter` | `PostService` | `InMemoryRateLimiter`, `RedisRateLimiter` |
| `AuthProvider` | `UserService` | `JwtAuthProvider`, `CookieAuthProvider` |
| `SessionRepository` | `CookieAuthProvider` | `InMemorySessionRepository`, `PgSessionRepository` |
| `StaffRequestRepository` | `StaffRequestService` | `PgStaffRequestRepository` |
| `StaffMessageRepository` | `StaffMessageService` | `PgStaffMessageRepository` |
| `DnsblChecker` | `PostService` | `SpamhausDnsblChecker` (`spam-dnsbl`), `NoopDnsblChecker` |
| `ArchiveRepository` | `ThreadService`, `PostService` | `PgArchiveRepository`, `NoopArchiveRepository` |

### Roles

```
Admin           Full site control
Janitor         Site-wide moderation
BoardOwner      Manages specific owned boards
BoardVolunteer  Board-scoped moderation only
User            Registered account; no mod powers; can submit staff requests
```

Anonymous posters have no role and no `User` record. Posting is anonymous at the model level regardless of login status (Invariant #11).

### `BoardConfig` — Runtime Behavior Surface

One row per board. Loaded per request, cached 60 seconds in-process. All behavioral toggles live here — no redeployment needed to change board behavior.

Key fields: `bump_limit`, `rate_limit_*`, `spam_*`, `forced_anon`, `allow_sage`, `allow_tripcodes`, `captcha_required`, `nsfw`, `search_enabled`, `archive_enabled`, `federation_enabled`.

See `ARCHITECTURE.md §8` for the full field table.

---

## Invariants

1. **No I/O**. No `tokio`, no `sqlx`, no framework imports. Only `std`, `chrono`, `serde`, `uuid`, `thiserror`.
2. **No `#[cfg(feature)]`**. Feature flags never appear in this crate.
3. **Port traits only**. Implementations belong in adapter crates.
4. **All port traits use native RPITIT async** — no `#[async_trait]` macro.
5. **`unwrap()` / `expect()` only in tests**.

---

## Adding a New Port

1. Define the trait in `ports.rs` with `///` doc comments on every method
2. Add an entry to `docs/PORTS.md`
3. Add a `Noop<PortName>` test stub in `integration-tests`
4. Add `#[automock]` for `mockall` mock generation in service tests
5. Do not implement it here — implementations go in adapter crates

See `docs/CONVENTIONS.md` for the full port addition checklist.

---

## v1.2 Status — Complete

- `Thread::cycle`, `Post::pinned` (migration 014), `AuditAction::CycleThread/PinPost`
- `DnsblChecker` port + `SpamhausDnsblChecker` adapter
- `ArchiveRepository` port + `PgArchiveRepository` (migration 015)
- `ThreadRepository::find_oldest_for_archive`, `PostRepository::find_attachment_by_hash`, `set_pinned`, etc.
