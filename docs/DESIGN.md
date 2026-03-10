# DESIGN.md
# rusty-board — Design Rationale & Patterns

> **Why the system is built the way it is. When facing a design decision not explicitly covered elsewhere, consult this document first.**

---

## 1. Guiding Principles

These eight principles are the decision-making filter for every design choice in the project. When two approaches seem equally valid, apply these in order.

**1. Compile-time Adapter Modularity** — Swapping any external dependency is a Cargo feature flag and a recompile. The compiler enforces that the new adapter satisfies the port contract. Zero runtime cost. This is the primary modularity mechanism.

**2. Runtime Behavioral Composition** — How the system behaves is controlled through operator dashboards. Settings live in `BoardConfig` in the database. No recompile needed to change behavior. This is the primary operator control mechanism.

**3. Imageboard Fidelity** — Every standard imageboard mechanic must be correctly implemented: anonymous posting, ephemeral threads, bump order, sage, greentext, quote links, tripcodes, catalog view, overboard, flags, bans, thread pruning. This is a content platform, not a generic CRUD app. Design decisions that harm the imageboard experience for the sake of engineering elegance are wrong.

**4. Security by Default** — Validation, sanitization, EXIF stripping, IP hashing, rate limiting, CSRF/CORS protection are on by default and not toggleable off at the infrastructure level. Security decisions that require operators to "remember to turn it on" are wrong.

**5. Testability** — Every external dependency is behind a port trait. Services are testable with mock ports, no real infrastructure required. Adapters are testable with contract tests against real infrastructure (testcontainers). Handlers are testable with integration tests against a full stack. If something is hard to test, it is probably in the wrong layer.

**6. Performance** — Zero-cost generics via monomorphization in hot paths. `BoardConfig` branching adds only a field read. No dynamic dispatch where it is not needed. No allocations in the common path. An imageboard that is slow under load fails its users.

**7. Ops Readiness** — Stateless application layer, structured JSON logging, Prometheus metrics, and health checks from day one. An operator should never need to log into the application server to understand what the system is doing.

**8. Long-term Maintainability** — The core (`domains`, `services`) reaches stability at v1.0 and requires only additive changes thereafter. Future work is adding new adapters, not restructuring existing code. Design decisions that make the core stable and narrow are preferred over decisions that make it flexible and wide.

---

## 2. The Central Design Decision

The entire architecture is organized around one explicit boundary:

> **Adapter selection is compile-time. Behavioral configuration is runtime.**

This is not a performance optimization. It is a correctness and clarity decision.

**Why compile-time for adapters?** Because the compiler can verify that `PgBoardRepository` actually implements `BoardRepository`. If it doesn't, the code doesn't compile. There is no way to deploy a binary with a missing or incorrectly implemented adapter. The type system enforces the contract.

**Why runtime for behavior?** Because an admin enabling rate limiting on a board, or a board owner lowering the spam threshold, should not require a new release. These are operational decisions made by people who are not developers. The feedback loop must be immediate.

**The hard rule this creates:** Services never read `#[cfg(feature = "...")]` flags. Feature flags never appear inside service method bodies. `BoardConfig` is the only path from a dashboard control to service behavior. Violating this rule is always a design error, not a implementation convenience.

---

## 3. Design Patterns

### Ports & Adapters (Hexagonal Architecture)

Every external boundary is a trait in `domains/ports.rs`. Adapter crates implement those traits. Services depend only on the traits.

This is the foundation of all compile-time modularity. The reason it works with zero overhead in Rust is that services are generic over their port types, and the compiler monomorphizes the generics to concrete types in `composition.rs`. After monomorphization, there are no virtual function calls — the compiler has direct knowledge of all concrete types used.

The pattern also makes testing trivial: swap the concrete adapter for a `mockall`-generated mock. The service doesn't know or care.

### Monomorphic Services

Services are generic structs bounded by port traits:

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

The composition root instantiates this with concrete types once. After that, the compiler sees `PostService<PgPostRepository, PgThreadRepository, PgBanRepository, S3MediaStorage, RedisRateLimiter, ImageMediaProcessor>` everywhere it matters.

**Ergonomic note**: Services with more than four type parameters produce verbose error messages and type annotations. Use type aliases in `composition.rs` to give the concrete instantiation a readable name. See `CONVENTIONS.md` for the `*Deps` alias pattern.

### Composition Root

`cmd/rusty-board/src/composition.rs` is the single file in the codebase that:
- Reads feature flags (`#[cfg(feature = "...")]`)
- Constructs concrete adapter types
- Injects them into generic services
- Returns a configured application router

Every other file is feature-flag-free. Every concrete type is instantiated exactly once. This makes the wiring of the application explicit, auditable, and correct by construction.

### Facade (Media Processing)

The `MediaProcessor` port is implemented as a facade in `storage-adapters/src/media/mod.rs`. The facade dispatches to the correct processor (images, video, documents) based on the MIME type of the incoming file and which processors are compiled in.

`PostService` calls `media_processor.process(raw_media)` and receives `ProcessedMedia`. It is unaware of whether video processing is available, whether documents are supported, or which concrete implementation is running. This is a compile-time dispatch — the facade is monomorphized with knowledge of which processors exist.

### Value Objects

Domain primitives are newtype wrappers, not raw scalars:

```rust
pub struct BoardId(Uuid);
pub struct IpHash(String);      // SHA-256 of IP + daily salt
pub struct Slug(String);        // validated: ^[a-z0-9_-]{1,16}$
pub struct FileSizeKb(u32);
pub struct ContentHash(String); // SHA-256 of file bytes
pub struct MediaKey(String);    // storage key (S3 path or local path)
```

Value objects carry their validation into the type system. You cannot accidentally pass a raw `String` where an `IpHash` is expected. You cannot construct an invalid `Slug`. This eliminates an entire class of domain bugs at the type level.

### BoardConfig as Behavior Surface

`BoardConfig` is a plain struct stored in the database. It is the complete and authoritative list of every behavioral parameter that operators control. Services receive it as a parameter, branch on its fields, and are done.

The discipline: if a behavior varies by board, it is a `BoardConfig` field. If it varies by deployment (infrastructure choice), it is in `Settings`. If it is a fixed business rule that never changes, it is a domain constant or encoded in the type system.

---

## 4. Error Handling

Errors are transformed at every layer boundary. No error type leaks across a boundary.

### Error Types by Layer

| Layer | Error Type | Mechanism | Example |
|-------|-----------|-----------|---------|
| Adapters | Caught internally | Map to `DomainError` | `sqlx::Error` → `DomainError::NotFound` |
| Domains | `DomainError` | `thiserror` enum | `DomainError::Validation(ValidationError)` |
| Services | `*Error` per service | `thiserror` enum wrapping `DomainError` | `PostError::SpamDetected` |
| Handlers | `ApiError` | Maps service errors to HTTP | `PostError::RateLimited` → 429 |

### Propagation Rule

Adapter code catches its own errors (e.g., `sqlx::Error`, `aws_sdk_s3::Error`) and returns `DomainError` variants. These must never appear in service code.

Service code catches `DomainError` and wraps it in the service-specific error type. These must never appear in handler code as raw values.

Handler code matches on service error types and maps them to `ApiError`. User-facing messages are generic. Internal details are logged via `tracing`.

### `anyhow` Usage

`anyhow::Error` is used internally within service and adapter methods for context propagation (`.context("while inserting post")`). It is never a public function return type. Public APIs return typed enums.

### Logging at Boundaries

- Adapters: `tracing::error!` with full context before returning `DomainError`
- Services: `tracing::warn!` for expected failures (validation, rate limit); `tracing::error!` for internal failures
- Handlers: `tracing::warn!` or `tracing::error!` before returning `ApiError`; never expose internal details to the response body

---

## 5. Media Pipeline

### Separation of Concerns

Processing and storage are separate ports because they have different adapter axes. A deployment may use S3 for storage but only image processing. Another may use local filesystem storage with full video+document processing. The combination is determined at compile time.

### Processing

`MediaProcessor::process(RawMedia)` → `ProcessedMedia`

Input: raw bytes + filename + MIME type.
Output: original bytes (MIME-validated), thumbnail bytes (may be None for types without thumbnail support), content hash, size in KB.

Processing steps for images (always):
1. Validate MIME type against `board_config.allowed_mimes`
2. Decode with `image` crate
3. Strip EXIF metadata (always — not configurable)
4. Resize to 320px width, proportional height
5. Encode thumbnail as PNG, compress with `oxipng`
6. Compute `ContentHash` (SHA-256 of original bytes)

Processing for video (`video` feature):
1. MIME validation
2. `ffmpeg-next` seek to keyframe position (2 seconds in, or first frame)
3. Extract frame as RGB bitmap
4. Pass to image thumbnail pipeline (resize + PNG encode)

Processing for documents (`documents` feature):
1. MIME validation
2. `pdfium-render` renders first page to bitmap at 150 DPI
3. Pass to image thumbnail pipeline

### Storage

`MediaStorage::store(key, data, content_type)` and `MediaStorage::get_url(key, ttl)`.

Keys are deterministic: `{board_id}/{thread_id}/{post_id}/{uuid}.{ext}` for originals, `{...}/thumb/{uuid}.png` for thumbnails. This allows manual recovery and deduplication inspection.

URL TTL: handlers pass `settings.media_url_ttl_secs` (default: 86400). S3 generates presigned URLs. Local filesystem generates static paths (TTL unused).

### Build Risks

`ffmpeg-next` requires libav* system libraries. The Docker builder stage must install them. Build times increase significantly. Validate in the Docker environment during Phase 3, week 1. If the build is intractable, defer the `video` feature to v1.1.

`pdfium-render` requires a pre-built PDFium binary. PDFium is BSD-licensed. Validate the binary distribution model before committing to shipping it. Treat `documents` as experimental for v1.0.

---

## 6. Anti-Spam & Rate Limiting

### Two Distinct Mechanisms

**Infrastructure rate limiting** (`RateLimiter` port): Per-IP, per-board counters. Redis provides distributed counters for multi-instance deployments. Checks happen inside `PostService` when `board_config.rate_limit_enabled` is true, using the board's `rate_limit_window_secs` and `rate_limit_posts` values. The `NoopRateLimiter` is provided for unit tests and can be used in single-instance development deployments without Redis.

**Heuristic spam detection** (inside `PostService`): Pure logic, no port needed. Scores posts based on: body length extremes (too short or too long), link density (count of URLs), duplicate content (hash comparison against recent posts via `PostRepository::find_recent_hashes`), repeated identical names or tripcodes. Score threshold is `board_config.spam_score_threshold`. Enabled by `board_config.spam_filter_enabled`. Board owners tune these parameters per board.

### Future Anti-Spam

CAPTCHA (v1.1): `board_config.captcha_required` field already exists. When true, `PostService` will verify a `CaptchaVerifier` port. The port is defined in `PORTS.md` (planned). Adapters: `HCaptchaCaptchaVerifier`, `ReCaptchaCaptchaVerifier`.

DNSBL (v1.2): New `DnsblChecker` port. PostService checks it when enabled.

ML scoring (v2.0): New `SpamScorer` port replacing or augmenting the heuristic function.

In every case: new port in `domains`, new adapter feature, new `BoardConfig` toggle. Zero changes to existing service logic.

---

## 7. Authentication & Authorization

### Auth Flow

Anonymous posters have no account and no token. They post by hitting `POST /board/:slug/post` with no auth header.

Staff (janitors, board owners, board volunteers, and admins) authenticate via `POST /auth/login` with username + password. The `UserService` verifies the password via `AuthProvider::verify_password`, then calls `AuthProvider::create_token` to return a JWT. Subsequent requests include `Authorization: Bearer <token>`.

The auth middleware (`api-adapters/axum/middleware/auth.rs`) extracts and verifies the token via `AuthProvider::verify_token`, then builds a `CurrentUser` struct — including the user's role and owned board IDs — from the claims and a `UserRepository` lookup. `CurrentUser` is attached to the request extensions.

### Role Model

```rust
pub enum Role {
    Admin,           // Full site access: CRUD boards, user management, moderate everywhere.
    Janitor,         // Site-wide moderation on all boards: delete, ban, sticky, close, resolve flags.
    BoardOwner,      // Owns specific boards: config management, volunteer assignment, board moderation.
    BoardVolunteer,  // Moderates their assigned boards only: delete posts, issue bans.
    User,            // Registered account. No moderation powers. Submits staff requests.
}
```

**Moderation is an action, not a role.** Any role with sufficient privilege on a board may
delete a post, issue a ban, or resolve a flag. The permission methods on `CurrentUser`
encode the privilege rules without hardcoding role names into handlers.

Board ownership is a database relationship (`board_owners` table), not a role variant.
A `BoardOwner` user can own multiple boards; each board can have multiple owners.
Board volunteers are tracked separately in `board_volunteers`.

### Permission Checks

Permission checks happen in handlers, not services. Services perform operations. Handlers decide whether the current user is permitted to request that operation.

```rust
// In handler — check permission before calling service
async fn update_board_config(
    current_user: Extension<CurrentUser>,
    Path(slug): Path<String>,
    // ...
) -> Result<impl IntoResponse, ApiError> {
    if !current_user.can_manage_board_config(board_id) {
        return Err(ApiError::Forbidden);
    }
    board_service.update_config(board_id, update).await?;
    // ...
}

// CurrentUser method
pub fn can_manage_board_config(&self, board_id: BoardId) -> bool {
    self.is_admin() || self.owned_boards.contains(&board_id)
}
```

### Future Auth

Tripcodes (v1.1): `board_config.allow_tripcodes` field already exists. A new `auth-tripcode` feature will provide tripcode hashing as a compile-time option. Tripcode hashing does not need the `AuthProvider` port — it is deterministic cryptographic hashing with no token management.

Cookie sessions (v1.1): `CookieAuthProvider` implements `AuthProvider`. The middleware detects whether to look for a Bearer token or a session cookie based on which auth adapter is compiled.

Two-factor auth (v1.3): New methods on `AuthProvider` or a separate `TwoFactorProvider` port.

OIDC (v2.0): `OidcAuthProvider` implements `AuthProvider` with PKCE flow.

---

## 8. Templates & UI

### Askama

Compile-time checked templates. Template errors are build errors, not runtime panics. Templates receive typed context structs from handlers. Escaping is automatic — XSS via template output is structurally prevented.

Templates are organized by view, not by component type. `board.html` is a complete page. `components/post.html` is a reusable partial included by `thread.html`.

### Dashboard Architecture

**One template. One context struct. Five roles.**

All roles share a single `dashboard.html` template and a single `DashboardTemplate` context struct. The template renders sections based on what the authenticated user actually has access to — derived from their `role` field, their `owned_boards`, and their `volunteer_boards`. This handles compound users naturally: a Janitor who owns `/b/` sees both site-wide moderation sections and board config controls for `/b/` in the same view.

`/mod/dashboard` is a compatibility shim: it issues a `303 See Other` to `/dashboard`, so old bookmarks continue to work.

#### Uniform section layout

Every dashboard renders these sections in order. Sections that have no content for the current user are hidden, not absent — the template structure is always the same.

```
{ROLE} Dashboard
─────────────────────────────────
Site Announcements       visible to: all roles (staff messages addressed to this user)
─────────────────────────────────
Boards                   visible to: all roles
  Admin / Janitor:  all boards — actions: config, manage, delete
  BoardOwner:       owned boards only — actions: config, manage volunteers
  BoardVolunteer:   assigned boards only — actions: view flags
  User:             all boards — actions: request ownership / volunteer
─────────────────────────────────
Staff                    visible to: Admin, Janitor, BoardOwner (own volunteers only)
  Admin:    all accounts — actions: deactivate, message
  Janitor:  all accounts — actions: message
  BoardOwner: volunteers on their boards — actions: remove volunteer, message
─────────────────────────────────
Recent Logs              visible to: Admin, Janitor, BoardOwner, BoardVolunteer
  Admin / Janitor:  site-wide last 10 audit entries
  BoardOwner:       last 10 entries across owned boards
  BoardVolunteer:   last 10 entries across assigned boards
─────────────────────────────────
Recent Posts             visible to: Admin, Janitor, BoardOwner, BoardVolunteer
  Admin / Janitor:  site-wide recent posts
  BoardOwner:       recent posts across owned boards
  BoardVolunteer:   recent posts across assigned boards
─────────────────────────────────
Messages                 visible to: all roles (once staff messaging lands in v1.1)
  Shows unread count badge + preview of last 3 messages + link to inbox
─────────────────────────────────
Pending Requests         visible to: Admin (all), BoardOwner (volunteer requests for their boards)
─────────────────────────────────
```

#### `DashboardTemplate` context struct (v1.1 target)

```rust
pub struct DashboardTemplate {
    // Header
    pub role_display:      String,          // "Admin", "Janitor", "Board Owner", etc.

    // Announcements — staff messages addressed to current user
    pub announcements:     Vec<StaffMessage>,

    // Boards — populated from role + owned_boards + volunteer_boards
    pub boards:            Vec<DashboardBoard>,  // includes per-board available actions

    // Staff — None if role has no staff visibility
    pub staff:             Option<Vec<DashboardUser>>,

    // Logs — filtered by role + board access
    pub recent_logs:       Vec<AuditEntry>,

    // Posts — filtered by role + board access
    pub recent_posts:      Vec<Post>,

    // Messages inbox
    pub messages:          Vec<StaffMessage>,
    pub unread_count:      u32,

    // Pending staff requests — None if role cannot review requests
    pub pending_requests:  Option<Vec<StaffRequest>>,
}
```

#### `CurrentUser` must carry `volunteer_boards` (v1.1 change)

Currently `CurrentUser` carries `owned_boards: Vec<BoardId>` but not volunteer assignments. The uniform dashboard requires both to correctly scope the Boards, Logs, and Posts sections for compound users (e.g. a `BoardOwner` who also volunteers on another board).

```rust
pub struct CurrentUser {
    pub id:               UserId,
    pub role:             Role,
    pub owned_boards:     Vec<BoardId>,   // from board_owners join table
    pub volunteer_boards: Vec<BoardId>,   // from board_volunteers join table — NEW in v1.1
}
```

Both lists are embedded in the JWT claims at login time (populated from a `UserRepository::find_owned_boards()` + `find_volunteer_boards()` call in `UserService::login()`). Board ownership / volunteer assignment changes take effect on the next login — same consistency model as `owned_boards` today.

#### Route structure

The per-role routes (`/admin/dashboard`, `/janitor/dashboard`, etc.) remain — they are the correct, bookmarkable URLs. Each calls the same unified handler which builds a `DashboardTemplate` context scoped to that user's role and board access. The separate per-role template files are replaced by a single `dashboard.html`.

`/board/:slug/dashboard` (per-board owner view) remains a **separate** template and handler — it is a deep-dive config surface for a specific board, not a role overview. It is reached from the "Manage →" link in the Boards table.

The board owner dashboard is the most important UI surface for runtime behavioral composition. It must be comprehensive, legible, and correct.

### Progressive Enhancement

All views are fully functional without JavaScript. JS adds:
- Lazy image loading (`IntersectionObserver`)
- Reply form toggle (show inline without page reload)
- Quote click to jump to referenced post
- Character count on post body textarea

The JSON API provides equivalent data for all views. Non-JS clients and Tor Browser users should have a first-class experience.

---

## 9. Observability

### Structured Logging

`tracing` with `tracing-subscriber`. JSON format in production. Human-readable format in development (detected via `RUST_LOG` and `APP_ENV`).

Every request span includes: `request_id` (UUID, generated by middleware), `method`, `path`, `status`, `latency_ms`. Every post creation includes: `board_slug`, `thread_id`, `ip_hash_prefix` (first 8 chars only — sufficient for log correlation, insufficient for re-identification).

### Metrics

`prometheus-client` with counters and histograms exposed at `/metrics`.

Key metrics:
- `http_requests_total{method, path, status}` — request count by route and outcome
- `http_request_duration_seconds{method, path}` — latency histogram
- `db_query_duration_seconds{operation}` — database query latency
- `media_process_duration_seconds{mime_type}` — thumbnail generation time
- `rate_limit_hits_total{board_slug}` — rate limit enforcement count
- `spam_rejections_total{board_slug, reason}` — spam filter rejection count
- `ban_checks_total{result}` — ban enforcement count

### Health Check

`/healthz` performs:
- `SELECT 1` against the database
- `PING` against Redis (if compiled)
- File write test against media path (if local-fs compiled)

Returns `{"status": "ok"}` or `{"status": "degraded", "components": {"db": "ok", "redis": "error: ..."}}`

---

## 10. Configuration Architecture

### Settings vs. BoardConfig

This distinction is enforced at the type level. When adding a new configuration field, the question is: does this vary per board, or does it vary per deployment?

**Per deployment → `Settings`** (infrastructure, loaded from env at startup):
- Server bind address and port
- Database URL
- Redis URL
- JWT secret and token TTL
- S3 bucket, region, endpoint, credentials
- Media path (for local-fs)
- Thumbnail dimensions (width: 320px default)
- IP salt rotation period (24h default)
- Argon2id parameters (m=19456, t=2, p=1)
- Media URL TTL (86400s default)
- Shutdown timeout

**Per board → `BoardConfig`** (behavioral, loaded from DB per request):
- Everything in the `BoardConfig` struct (see `ARCHITECTURE.md` § 8)

**Fixed business rules → domain constants or types**:
- EXIF stripping is always done (not a toggle at any level)
- IP addresses are never stored raw (structural, not a setting)
- Thumbnail format is always PNG (not configurable)
- Slug regex is always `^[a-z0-9_-]{1,16}$` (domain invariant)

### BoardConfig Caching

`BoardConfig` is cached in-process using a `DashMap<BoardId, (BoardConfig, Instant)>` with a 60-second TTL. The cache lives in `storage-adapters/src/cache/board_config.rs` as a `BoardConfigCache` struct passed through the app state.

On `PUT /board/:slug/config`, the handler invalidates the cache entry for that board immediately before returning success. This means config updates take effect within the next request on any instance, or immediately on the instance that processed the update.

For multi-instance deployments in v1.0, up to 60 seconds of stale config is acceptable for behavioral toggles (rate limit thresholds, spam settings). If a board owner turns off NSFW and the change propagates within 60 seconds, that is acceptable. If stricter consistency is needed, Redis pub/sub cache invalidation can be added without touching service code — the cache is behind an interface.

---

## 11. Trade-offs

### Compile-time features → no runtime adapter swap, but zero cost and compiler-enforced correctness

The inability to swap a database at runtime without a recompile is the explicit trade-off for having the compiler guarantee the adapter is correct. An operator who wants to switch from Postgres to SQLite must redeploy. This is a reasonable operational constraint for a system that is not expected to switch databases frequently.

### Monomorphic generics → verbose type signatures, but zero overhead

`PostService<PgPostRepository, PgThreadRepository, PgBanRepository, S3MediaStorage, RedisRateLimiter, ImageMediaProcessor>` is a verbose type. The `*Deps` alias pattern in `composition.rs` mitigates this. The performance benefit is real: the compiler inlines and optimizes across the call sites in ways that dynamic dispatch prevents.

### BoardConfig branching → more conditional logic in services, but clean runtime extension

Adding a new behavioral toggle adds a field read and a conditional in a service method. This is the cost of runtime behavioral composition. The alternative — a new port and adapter for every toggle — would make behavioral changes enormously expensive. The complexity is local and bounded.

### SSR (Askama) → fast, safe, no JS build pipeline, but limited interactivity

Imageboard users have low JS expectations and high privacy expectations. SSR with minimal JS enhancement is the correct choice for v1.0. Live updates (v1.3) will be additive via WebSockets, not a replacement of SSR.

### Postgres/Axum for v1.0 → faster working software, modularity preserved for later swaps

Every port is defined. The composition root is structured for extension. The first alternative adapters (SQLite in v1.2, Actix in v1.x) will validate that the swap actually works end-to-end. The architecture claim is not proven until at least one real adapter swap is complete.
