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

---

## ADR-013: `create_post` is a Browser-Form Endpoint — Always 303

**Date**: v1.0 implementation
**Status**: Accepted

### Context

`POST /board/:slug/post` accepts multipart form data (the native encoding for HTML `<form>` submissions with file uploads). A decision was needed about the success response: return `201 Created` with a JSON body (API style) or `303 See Other` with a `Location` header (browser form style).

### Decision

`create_post` always returns `303 See Other` redirecting to `/board/:slug/thread/:id#post-:number`. There is no JSON response path for this endpoint. Programmatic readers should use the read endpoints.

### Alternatives Considered

**Content negotiation (`Accept: application/json` → 201, otherwise → 303)**: Seemed appealing for test ergonomics. Rejected because it was adopted to make tests pass, not because it was the right design. The handler is fundamentally a browser-form endpoint; adding a JSON path would require the `WantsJson` extractor before the `Multipart` body extractor, complicating the handler signature and splitting the conceptual model of the endpoint in two.

**Always return 201 with JSON**: API-friendly but breaks the browser form flow. A browser submitting a form to an endpoint that returns `201` will display the JSON as a blank page, not navigate to the new post.

### Rationale

Browser form endpoints must redirect on success to avoid double-submission on page reload (Post/Redirect/Get pattern). The redirect to the thread anchor (`#post-N`) also scrolls the user to their new post, which is the correct UX. Tests that expected `201` had the wrong expectation; the correct test asserts `303` and verifies the `Location` header.

The `WantsJson` extractor (`middleware/accept.rs`) exists and is used by read endpoints (`list_flags`, `list_bans`) that legitimately serve both browsers and API consumers. It is intentionally not used on write-form endpoints.

### Consequences

- `POST /board/:slug/post` always returns `303 See Other`.
- Tests for this endpoint assert `StatusCode::SEE_OTHER` and check the `Location` header.
- API consumers wanting to read post data use `GET /board/:slug/thread/:id`.
- The Post/Redirect/Get pattern prevents double-post on browser refresh.

---

## ADR-014: `WantsJson` Extractor for Dual-Mode Read Endpoints

**Date**: v1.0 implementation
**Status**: Accepted

### Context

Some read endpoints (`list_flags`, `list_bans`) serve both browser dashboard requests (expecting HTML) and API/test clients (expecting JSON). A reliable mechanism was needed to detect JSON preference before consuming the request body.

### Decision

A `WantsJson(bool)` newtype extractor implements `FromRequestParts` (not `FromRequest`). It reads the `Accept` header directly from `parts.headers` and returns `WantsJson(true)` when `application/json` is present. Handlers pattern-match on it to choose their response type.

### Alternatives Considered

**`HeaderMap` extractor**: Available in Axum but unreliable when combined with body-consuming extractors like `Multipart`. The ordering of extractors matters — body extractors consume the request, after which header-only extractors may not run correctly. `FromRequestParts` is guaranteed to run before any body extraction.

**Separate routes** (`GET /mod/flags` for HTML, `GET /api/flags` for JSON): Clean but duplicates routing. Rejected because the handlers share all logic except the final serialization step.

**Query parameter** (`?format=json`): Works but non-standard. The `Accept` header is the correct HTTP mechanism for content negotiation.

### Rationale

`FromRequestParts` is guaranteed by Axum to execute before body-consuming extractors. Declaring the associated `Rejection` as `Infallible` (the extractor never fails — a missing or unrecognised `Accept` header defaults to `false`) means handler signatures remain clean. The `WantsJson` name is explicit about its single purpose.

### Consequences

- `accept.rs` is a small, self-contained module with no dependencies beyond `axum`.
- All dual-mode handlers use the same pattern consistently.
- `WantsJson` is deliberately not used on write-form endpoints (see ADR-013).
- axum 0.8 RPITIT trait syntax is used (plain `async fn`, no `#[async_trait]`, no associated `Future` type).

---

## ADR-015: Four Roles — Admin, Janitor, BoardOwner, BoardVolunteer

**Date**: v1.0 finalisation
**Status**: Accepted — supersedes earlier three-role description in documentation

### Context

Early documentation described three roles: Janitor, Moderator, Admin. The codebase evolved to four roles with a `board_owner` join table. The naming was inconsistent — `Janitor` referred to board-scoped deletion and `Moderator` to site-wide authority, which is backwards relative to common imageboard terminology.

### Decision

The four roles are, in order of decreasing privilege:

| Role | String | Scope |
|------|--------|-------|
| `Admin` | `"admin"` | Full site access, user management, all logs |
| `Janitor` | `"janitor"` | Site-wide moderation on all boards |
| `BoardOwner` | `"board_owner"` | Manages specific boards: config + moderation |
| `BoardVolunteer` | `"board_volunteer"` | Moderation on assigned boards only |

**Moderation is an action, not a role.** Delete, ban, sticky, close, and flag-resolve are actions that any sufficiently-privileged role may perform depending on context (site-wide vs. board-scoped).

### Migration

DB migration `013_rename_roles.sql` renames:
- `"moderator"` → `"janitor"` (was the global site moderator — renamed to match terminology)
- `"janitor"` → `"board_volunteer"` (was board-scoped deletion-only — renamed to reflect the volunteer model)

### Consequences

- `Role::Moderator` is removed from the codebase. All tests and documentation use the new names.
- `can_moderate()` = Janitor | Admin (site-wide authority)
- `can_delete()` = BoardVolunteer | BoardOwner | Janitor | Admin (any staff with board-level access)
- `is_moderator_or_above()` = Janitor | Admin (kept for route guards that require site-wide authority)

---

## ADR-016: Login Redirects to Role-Specific Dashboard

**Date**: v1.0 finalisation
**Status**: Accepted

### Context

After a successful login, where should the user be redirected? An imageboard has four staff roles with distinct interfaces. Sending everyone to the same URL and then redirecting is an extra round-trip that also leaks role information via redirect chain.

### Decision

The login page JavaScript decodes the JWT payload (base64url, no verification needed client-side — the server already verified it) to read the `role` claim and redirects directly:

| Role | Dashboard URL |
|------|--------------|
| `admin` | `/admin/dashboard` |
| `janitor` | `/janitor/dashboard` |
| `board_owner` | `/board-owner/dashboard` |
| `board_volunteer` | `/volunteer/dashboard` |
| `user` | `/user/dashboard` |

`/mod/dashboard` is kept as a server-side redirect shim. Navigating there with a valid session issues a `303 See Other` to the correct dashboard. This supports external links and bookmarks to the old URL.

### Consequences

- Login is a single POST → token → immediate redirect with no intermediate page.
- Each role lands on a page appropriate to their capabilities without seeing buttons they cannot use.
- Adding a new role requires adding one line to the login JS redirect table and one route.

---

## ADR-017: User Role — Registered Accounts for the Staff Pipeline

**Date**: v1.1 planning
**Status**: Accepted

### Context

The v1.0 system has a gap: there is no path from "anonymous visitor" to "board owner or volunteer" without admin involvement from the very first step. An admin must create the account, set the password, and assign the role manually. This works for a small, curated staff team but is friction for operators who want community members to self-nominate.

The question is whether to add a fifth role below `BoardVolunteer` — a `User` tier — that self-registers, has no moderation powers, but gains the ability to submit requests that move them up the staff track.

### Decision

Add `Role::User` as the lowest staff-track tier. This is the **only** path between "anonymous" and "any named role". It does not change posting — posting remains anonymous regardless of whether you hold a User account.

```rust
pub enum Role {
    Admin,           // Full site access: CRUD boards, user management, moderate everywhere
    Janitor,         // Site-wide moderation on all boards
    BoardOwner,      // Manages specific owned boards: config, volunteers, moderation
    BoardVolunteer,  // Moderation on assigned boards only
    User,            // Registered account. No moderation powers. Can submit staff requests.
}
```

**Self-registration** is gated by a `Settings.open_registration: bool` toggle (default: `true`). When `false`, only admins can create User accounts. The registration endpoint is `POST /auth/register`. No email verification in v1.1 — username + password only.

**`StaffRequestRepository`** is a new port covering a new `staff_requests` table:

```
id              UUID PK
from_user_id    UUID FK → users
request_type    TEXT CHECK IN ('board_create', 'become_volunteer', 'become_janitor')
target_slug     TEXT NULLABLE   -- for become_volunteer: the board being requested
payload         JSONB           -- preferred slug/title/rules for board_create; reason for others
status          TEXT CHECK IN ('pending', 'approved', 'denied') DEFAULT 'pending'
reviewed_by     UUID NULLABLE FK → users
review_note     TEXT NULLABLE   -- admin/board-owner note visible to requester
created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
```

**Approval authority:**

| Request type | Who can approve | On approval |
|---|---|---|
| `board_create` | Admin only | Creates board (using payload as defaults, admin can override), promotes User → BoardOwner |
| `become_volunteer` | Admin **or** the board owner of `target_slug` | Adds to `board_volunteers`, promotes User → BoardVolunteer (if currently User) |
| `become_janitor` | Admin only | Promotes to Janitor |

Role promotion follows `MAX(current_role, requested_role)`. A BoardVolunteer approved as a BoardOwner is promoted; a Janitor who submits a become_volunteer request is not demoted.

**User dashboard** (`/user/dashboard`): Minimal. Shows:
- Current role and status
- List of submitted requests with status + reviewer note
- Form to submit a new request (board_create | become_volunteer | become_janitor)
- For board_create: fields for preferred slug, title, rules, and a reason/pitch

**The posting wall is an invariant.** `Role::User` accounts have zero interaction with the posting pipeline. `Post`, `Thread`, `Attachment`, `Ban`, and `Flag` models are never modified to reference user accounts. An authenticated User posting anonymously is indistinguishable from an unauthenticated visitor — same `IpHash`, same anonymous name, same rate limits. This constraint is permanent.

### Consequences

- `Role` enum grows by one variant. DB `CHECK` constraint updated in migration 014.
- `UserService` gains `register()` method (password hash + save User with role=User).
- `StaffRequestService` is a new service (or new module in UserService) handling submit, approve, deny.
- Admin dashboard gains a "Pending Requests" section.
- Board owner dashboard gains a "Volunteer Requests" section for requests targeting their boards.
- Login redirect for `Role::User` goes to `/user/dashboard`.
- `Settings` gains `open_registration: bool`.
- `/auth/register` is a new public endpoint (when `open_registration` is true).
- Existing staff accounts are unaffected — no data migration needed for current users.
- The posting pipeline, `PostService`, `BoardConfig`, and all media/ban/flag logic are untouched.

---

## ADR-018: User Dashboard Is Not a Moderation Surface

**Date**: v1.1 planning
**Status**: Accepted

### Context

ADR-017 introduces a User dashboard at `/user/dashboard`. There is a temptation to add "quality of life" features that blur the line between the staff track and the posting experience — e.g., showing a user's own post history, or linking posts to their account.

### Decision

The User dashboard is strictly a **staff pipeline management surface**. It shows:

1. Current role and pending/historical requests
2. A form to submit new requests
3. Staff messages addressed to this user (v1.1)

It does **not** show:
- Post history (posts are not linked to user accounts — ever)
- Saved threads or watchlists (future feature, if needed, is separate)
- Any moderation controls (those belong to the role dashboards above User)

The rationale: mixing post history with a staff account creates an identity linkage that violates the anonymous-first principle. Even if the *user* chooses to make the connection mentally, the *system* must never record or expose it.

### Consequences

- The User model has no `posts` relation and will never have one.
- No "my posts" endpoint is planned at any version.
- Any future "bookmarks" or "watchlist" feature would use a separate cookie/localStorage mechanism with no server-side account linkage.

---

## ADR-019: Unified Dashboard — One Template, One Context Struct

**Date**: v1.1 planning
**Status**: Accepted

### Context

v1.0 shipped five separate dashboard templates (`admin_dashboard.html`, `janitor_dashboard.html`, `board_owner_top_dashboard.html`, `volunteer_dashboard.html`, and the per-board `board_owner_dashboard.html`). Each template has a different layout and different context struct. Adding a sixth template for `Role::User` in v1.1 would mean six diverging templates that must be kept in sync whenever a new section is added.

The deeper problem: the separate-template model breaks on compound users. A Janitor who also owns `/b/` (via `board_owners`) has no single dashboard that shows both their site-wide moderation authority and their board config controls. The current model would require them to navigate to two separate URLs.

### Decision

Replace the four role-overview templates with a single `dashboard.html` and a single `DashboardTemplate` context struct. Each section is populated (or left empty) based on the user's `role` + `owned_boards` + `volunteer_boards`. The template uses `{% if %}` and `{% for %}` to show/hide sections — it never branches on role name directly, only on whether specific data lists are non-empty or `Option` fields are `Some`.

**The per-board `board_owner_dashboard.html` is NOT merged** — it is a deep-dive config surface for a single board, not a role overview. It remains a separate template reached via the "Manage →" link from the Boards table.

**`volunteer_boards: Vec<BoardId>` is added to `CurrentUser`** (and to JWT claims). This is necessary because the Boards, Logs, and Posts sections must correctly scope content for users whose moderation authority comes from volunteer assignment rather than from their role field alone. Like `owned_boards`, this list is embedded in the JWT at login time and stale by up to the JWT TTL on reassignment.

### Why not per-role handler logic with the same template?

The handlers remain per-route (`/admin/dashboard`, `/janitor/dashboard`, `/board-owner/dashboard`, `/volunteer/dashboard`, `/user/dashboard`). They call shared helper functions to build the common context sections, then specialise only the sections that differ by role. The template never needs to know which route it was served from — it only inspects the data it received.

### Consequences

- `volunteer_boards: Vec<BoardId>` added to `CurrentUser` and `Claims` in `domains/models.rs`.
- `UserRepository::find_volunteer_boards(user_id)` added as a new method on the port.
- `UserService::login()` calls both `find_owned_boards` and `find_volunteer_boards` to populate the JWT.
- Four separate dashboard `*Template` structs in `templates.rs` are replaced by one `DashboardTemplate`.
- Four separate dashboard `.html` templates are replaced by one `dashboard.html`.
- Adding a new dashboard section in the future requires one template change and one context field — not five.
- Per-role routes remain for bookmarkability and role-appropriate auth guards.
---

## ADR-020: `revoke_token` as a Default-No-Op on `AuthProvider`

**Date**: v1.1
**Status**: Accepted

### Context

`JwtAuthProvider` is stateless — tokens cannot be invalidated before their expiry time. `CookieAuthProvider` is stateful — sessions can be deleted from the `user_sessions` table for immediate revocation. Both implement the same `AuthProvider` port. The logout handler needs to call revocation logic, but only the cookie implementation can honour it.

### Decision

Add `revoke_token(&self, token: &Token)` to the `AuthProvider` port with a **default no-op body**. `JwtAuthProvider` inherits the no-op. `CookieAuthProvider` overrides it to call `SessionRepository::delete`. The logout handler always calls `revoke_token` — the result is correct behaviour under both providers without any conditional branching in service or handler code.

### Alternatives Considered

- **Separate `RevocableAuthProvider` sub-trait** — would require the logout handler to downcast or change its generic bounds. Rejected: complexity without benefit.
- **Always return an error for JWT** — would break the logout flow for JWT users. Rejected: logout should always succeed from the user's perspective; the JWT will expire naturally.

### Consequences

- `JwtAuthProvider` gains a no-op `revoke_token` for free via the default impl.
- `CookieAuthProvider` overrides `revoke_token` to call `SessionRepository::delete`.
- The logout handler calls `auth_provider.revoke_token(&token).await?` unconditionally.
- Adding a future `OidcAuthProvider` with its own revocation endpoint is a straightforward override.

---

## ADR-021: `SessionRepository` as a Separate Port from `AuthProvider`

**Date**: v1.1
**Status**: Accepted

### Context

`CookieAuthProvider` needs persistent session storage. Two options: (a) embed a `PgPool` directly in `CookieAuthProvider`, or (b) make `CookieAuthProvider` generic over a `SessionRepository` port.

### Decision

`CookieAuthProvider<SR: SessionRepository>` — the provider is generic over the session store. `InMemorySessionRepository` is the dev/test adapter; `PgSessionRepository` is the production adapter.

This follows the identical pattern used for every other port in the system. The architecture rule is clear: no concrete adapter type appears inside another adapter.

### Consequences

- `SessionRepository` port defined in `domains/ports.rs` alongside all other ports.
- `InMemorySessionRepository` in `storage-adapters/src/in_memory/` — no DB required for dev or CI.
- `PgSessionRepository` pending for v1.1 production use.
- `CookieAuthProvider` unit tests use `InMemorySessionRepository` directly — 12 tests, zero infrastructure.

---

## ADR-022: Basic FTS via `PostRepository::search_fulltext` Rather Than a New Port

**Date**: v1.1
**Status**: Accepted

### Context

ROADMAP v1.1 specifies "Basic Search — PostgreSQL full-text search using the existing GIN index on `posts.body`." PORTS.md defines `SearchIndex` as a v1.2 pluggable port. The v1.1 delivery is explicitly "not a new port yet."

### Decision

Add `search_fulltext(board_id, query, page)` as a new method on the existing `PostRepository` port. The `PgPostRepository` implements it using `plainto_tsquery` + `ts_rank` against the existing GIN index. The `SearchIndex` port remains as a v1.2 planned port. When v1.2 ships `PgFullTextIndex`, the direct `PostRepository` method will be deprecated.

The search endpoint (`GET /boards/:slug/search?q=...`) uses a `SearchState<BR, PR>` holding both `BoardService` (slug lookup + config check for `search_enabled`) and `PostRepository` (FTS query). This avoids a new service layer for what is a direct repo call.

### Consequences

- `PostRepository` gains one new method. All stubs and test implementations must implement it (returning empty pages).
- The search endpoint is gated by `board_config.search_enabled` (default: false). Board owners enable it per-board.
- `board_public_routes` gains a second generic parameter `PR: PostRepository + Clone`.
- When v1.2 pluggable search ships, the `SearchState` becomes `SearchState<BR, SI: SearchIndex>` and the `PostRepository` method is removed.

---

## ADR-023: Name-Rate-Limiting via Pseudo-IP Key on Existing `RateLimiter` Port

**Date**: v1.1
**Status**: Accepted

### Context

ROADMAP v1.1 "Improved Spam Heuristics" includes "posting pattern detection: same name+tripcode combination posting too frequently." This requires rate-limiting by identity, not IP. A new port (`NameRateLimiter`) would be the purist approach.

### Decision

Derive a pseudo-IP-hash from `hash_content("name:{name}:{board_id}")` and pass it as the `ip_hash` field of the existing `RateLimitKey`. This reuses the existing `RateLimiter` port without a new abstraction. The `name_rate_limit_window_secs` field on `BoardConfig` gates the check (0 = disabled).

This is acceptable because: (a) `RateLimitKey` contains both `ip_hash` and `board_id`, making the key space already scoped; (b) the pseudo-hash is deterministic and collision-resistant; (c) it avoids adding a new port for what is a minor heuristic.

### Consequences

- `BoardConfig` gains `name_rate_limit_window_secs: u32` (default: 0 = disabled).
- `BoardConfig` gains `link_blacklist: Vec<String>` in the same migration (017).
- `score_spam` signature changes to `score_spam(body, link_blacklist)`. All callers pass `&board_config.link_blacklist`.
- No new port. No new service. Minimal blast radius.
---

## ADR-024: `StaffMessageService` — Body-Only, Staff-Only, 14-Day Expiry

**Date**: v1.1
**Status**: Accepted

### Decision

`StaffMessageService` enforces three invariants at the service layer:

1. **Sender gate**: `Role::User` cannot send staff messages. `StaffUser` extractor enforces this at the route level; the service also checks and returns `PermissionDenied` if bypassed.
2. **Body-only**: No attachments. `StaffMessage.body` is a plain string, 1–4 000 characters. Attachment support is deferred to v1.2 as a distinct `StaffAttachment` feature.
3. **14-day expiry**: `purge_expired(14)` deletes old messages. Called from a maintenance endpoint (`POST /staff/messages/purge`, admin-only) rather than automatically on every request, to avoid adding latency to the read path.

### Consequences

- `StaffMessageService` is generic over `MR: StaffMessageRepository` — easy to swap for `SqliteStaffMessageRepository` in v1.2.
- No template is added for messages in v1.1 — the inbox is JSON API only. Dashboard unread badge planned as v1.1 follow-up iteration.
- Expiry is operator-triggered, not cron-triggered, to avoid requiring a scheduler.

---

## ADR-025: `PgStaffRequestRepository` Replaces `NoopStaffRequestRepository`

**Date**: v1.1
**Status**: Accepted

### Decision

`NoopStaffRequestRepository` (which silently swallowed all writes) is replaced by `PgStaffRequestRepository` in `composition.rs`. The stub is retained in `storage-adapters/src/stubs/` for integration tests that need a compilable implementation without database access.

The `update_status` query uses `WHERE status = 'pending'` to guard against double-review races. If a request has already been reviewed (status ≠ `'pending'`), the update returns 0 rows affected and `DomainError::NotFound` is returned, which the service converts to `StaffRequestError::NotPending`.

### Consequences

- Staff requests now persist across restarts.
- The admin "Pending Requests" dashboard section becomes functional.
- The board owner "Volunteer Requests" section becomes functional.

---

## ADR-026: `PgSessionRepository` as the Production `auth-cookie` Backing Store

**Date**: v1.1
**Status**: Accepted

### Decision

`PgSessionRepository` stores sessions in the `user_sessions` table (migration 016). It is wired into `CookieAuthProvider` in `composition.rs` under the `auth-cookie` feature flag. `InMemorySessionRepository` remains available for unit tests and single-instance dev environments.

`find_by_id` filters by `expires_at > NOW()` at the SQL level, so the application never sees expired sessions. `purge_expired` is a maintenance method that physically deletes expired rows (to prevent unbounded table growth); it is not called automatically.

### Consequences

- Sessions survive server restarts (unlike `InMemorySessionRepository`).
- Logout is immediate — `delete(session_id)` removes the row; subsequent requests with that token find nothing.
- `InMemorySessionRepository` is the default in CI and unit tests (no DB required).
