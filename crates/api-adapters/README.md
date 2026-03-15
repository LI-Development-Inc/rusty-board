# `api-adapters` — HTTP Transport Layer

Routes, handlers, middleware, templates, and error mapping. Feature-gated per web framework. Handlers are thin: extract → validate request format → call service → map error.

---

## Feature Flags

| Flag | Adapter | Framework |
|------|---------|-----------|
| `web-axum` | `build_router()` | Axum 0.8 + tower-http |
| `web-actix` | `build_app()` | Actix-web — **v1.x, not yet implemented** |

Default build uses `web-axum`.

---

## Module Map

```
src/
├── axum/                          # feature: web-axum
│   ├── mod.rs                     # build_router(...)
│   ├── routes/
│   │   ├── board_routes.rs
│   │   ├── thread_routes.rs
│   │   ├── post_routes.rs
│   │   ├── auth_routes.rs
│   │   ├── admin_routes.rs
│   │   ├── moderation_routes.rs   # mod actions + cycle/pin stubs (TODO v1.2)
│   │   ├── board_owner_routes.rs
│   │   ├── staff_message_routes.rs
│   │   └── user_routes.rs
│   ├── handlers/
│   │   ├── board_handlers.rs
│   │   ├── thread_handlers.rs     # show_thread_html: viewer_role → mod toolbar
│   │   ├── post_handlers.rs
│   │   ├── auth_handlers.rs       # login, register, logout, /auth/me
│   │   ├── admin_handlers.rs
│   │   ├── moderation_handlers.rs # D, D*, B, B&D, B&D*, S+/-, C+/-
│   │   ├── board_owner_handlers.rs
│   │   ├── staff_message_handlers.rs
│   │   ├── volunteer_handlers.rs
│   │   └── user_handlers.rs
│   ├── middleware/
│   │   ├── auth.rs                # JWT/cookie → CurrentUser extension;
│   │   │                          # ModeratorUser, BoardOwnerUser, VolunteerUser extractors
│   │   ├── board_config.rs        # Load + cache BoardConfig per request
│   │   ├── accept.rs              # WantsJson extractor
│   │   ├── cors.rs
│   │   ├── csrf.rs                # CSRF double-submit for cookie auth
│   │   └── request_id.rs
│   ├── error.rs                   # ApiError → HTTP response mapping
│   ├── health.rs                  # GET /health
│   ├── metrics.rs                 # GET /metrics (prometheus)
│   └── templates.rs               # Template structs for askama rendering
├── actix/                         # feature: web-actix — TODO v1.x
│   └── mod.rs
└── common/                        # NOT feature-gated
    ├── errors.rs                  # ApiError — shared error type
    ├── dtos.rs                    # Request/response structs
    └── pagination.rs              # PageResponse<T>
```

---

## Route Table

### Public (no auth)

| Method | Path | Handler | Notes |
|--------|------|---------|-------|
| `GET` | `/` | redirect → `/board/b` | |
| `GET` | `/board/:slug` | `list_threads_html` | paginated thread index |
| `GET` | `/board/:slug/catalog` | `catalog_html` | catalog grid |
| `GET` | `/board/:slug/thread/:id` | `show_thread_html` | thread + posts; mod toolbar if staff |
| `GET` | `/board/:slug/post/:number` | `redirect_to_post` | resolves board-scoped post number → 303 to thread anchor |
| `GET` | `/overboard` | `overboard_html` | recent posts all boards |
| `POST` | `/board/:slug/post` | `create_post` | anonymous post creation |
| `POST` | `/board/:slug/thread/:id/flag` | `create_flag` | report a post |
| `GET` | `/media/:key` | `serve_media` | media file serving |
| `GET` | `/health` | `health_check` | 200 OK |
| `GET` | `/metrics` | `metrics` | prometheus |

### Auth

| Method | Path | Handler |
|--------|------|---------|
| `GET` | `/auth/login` | `login_page` |
| `POST` | `/auth/login` | `login` |
| `POST` | `/auth/logout` | `logout` |
| `POST` | `/auth/refresh` | `refresh_token` |
| `GET` | `/auth/register` | `register_page` |
| `POST` | `/auth/register` | `register` |
| `GET` | `/auth/me` | `me` — returns `{username, role, dashboard_url}` or 401 |

### Moderation (requires `ModeratorUser`)

| Method | Path | Action |
|--------|------|--------|
| `POST` | `/mod/posts/:id/delete` | [D] Delete post |
| `POST` | `/mod/threads/:id/delete` | Delete thread |
| `POST` | `/mod/threads/:id/delete-by-ip` | [D*] Bulk delete by IP in thread |
| `POST` | `/mod/threads/:id/sticky` | [S+/-] Set sticky (`{"value": bool}`) |
| `POST` | `/mod/threads/:id/close` | [C+/-] Set closed (`{"value": bool}`) |
| `POST` | `/mod/bans` | [B] Issue IP ban |
| `POST` | `/mod/bans/:id/expire` | Expire a ban immediately |
| `GET` | `/mod/bans` | List bans |
| `GET` | `/mod/flags` | List pending flags |
| `POST` | `/mod/flags/:id/resolve` | Resolve a flag |
| `GET` | `/mod/dashboard` | Role-aware dashboard redirect |

> **TODO v1.2**: `POST /mod/threads/:id/cycle` (toggle cycle mode) and `POST /mod/posts/:id/pin` — stubs commented in `moderation_routes.rs`.

---

## Templates (`crates/api-adapters/templates/`)

Askama templates — type-checked at compile time. Template structs live in `templates.rs`.

| Template | Struct | Notes |
|----------|--------|-------|
| `base.html` | (layout) | Auth-aware nav; `rbToast` global; fetches `/auth/me` |
| `thread.html` | `ThreadTemplate` | Mod toolbar when `viewer_role.is_some()`; (You) tracking; single-pass quote linkification; all posts shown without pagination |
| `board.html` | `BoardTemplate` | Thread index |
| `catalog.html` | `CatalogTemplate` | Grid view |
| `overboard.html` | `OverboardTemplate<OverboardPostDisplay>` | Recent posts + inline images across all boards |
| `staff_inbox.html` | `StaffInboxTemplate` | Staff messages |
| `staff_compose.html` | `StaffComposeTemplate` | Message compose |
| `*_dashboard.html` | `*DashboardTemplate` | Per-role dashboards |

### Thread page JS features

- **Quote links** — single combined regex (fixes `>>>/slug/N` bug where pass 2 corrupted pass 1 anchors)
- **(You) tracking** — `POST /board/:slug/post` returns `201 {post_number}` on `Accept: application/json`; number stored in `localStorage` and shown as green `(You)` badge
- **Click-to-quote** — clicking the `No.N` anchor only (not the whole post) inserts `>>N` into the reply form
- **Timestamp format** — user-selectable: Relative / MM/DD/YY HH:MM:SS / ISO 8601; `data-ts` epoch attribute on every `<time>` element; preference in `localStorage rb:time-fmt`; relative mode refreshes every 60 s
- **Auto-update** — optional checkbox; polls thread for new posts; exponential back-off 10 s → 5 min on no new activity; preference in `sessionStorage rb:auto-update`
- **Mod toolbar** — `[D] [D*] [B] [B&D] [B&D*] [S+/-] [C+/-]` with confirm dialogs and ban modal
- **IP hash display** — shown to staff only, via `viewer_role` server-side gate
- **Rate limit / capcode errors** — shown as toast popups, not JSON

---

## Handler Conventions

- Handlers are thin. Extract → validate format only → call service → map `ServiceError` to `ApiError`.
- No SQL, no business rules, no `BoardConfig` branching in handlers.
- JSON responses for API clients; HTML responses for browser clients — content negotiation via `WantsJson` middleware.
- `ApiError` maps service errors to appropriate HTTP status codes with structured JSON bodies.

---

## `common/` — Not Feature-Gated

`ApiError`, DTOs, and pagination helpers are shared across all web framework adapters. Adding a second framework (`web-actix`) reuses this entire module.

---

## Testing

Run: `cargo test -p api-adapters` and `cargo test -p integration-tests`

Integration tests use in-process Axum `TestServer` with stub repositories — no real DB required.

Current: **5 handler unit tests** + **75 integration tests** across all route groups, all passing.
