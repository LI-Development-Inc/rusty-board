# CONVENTIONS.md
# rusty-board — Coding Conventions & Standards

> **The rules that keep the codebase consistent, navigable, and architecturally sound as it grows. All pull requests are reviewed against these conventions. Architectural rules are non-negotiable. Style rules require justification to override.**

---

## 1. The Non-Negotiable Architectural Rules

These are invariants, not preferences. A PR that violates any of these is rejected.

1. `domains` and `services` never contain `#[cfg(feature = "...")]` reads.
2. `domains` and `services` never import from adapter crates (`sqlx`, `axum`, `aws_sdk_s3`, `jsonwebtoken`, `deadpool_redis`, etc.).
3. Services branch on `BoardConfig` fields. They never branch on feature flags, env vars, or global state.
4. `BoardConfig` is the only path from dashboard UI to service behavior.
5. `unwrap()` and `expect()` appear only in `composition.rs` and test fixtures. Never in service methods, handlers, or adapter implementations.
6. All port traits use native async (RPITIT). The `async-trait` macro is not used anywhere.
7. Concrete adapter types are instantiated only in `composition.rs`.
8. Every public type and function in `domains` and `services` has a `///` doc comment.
9. Every new file has a module-level comment explaining what the module does and why it exists separately.
10. No `#[allow(clippy::...)]` suppressions without a comment explaining why.

---

## 2. What Belongs Where

The most common design question is: "Where does this go?" Apply this in order:

| If it... | It belongs in... |
|----------|----------------|
| Is a pure data structure or enum with no I/O | `domains/models.rs` |
| Is a capability boundary (DB, auth, storage, rate limit) | `domains/ports.rs` as a trait |
| Is a domain invariant or validation rule | `domains/errors.rs` or `domains/models.rs` |
| Is business logic that varies by board | `services/*/mod.rs`, branching on `BoardConfig` |
| Is business logic that is fixed for all boards | `services/*/mod.rs`, unconditional |
| Is infrastructure configuration (URLs, secrets, timeouts) | `configs/src/lib.rs` in `Settings` |
| Is per-board behavioral configuration | `domains/models.rs` in `BoardConfig` + DB migration |
| Is a concrete implementation of a port | Appropriate adapter crate, feature-gated |
| Is adapter selection logic | `composition.rs` only |
| Is HTTP request/response handling | `api-adapters/*/handlers/` |
| Is URL routing | `api-adapters/*/routes/` |
| Is HTTP middleware | `api-adapters/*/middleware/` |
| Is a shared HTTP concern (DTOs, errors, pagination) | `api-adapters/common/` |

If the answer is ambiguous, consult `DESIGN.md` § 3 (Design Patterns) and `ARCHITECTURE.md` § 6 (Crate Responsibilities). If still ambiguous, open a discussion before writing code.

---

## 3. Naming Conventions

### Adapter Naming

Concrete adapters are prefixed with the technology they implement:

| Technology | Prefix | Examples |
|-----------|--------|---------|
| PostgreSQL | `Pg` | `PgBoardRepository`, `PgPostRepository`, `PgUserRepository` |
| SQLite | `Sqlite` | `SqliteBoardRepository` |
| SurrealDB | `Surreal` | `SurrealBoardRepository` |
| AWS S3 / MinIO | `S3` | `S3MediaStorage` |
| Local filesystem | `LocalFs` | `LocalFsMediaStorage` |
| Cloudflare R2 | `R2` | `R2MediaStorage` |
| JWT | `Jwt` | `JwtAuthProvider` |
| Cookie session | `Cookie` | `CookieAuthProvider` |
| OIDC | `Oidc` | `OidcAuthProvider` |
| Redis | `Redis` | `RedisRateLimiter` |
| In-memory | `InMemory` | `InMemoryRateLimiter` |
| No-op (test stub) | `Noop` | `NoopRateLimiter`, `NoopMediaStorage` |
| hCaptcha | `HCaptcha` | `HCaptchaCaptchaVerifier` |
| reCAPTCHA | `ReCaptcha` | `ReCaptchaCaptchaVerifier` |
| MeiliSearch | `MeiliSearch` | `MeiliSearchIndex` |
| ActivityPub | `ActivityPub` | `ActivityPubFederationSync` |

### Port Method Naming

All repository ports follow this vocabulary. Implementations must use these exact names — do not rename methods:

| Operation | Method | Signature pattern |
|-----------|--------|-------------------|
| Fetch by primary key | `find_by_id` | `async fn find_by_id(&self, id: XxxId) -> Result<Xxx, DomainError>` |
| Fetch by unique field | `find_by_<field>` | e.g., `find_by_slug`, `find_by_username` |
| Fetch paginated collection | `find_all` | `async fn find_all(&self, page: Page) -> Result<Paginated<T>, DomainError>` |
| Fetch filtered collection | `find_by_<field>` | Returns `Vec<T>` or `Paginated<T>` |
| Persist (insert or update) | `save` | `async fn save(&self, entity: &Xxx) -> Result<(), DomainError>` |
| Delete | `delete` | `async fn delete(&self, id: XxxId) -> Result<(), DomainError>` |
| Domain action (not CRUD) | descriptive verb | `bump`, `prune_oldest`, `expire`, `resolve`, `deactivate` |

### Type Naming

| Category | Convention | Examples |
|----------|-----------|---------|
| Domain models | `UpperCamelCase` noun | `Board`, `Thread`, `Post`, `BoardConfig` |
| Port traits | `<Entity><Responsibility>` | `BoardRepository`, `MediaStorage`, `AuthProvider` |
| Value objects | Descriptive noun | `BoardId`, `IpHash`, `MediaKey`, `FileSizeKb`, `Slug` |
| Service error enums | `<Service>Error` | `PostError`, `BoardError`, `ModerationError` |
| Domain errors | Descriptive | `DomainError`, `ValidationError`, `NotFoundError` |
| API error enum | `ApiError` | (singular, shared) |
| Config structs | `<Scope>Config` | `BoardConfig`, `ServerConfig` |
| Infrastructure settings | `Settings` | (singular) |
| Service structs | `<Name>Service` | `PostService`, `BoardService` |
| Handler functions | `<action>_<entity>` | `create_post`, `list_boards`, `update_board_config` |
| Middleware functions | descriptive | `require_auth`, `load_board_config`, `attach_request_id` |

### Module & File Naming

- Module directories: `snake_case/`
- Rust files: `snake_case.rs`
- Adapter modules: named after technology — `postgres/`, `sqlite/`, `jwt_bearer/`, `s3.rs`, `local_fs.rs`
- Never name a file after the trait it implements: prefer `board_repository.rs` over `repository.rs`
- Template files: `snake_case.html`

### Feature Flag Naming

| Category | Pattern | Examples |
|----------|---------|---------|
| Web framework | `web-<name>` | `web-axum`, `web-actix`, `web-poem` |
| Database | `db-<name>` | `db-postgres`, `db-sqlite`, `db-surrealdb` |
| Auth mechanism | `auth-<name>` | `auth-jwt`, `auth-cookie`, `auth-oidc` |
| Media storage | `media-<name>` | `media-s3`, `media-local`, `media-r2` |
| Media processing | descriptive | `video`, `documents` |
| Rate limiting / caching | descriptive | `redis` |
| Anti-spam | `spam-<name>` | `spam-hcaptcha`, `spam-recaptcha` |
| Search | `search-<name>` | `search-meilisearch` |
| Federation | `federation-<name>` | `federation-activitypub` |

---

## 4. Settings vs. BoardConfig vs. Domain Constants

This is the most common source of confusion when adding new configuration. Apply this decision tree:

```
Does this value vary per board?
  YES → BoardConfig field + DB migration + dashboard UI
  NO → Does it vary per deployment (infrastructure choice)?
    YES → Settings field + env var + .env.example entry
    NO → Is it a fixed business rule that never changes?
      YES → Domain constant (const in domains/models.rs) or encoded in the type system
      NO → Discuss before adding anywhere
```

**Examples**:
- "Rate limit posts per window" → `BoardConfig` (varies per board)
- "Redis URL" → `Settings` (varies per deployment)
- "Thumbnail width" → `Settings` (infrastructure default, operator may tune)
- "Slug regex" → Domain constant (business rule, never changes)
- "EXIF stripping is always done" → Not a setting at all — it's a code invariant
- "JWT token TTL" → `Settings` (infrastructure, same for all users of a deployment)
- "Max post length" → `BoardConfig` (board owners may configure different limits)

---

## 5. Error Handling

### Boundary Mapping

Errors are transformed at every crate boundary. No error type crosses a boundary unmodified.

```
Adapter crate errors (sqlx::Error, aws::Error, ...)
    │  caught inside adapter, mapped to:
    ▼
DomainError (in domains/errors.rs)
    │  wrapped with context in services, mapped to:
    ▼
ServiceError (*Error per service, in services/*/errors.rs)
    │  caught in handlers, mapped to:
    ▼
ApiError (in api-adapters/common/errors.rs → HTTP response)
```

### Error Type Rules

- Adapter implementations never return `sqlx::Error`, `aws_sdk_s3::Error`, etc. to callers. They return `DomainError`.
- Services never expose `anyhow::Error` in public method signatures. Public methods return typed service error enums.
- `anyhow::Error` is used internally (via `.context("...")`) for stack context within service and adapter methods.
- `ApiError` maps to HTTP status codes. User-facing messages are generic. Internal details are in the log, never in the response body.

### `From` Impls

Implement `From<ServiceError> for ApiError` for each service error type. This keeps handler code clean:

```rust
// In handlers — no explicit mapping needed
let post = post_service.create_post(draft, &board_config).await?;
//                                                                ^^^ ApiError via From
```

### Logging at Error Points

- Adapter: `tracing::error!(error = %e, "failed to query posts")` before mapping to `DomainError`
- Service: `tracing::warn!` for expected failures (validation, rate limit, not found); `tracing::error!` for unexpected internal failures
- Handler: `tracing::warn!` or `tracing::error!` as appropriate before returning `ApiError`

The `request_id` field is automatically included in all log lines within a request span.

---

## 6. Async Conventions

### No `async-trait` Macro

All port traits use RPITIT. This is a hard rule.

```rust
// CORRECT
pub trait BoardRepository: Send + Sync + 'static {
    async fn find_by_slug(&self, slug: &Slug) -> Result<Board, DomainError>;
}

// NEVER DO THIS
#[async_trait::async_trait]
pub trait BoardRepository {
    async fn find_by_slug(&self, slug: &Slug) -> Result<Board, DomainError>;
}
```

### Trait Bounds

All port traits carry `Send + Sync + 'static`. Services and the composition root require this for Tokio task compatibility.

### No `Arc<dyn Trait>` in Service Structs

Services hold their port implementations as owned values or generic type parameters. `Arc<dyn Trait>` is not used inside service structs. If a value needs to be shared (e.g., `BoardConfigCache`), it is wrapped in `Arc` externally and passed to the service by value.

### Tokio Runtime

The binary uses `#[tokio::main]` with the multi-thread runtime. All async operations are compatible with the multi-thread scheduler (`Send` futures only). No blocking operations inside async functions — use `tokio::task::spawn_blocking` for CPU-heavy work (image processing, argon2 hashing).

---

## 7. Service Generic Type Alias Pattern

When a service has more than four type parameters, define a concrete type alias in `composition.rs` to keep the code readable:

```rust
// In composition.rs — readable alias for the concrete instantiation
type AppPostService = PostService<
    PgPostRepository,
    PgThreadRepository,
    PgBanRepository,
    S3MediaStorage,
    RedisRateLimiter,
    ImageMediaProcessor,
>;

// Use the alias everywhere in composition.rs
let post_service: AppPostService = PostService::new(
    post_repo, thread_repo, ban_repo, media_storage, rate_limiter, media_processor,
);
```

This pattern keeps the service generic for testing (mock types are used in unit tests) while making the concrete wiring readable.

---

## 8. BoardConfig Change Checklist

Adding a new behavioral toggle must follow this sequence. All steps are required.

1. **Add field to `BoardConfig`** in `domains/models.rs`. Choose a conservative default (safest behavior for a new board).
2. **Add column to migration** — new file `V0NN__add_<field>_to_board_configs.sql`. Include `DEFAULT` clause matching the Rust default.
3. **Add branch in service** — read `board_config.<field>` in the relevant service method(s). Keep the branch local and obvious.
4. **Add UI control** — expose the field in `board_owner_dashboard.html` with a label, description of what it does, current value, and valid range.
5. **Add DTO field** — add to `BoardConfigUpdate` in `api-adapters/common/dtos.rs`.
6. **Add unit tests** — one test for the service branch with the toggle `true`, one with `false`. Use the `board_configs.rs` fixture as base.
7. **Update `.env.example`** — only if the toggle exposes a new infrastructure capability that requires configuration.
8. **Update `PORTS.md`** — only if the toggle requires a new port (e.g., CAPTCHA).

Never add a behavioral toggle as:
- An environment variable (use `BoardConfig`)
- A `Settings` field (use `BoardConfig`)
- A Cargo feature flag (use `BoardConfig`)
- A global or thread-local variable (use `BoardConfig`)

---

## 9. Testing Conventions

### Test Categories

| Category | Location | Infrastructure | Purpose |
|----------|----------|---------------|---------|
| Unit tests | `tests/unit/` or `src/` inline | None (mock ports) | Service logic, domain validation |
| Adapter contract tests | `tests/adapter_contracts/` | Real (testcontainers) | Verify adapter satisfies port contract |
| Integration tests | `tests/integration/` | Full stack (testcontainers) | End-to-end HTTP flows |
| Snapshot tests | `tests/snapshots/` | None | Template rendering regression |

### Unit Tests (Services)

Use `mockall` for all port mocks. Mock names follow `Mock<PortName>`: `MockBoardRepository`, `MockRateLimiter`.

Every service method requires tests for:
- Happy path (all operations succeed, board config at defaults)
- Each error variant the method can return
- Each `BoardConfig` boolean field that affects the method — one test per field, both `true` and `false` states

Use the pre-built `BoardConfig` fixtures from `tests/fixtures/board_configs.rs`. Do not construct `BoardConfig` inline in tests unless the test is specifically about config construction.

```rust
// Good — uses named fixture
let config = fixtures::board_configs::strict();
let result = post_service.create_post(draft, &config).await;

// Avoid — constructing inline obscures what the test is about
let config = BoardConfig { rate_limit_enabled: true, rate_limit_posts: 1, ..Default::default() };
```

### Adapter Contract Tests

Every adapter implementing a port must have a contract test file in `tests/adapter_contracts/`. Contract tests:
- Use `testcontainers` to spin up real infrastructure (Postgres, Redis, MinIO)
- Test every method in the port trait
- Verify not just that methods compile and run, but that they satisfy behavioral expectations (e.g., `save()` is idempotent, `find_by_id()` returns `NotFound` for missing IDs, `prune_oldest()` actually deletes and returns the correct count)

When a new adapter is added (e.g., `SqliteBoardRepository`), it must pass the same contract tests as the existing adapter.

### Integration Tests

Tests live in `tests/integration/`. They use `reqwest` to make HTTP requests against a test server started with the real composition root. They verify database state after operations.

Integration tests must not rely on test execution order. Each test creates its own data and cleans up after itself (or uses transactions rolled back at test end).

### Fixture Conventions

Pre-built domain fixtures in `tests/fixtures/`:
- `board_configs::permissive()` — all restrictions disabled, no rate limit, no spam filter
- `board_configs::strict()` — all restrictions enabled at their defaults
- `board_configs::nsfw()` — NSFW flag set, all others at default
- `board_configs::no_media()` — empty allowed_mimes (no attachments)

Fixtures for posts, threads, boards, and users are generated via `fake::Faker` in `tests/fixtures/` modules. Never hardcode UUIDs or timestamps in fixtures — use generated values.

### Snapshot Tests

Askama template snapshots use `insta`. Run `cargo insta review` to review and accept changes intentionally. Snapshot changes in a PR must be accompanied by a description of what changed in the template and why.

---

## 10. Documentation Conventions

### Code Comment Prefixes

| Prefix | Meaning |
|--------|---------|
| `// INVARIANT:` | Documents an architectural constraint or domain rule that must never be violated |
| `// SAFETY:` | Explains why an `unsafe` block is sound (prefer safe Rust; always document when unsafe is used) |
| `// TODO(v1.1):` | Planned addition for a specific version milestone |
| `// FIXME:` | Known defect with a description of the issue |
| `// PERF:` | Performance note or optimization opportunity |

### Doc Comment Requirements

All `///` doc comments must document:
- **For types**: What this type represents; when you would use it
- **For port trait methods**: What the method does, what it returns on success, which `DomainError` variant it returns on each failure mode
- **For service methods**: What the business operation does, what `BoardConfig` fields affect behavior, what errors are returned and when
- **For public functions**: Parameters, return value, error conditions

### Module-Level Comments

Every `.rs` file starts with a `//!` module doc comment (for `lib.rs` and `mod.rs` files) or a `//` comment block explaining:
- What this module is responsible for
- Why it exists as a separate module (if non-obvious)
- Any important design decisions local to the module

---

## 11. Git & PR Conventions

### Branch Naming

- `feat/<short-description>` — new feature or capability
- `fix/<short-description>` — bug fix
- `refactor/<short-description>` — structural change with no behavior change
- `docs/<short-description>` — documentation only
- `chore/<short-description>` — tooling, CI, dependencies

### Commit Messages

First line: imperative mood, ≤72 chars, present tense (`Add UserRepository port`, not `Added` or `Adding`).
Body (optional): explain *why*, not *what*. Reference ADRs or document sections when making architectural decisions.

### PR Requirements

- All CI checks pass (fmt, clippy, test matrix, audit)
- No `#[allow(clippy::...)]` without explanation
- New ports added to `PORTS.md`
- New architectural decisions added to `DECISIONS.md`
- New `BoardConfig` fields follow the full change checklist (§ 8)
- Test coverage does not decrease
