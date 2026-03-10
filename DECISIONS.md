# DECISIONS.md
# rusty-board — Architecture Decision Records

> **A permanent log of significant decisions, what alternatives were considered, and why the chosen approach was selected. Consult this before reopening a settled question.**

Each record follows the format: Context → Decision → Alternatives Considered → Rationale → Consequences → Status.

---

## ADR-001: Compile-time Adapter Selection via Cargo Features

**Date**: Project inception
**Status**: Accepted — foundational

### Context

The system needs to support multiple database backends, web frameworks, auth mechanisms, and media storage backends over its lifetime. A decision was needed about whether to resolve which implementation to use at compile time or at runtime.

### Decision

Adapter selection is compile-time via Cargo feature flags. The compiler selects concrete types based on active features. Services are generic over port traits and are monomorphized to concrete types in `composition.rs`.

### Alternatives Considered

**Runtime plugin loading (dlopen / dylib)**: Would allow true runtime swapping without recompile. Rejected because: Rust's dynamic library story is immature and unsafe-heavy, debugging across FFI boundaries is painful, and the operational benefit (swap without recompile) is not needed — operators can redeploy.

**`Arc<dyn Trait>` for all ports**: Dynamic dispatch, selected at runtime via config string. Rejected because: adds vtable overhead to every port call in the hot path, loses compiler verification that the selected implementation is complete, and makes service code dependent on runtime strings that cannot be type-checked.

**Enum dispatch** (`match adapter { Postgres(r) => r.find(id), Sqlite(r) => r.find(id) }`): Would keep everything in one binary with runtime selection. Rejected because: requires all adapters to be compiled into every binary regardless of use, every new adapter adds a variant to every enum in the hot path, and the ergonomic cost is high.

### Rationale

Rust's trait system was designed for exactly this use case. The compiler can verify that `PgBoardRepository` satisfies `BoardRepository` at compile time. Monomorphization eliminates all dispatch overhead. The operational constraint (require recompile to change adapter) is acceptable — this is not a system where database engines are swapped at runtime.

### Consequences

- Binaries are lean: only the selected adapters are compiled in.
- Compiler catches incomplete or incorrect adapter implementations.
- Swapping adapters requires a recompile and redeploy.
- `composition.rs` grows a new `#[cfg]` branch for each new adapter.
- CI must test feature combinations, not just the default.

---

## ADR-002: Runtime Behavioral Configuration via BoardConfig

**Date**: Project inception
**Status**: Accepted — foundational

### Context

The system needs per-board behavioral customization (rate limits, spam settings, content rules, feature toggles). A decision was needed about how operators control this behavior.

### Decision

All per-board behavioral parameters are stored in a `BoardConfig` struct in the database. Services receive `BoardConfig` as a parameter and branch on its fields. Operators change behavior through admin and board owner dashboards. No recompile required.

### Alternatives Considered

**Feature flags for behaviors**: e.g., `--features spam-filter,rate-limiting`. Rejected because: per-board configuration cannot be done with binary-level feature flags. You cannot have board A with rate limiting and board B without it in the same binary.

**Environment variables for behaviors**: e.g., `RATE_LIMIT_ENABLED=true`. Rejected because: applies globally, not per-board. Changing behavior requires a redeploy. Operators cannot change it through a UI.

**Database-driven plugins**: Full plugin system where behaviors are loaded from DB at runtime. Rejected because: dramatically increases complexity with no meaningful benefit over a well-designed config struct.

### Rationale

`BoardConfig` gives operators immediate, per-board control without touching the deployment. The service layer's branching on config fields is the natural Rust idiom for this pattern. The struct is cheap to read (a field access after one cached DB lookup) and cheap to extend (add a field, add a migration, add a branch, add a UI control).

### Consequences

- Service methods become more complex as `BoardConfig` fields increase.
- New behavioral toggles require a migration, service branch, UI control, and DTO update.
- Operators have immediate, granular, per-board control.
- Config is human-readable (it's a database row) and auditable.
- Cache invalidation is a concern for multi-instance deployments (solved with 60s TTL + immediate invalidation on write).

---

## ADR-003: Native Async Traits (RPITIT) — No `async-trait` Macro

**Date**: Project inception
**Status**: Accepted

### Context

Port traits require async methods. Two options existed: the `async-trait` procedural macro (stable, widely used) or native return-position `impl Trait` in traits (RPITIT, stable in Rust 1.75+).

### Decision

All port traits use native async (RPITIT). The `async-trait` macro is not used anywhere in the codebase.

### Alternatives Considered

**`async-trait` macro**: Would support older Rust versions (pre-1.75). Adds a boxing overhead per async call (`Box<dyn Future<...>>`). Adds a macro dependency. Creates minor ergonomic friction in IDE tooling.

### Rationale

The minimum Rust version is 1.75+, which fully supports RPITIT. Native async traits produce no boxing overhead. The macro dependency is eliminated. The trait syntax is identical to what Rust developers expect from synchronous traits, improving readability.

Object safety (`dyn Trait`) is not needed — services are generic over concrete types, not dynamically dispatched. If object safety becomes necessary in a future context, that decision will be made explicitly.

### Consequences

- Minimum Rust version is 1.75+.
- No `async-trait` dependency in any crate.
- Port traits are concise and idiomatic.
- Object safety of port traits is not guaranteed (this is intentional).

---

## ADR-004: Board Ownership as a Relationship, Not a Role

**Date**: Design phase
**Status**: Accepted

### Context

The system needs a concept of "board owner" — a user who can manage `BoardConfig` for specific boards. The question was whether this should be a role variant (`Role::BoardOwner`) or a database relationship.

### Decision

Board ownership is stored in a `board_owners` join table (`board_id`, `user_id`). It is not a `Role` variant. The `CurrentUser` context struct carries a `Vec<BoardId>` of owned boards for the authenticated user.

### Alternatives Considered

**`Role::BoardOwner` variant**: Simple to implement. Rejected because: a role applies globally, not per-board. A user who owns two boards cannot be distinguished from a user who owns none with a role alone. The role would need to carry a board ID to be useful, at which point it becomes a relationship anyway.

**`Role::BoardOwner(Vec<BoardId>)` enum variant with payload**: Closer to correct. Rejected because: encoding a relationship in a JWT claim creates consistency problems (the token becomes stale when board ownership changes), complicates token refresh, and requires special handling throughout the auth path.

### Rationale

Board ownership is inherently a many-to-many relationship between users and boards. A join table is the correct relational model. Loading the owned board IDs alongside user authentication is a single query that can be cached in the `CurrentUser` context for the life of the request.

### Consequences

- A `board_owners` migration is required.
- `UserRepository` must provide a method to fetch owned board IDs for a user.
- `CurrentUser` carries `Vec<BoardId>` which grows with the number of owned boards.
- Ownership changes take effect immediately (next request after the change, no token refresh needed).
- The `Role` enum stays clean and represents site-wide privilege level only.

---

## ADR-005: In-Process BoardConfig Caching with TTL

**Date**: Design phase
**Status**: Accepted

### Context

`BoardConfig` is read from the database on every post creation and many other operations. Reading it from the database on every request would add a DB round-trip to every operation. A caching strategy was needed.

### Decision

`BoardConfig` is cached in-process using a `DashMap<BoardId, (BoardConfig, Instant)>` with a 60-second TTL. The cache is invalidated immediately when a board's config is updated via the dashboard.

### Alternatives Considered

**No caching, read from DB every time**: Correct by construction. Rejected because: adds 5–15ms to every post creation for a value that changes rarely (maybe once per week). At 500 concurrent users, this adds meaningful DB load for no benefit.

**Redis-based shared cache**: Consistent across all instances instantly. Rejected for v1.0 because: adds Redis as a hard dependency even when not using the `redis` feature, adds a cache invalidation protocol, and 60 seconds of stale config is acceptable for behavioral toggles.

**Read on startup, reload on signal**: Simple. Rejected because: doesn't allow per-board configuration (all boards would share one config) and doesn't support dashboard-driven updates without process restart.

### Rationale

In-process caching with a short TTL is the simplest correct solution for v1.0. The `BoardConfigCache` struct is behind an interface that could be replaced with a Redis-backed cache in the future without touching service code. The 60-second stale window is acceptable for the nature of the settings involved — an operator enabling CAPTCHA on a board can wait up to 60 seconds for it to take effect on all instances.

### Consequences

- Multi-instance deployments may have up to 60 seconds of config skew between instances.
- Cache takes minimal memory (one struct per board, boards are expected to number in the hundreds at most).
- Config updates are immediate on the instance that handled the update.
- The `BoardConfigCache` interface allows future migration to a shared cache.

---

## ADR-006: Separate `MediaProcessor` and `MediaStorage` Ports

**Date**: Design phase
**Status**: Accepted

### Context

Media handling involves two distinct concerns: processing (thumbnail generation, EXIF stripping, validation) and storage (upload, retrieve, URL generation). The question was whether these should be one port or two.

### Decision

`MediaProcessor` and `MediaStorage` are separate ports with separate adapter axes. Processing adapters are selected based on media format capabilities (`video`, `documents` features). Storage adapters are selected based on storage backend (`media-s3`, `media-local` features).

### Alternatives Considered

**Single `MediaHandler` port** combining process and store: Simpler composition. Rejected because: forces a 1:1 relationship between processor and storage — you could not use ffmpeg processing with local filesystem storage, for example. Every combination would need a separate adapter.

**Integrated in `PostService` with direct library calls**: No ports at all for media. Rejected because: `ffmpeg-next` and `pdfium-render` are heavy dependencies that need to be feature-gated. If they were called directly from service code, the service crate would need to depend on them, violating the principle that services depend only on `domains`.

### Rationale

The two ports vary independently. Any storage backend can be paired with any processing capability. Keeping them separate means two clean, small adapter interfaces instead of one large combined one. The facade in `storage-adapters/src/media/mod.rs` composes them internally.

### Consequences

- Two port traits to implement for each combination of capabilities.
- Composition is more explicit but also more flexible.
- `PostService` depends on both ports and orchestrates the pipeline.
- Adding a new storage backend does not require touching the processor adapters, and vice versa.

---

## ADR-007: Askama for Server-Side Templates

**Date**: Design phase
**Status**: Accepted

### Context

The system needs to render HTML for board views, thread views, catalog, dashboards, and components. Options ranged from a JavaScript SPA to pure API responses to server-side templates.

### Decision

Askama compile-time templates with minimal JavaScript enhancement. All pages are fully functional without JavaScript. JSON API endpoints provide machine-readable equivalents.

### Alternatives Considered

**React/Vue SPA with API backend**: Full client-side rendering. Rejected because: imageboard users have high privacy expectations and often use Tor Browser or JavaScript-blocking extensions. A JS-required interface excludes a meaningful portion of the target user base. Also adds a JavaScript build pipeline (node, webpack/vite, npm dependencies) to a Rust project, increasing operational complexity significantly.

**`minijinja` or `tera` (runtime templates)**: Flexible, dynamic. Rejected because: runtime template errors are harder to catch. Askama's compile-time checking means template errors are build errors, and type-safe template parameters prevent a class of runtime panics.

**Plain HTML with no templating**: Not viable at this scale.

### Rationale

Askama is the natural choice for a Rust project that values compile-time correctness. Templates are checked at build time, parameters are typed, and escaping is automatic. The no-JS requirement for core functionality is a non-negotiable UX decision for an imageboard. Minimal JS enhancement is additive and does not break anything for users without it.

### Consequences

- Template errors are build errors — caught early.
- XSS via template output is structurally prevented by automatic escaping.
- No JavaScript build pipeline in the project.
- All pages degrade gracefully without JS.
- Real-time features (v1.3 WebSockets) will be additive JS enhancement, not a replacement.

---

## ADR-008: `UserRepository` as a Separate Port

**Date**: Design review
**Status**: Accepted

### Context

Initial design iterations did not include a `UserRepository` port. Users (moderators, admins) clearly need persistence, but the scope of what the repository needs to do was not initially specified.

### Decision

`UserRepository` is a first-class port with methods covering user creation (admin only), lookup by ID and username, role updates, and owned board ID retrieval. It is used by `UserService` (login, account management) and `ModerationService` (audit log actor lookup).

### Rationale

Without `UserRepository`, user persistence would either leak into the auth adapter (wrong — auth should only handle tokens and hashing, not persistence) or into service code as direct DB calls (wrong — violates the ports & adapters pattern). A clean port keeps the user domain concern in the right place.

### Consequences

- `PgUserRepository` is a v1.0 adapter alongside the other Postgres repositories.
- `UserService` is added to the service layer.
- `CurrentUser` context struct is populated from a `UserRepository` lookup during auth middleware.
- `board_owners` query is part of the user repository, not a separate port.

---

## ADR-009: MediaStorage URL TTL as an Explicit Parameter

**Date**: Design review
**Status**: Accepted

### Context

The original `MediaStorage::get_url` signature returned a `Url` with no TTL consideration. S3 presigned URLs expire. Local filesystem URLs do not. This asymmetry was unaddressed.

### Decision

`MediaStorage::get_url` accepts an explicit `ttl: Duration` parameter. S3 implementations use it to set presigned URL expiry. Local filesystem implementations ignore it and return a static path.

### Alternatives Considered

**TTL configured at adapter construction time**: S3 adapter stores a default TTL. Rejected because: different callers may need different TTLs (inline media in a thread view vs. a download link), and the TTL belongs with the caller's intent, not the adapter's configuration.

**Separate `get_presigned_url(key, ttl)` and `get_url(key)` methods**: More precise. Rejected because: forces callers to know which storage backend is in use, which breaks the port abstraction.

### Rationale

Making TTL explicit at the call site keeps the port abstraction clean. The caller specifies the intent (how long this URL should be valid for). The adapter interprets that intent in whatever way makes sense for its backend. `Settings.media_url_ttl_secs` provides the default value for most callers.

### Consequences

- All callers of `get_url` must provide a TTL.
- `Settings` must include `media_url_ttl_secs`.
- S3 URLs respect the TTL. Local filesystem URLs are always "permanent" (static paths).
- Future storage backends with TTL support (e.g., Cloudflare R2) work naturally.

---

## ADR-010: `CurrentUser` Exposes `user_id()` as a Method, Not a Public Field

**Date**: Phase 1 implementation
**Status**: Accepted

### Context

During implementation of auth middleware and moderation handlers, `CurrentUser` was initially designed with a public field `user_id: UserId`. Handlers and middleware accessed it as `current_user.user_id`. This worked until refactoring introduced the need for field-level validation and consistency with the `claims: Claims` source.

### Decision

`CurrentUser.user_id` is exposed via an accessor method `user_id() -> UserId` rather than a public field. All handler code uses `current_user.user_id()`.

### Rationale

Methods allow the implementation detail (where the ID is stored, whether it's copied or cloned) to change without breaking callers. They also make it explicit that you're reading a derived or validated value, not a raw struct field. Consistency: all `CurrentUser` value extraction goes through methods (`user_id()`, `role()`, `is_moderator_or_above()`, `can_manage_board_config()`).

### Consequences

- Handlers call `current_user.user_id()` — slightly more verbose than field access, but consistent.
- `CurrentUser::from_claims(claims)` is the single construction path from JWT verification.
- Adding future derived getters (e.g. `is_admin()`) requires no handler changes.

---

## ADR-011: Moderation Handlers Pass `actor_id: UserId`, Not `&CurrentUser`

**Date**: Phase 1 implementation
**Status**: Accepted

### Context

Initial moderation handler stubs passed `&current: &CurrentUser` directly to service methods. The `ModerationService` methods signature was `delete_post(post_id, actor_id: UserId)` — the service needs only the actor's ID for audit logging, not the full `CurrentUser` context.

### Decision

All moderation service methods accept `actor_id: UserId` (not `&CurrentUser`). Handlers extract `current_user.user_id()` and pass that scalar value.

### Rationale

Services must not depend on HTTP-layer types. `CurrentUser` is an Axum extractor — a web adapter concept. Passing it into `ModerationService` would violate the hexagonal boundary. The service needs only the actor's ID for audit log entries; it does not need the role or other HTTP-session context.

### Consequences

- Audit entries record `actor_id: UserId` from the JWT claim.
- Services remain completely decoupled from HTTP types.
- Any future non-HTTP caller (CLI, background job) can invoke moderation services without constructing a `CurrentUser`.

---

## ADR-012: Testing Feature Flag on `domains` Crate

**Date**: Phase 10 testing
**Status**: Accepted

### Context

`mockall::automock` generates `MockXxx` types for each port trait. These are needed in test code in `crates/services` and `crates/integration-tests` but must not appear in production builds.

### Decision

The `domains` crate has a `testing` feature flag: `[features] testing = ["dep:mockall"]`. The `automock` attribute on all port traits uses `#[cfg_attr(any(test, feature = "testing"), mockall::automock)]`. External test crates add `domains = { features = ["testing"] }` to their `dev-dependencies`.

### Alternatives Considered

**`mockall` as a dev-dependency of `domains`**: Mocks are then only available inside `domains` tests, not to `services` or `integration-tests`. Rejected.

**Re-export mocks from `domains`**: Possible but pollutes the public API of `domains` in production. Rejected.

### Rationale

The `testing` feature cleanly separates production and test compilation. Cargo ensures the feature is never active in release builds unless explicitly requested. External crates can opt in via `dev-dependencies` without affecting the `services` or `api-adapters` release build graph.

### Consequences

- `domains` production builds: no `mockall` dependency.
- Test builds: `MockBoardRepository`, `MockThreadRepository`, etc. are available to any crate that adds `domains = { features = ["testing"] }` to its dev-dependencies.
- All 12 port trait mocks are generated automatically; no manual mock maintenance.
