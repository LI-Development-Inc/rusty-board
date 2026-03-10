# api.md
# rusty-board — HTTP API Reference (v1.0)

The complete endpoint list is in `TECHNICALSPECS.md §5`. This document covers request/response formats, authentication, error codes, and usage patterns.

---

## Authentication

Authenticated endpoints require a bearer token in the `Authorization` header:

```
Authorization: Bearer <token>
```

Tokens are issued by `POST /auth/login` and refreshed by `POST /auth/refresh`. Tokens expire after the configured TTL (default: 24 hours).

### Roles

| Role | String in token | Scope |
|------|----------------|-------|
| `Admin` | `admin` | Full site access: board CRUD, user management, all moderation |
| `Janitor` | `janitor` | Site-wide moderation on all boards: delete, ban, sticky, close, resolve flags |
| `BoardOwner` | `board_owner` | Manages specific boards: config, volunteers, moderation on own boards |
| `BoardVolunteer` | `board_volunteer` | Moderation on assigned boards only: delete posts, issue bans |
| `User` | `user` | Registered account. No moderation. Can submit staff requests. |

Board owners also require membership in the `board_owners` join table for each board they manage.

---

## Common Patterns

### Pagination

All list endpoints accept `?page=N` (1-indexed, default 1). Responses follow this shape:

```json
{
  "items": [...],
  "total": 42,
  "page": 1,
  "page_size": 15,
  "total_pages": 3
}
```

### Error Responses

All errors return a JSON body:

```json
{
  "error": "human-readable message",
  "code": "snake_case_code"
}
```

| HTTP Status | When |
|------------|------|
| `400 Bad Request` | Malformed request body |
| `401 Unauthorized` | Missing or invalid bearer token |
| `403 Forbidden` | Authenticated but insufficient role |
| `404 Not Found` | Resource does not exist |
| `409 Conflict` | Duplicate slug on board creation |
| `422 Unprocessable Entity` | Validation failure (body too long, disallowed MIME type) |
| `429 Too Many Requests` | Rate limit exceeded; includes `Retry-After` header |
| `500 Internal Server Error` | Unexpected server error (details never exposed) |

---

## Public Endpoints

### `GET /boards`

List all boards. No authentication required.

**Response** `200 OK`:
```json
{
  "items": [
    { "id": "uuid", "slug": "tech", "title": "/tech/ — Technology", "rules": "..." }
  ],
  "total": 1, "page": 1, "page_size": 15, "total_pages": 1
}
```

### `GET /board/:slug`

Board index — thread list sorted by `bumped_at` descending, sticky threads first. Returns HTML.

### `GET /board/:slug/catalog`

Catalog grid view — all threads with thumbnail and post count. Returns HTML.

### `GET /board/:slug/thread/:id`

Thread view with all posts. Returns HTML.

### `POST /board/:slug/post`

Create a new thread (no `thread_id`) or reply (with `thread_id`). Multipart form data.

**Fields:**
- `thread_id` (UUID, optional) — omit to create a new thread
- `name` (string, optional, max 64 chars) — ignored if `forced_anon` is set
- `email` (string, optional) — send `sage` to suppress bump
- `body` (string, required if no files, max `board_config.max_post_length`) — post text
- `files` (binary, 0..N) — file attachments (max `board_config.max_files`)

**Responses:**
- `303 See Other` — post created; `Location` header points to `/board/:slug/thread/:id#post-:number`
- `403 Forbidden` — poster IP is banned
- `422 Unprocessable Entity` — validation failure (empty post, body too long, disallowed MIME)
- `429 Too Many Requests` — rate limited

> **Note:** This is a browser-form endpoint. The success response is always a redirect, not a JSON body. Programmatic access to thread and post data should use the read endpoints (`GET /board/:slug/thread/:id`).

### `POST /board/:slug/thread/:id/flag`

Report a post. No authentication required.

**Body** (JSON):
```json
{ "post_id": "uuid", "reason": "spam" }
```

**Response** `201 Created`.

### `GET /overboard`

Recent posts across all boards, paginated by `created_at` descending.

### `GET /healthz`

Health check. Returns `200 OK` with `{"status":"ok"}` when all dependencies are healthy, or `503 Service Unavailable` with degraded component details.

### `GET /metrics`

Prometheus text format metrics export.

---

## Auth Endpoints

### `POST /auth/register`

Open self-registration. Only available when `Settings.open_registration` is `true` (the default). Creates an account with `Role::User`.

**Body** (JSON):
```json
{ "username": "alice", "password": "correct_horse_battery_staple" }
```

**Response** `201 Created`:
```json
{ "token": "eyJ...", "expires_at": 1735689600 }
```

**Error** `409 Conflict` — username already taken.
**Error** `403 Forbidden` — open registration is disabled on this instance.

### `POST /auth/login`

**Body** (JSON):
```json
{ "username": "admin", "password": "correct_horse_battery_staple" }
```

**Response** `200 OK`:
```json
{ "token": "eyJ...", "expires_at": 1735689600 }
```

**Error** `401 Unauthorized` — wrong username or password.

### `POST /auth/refresh`

Requires valid bearer token. Returns a new token with a refreshed expiry.

**Response** `200 OK`: same shape as login.

---

## Board Owner Endpoints

### `GET /board/:slug/config`

Retrieve current `BoardConfig` for a board. Requires authenticated user.

**Response** `200 OK`:
```json
{
  "bump_limit": 300,
  "max_files": 4,
  "max_file_size_kb": 4096,
  "allowed_mimes": ["image/jpeg", "image/png", "image/gif", "image/webp"],
  "max_post_length": 4000,
  "rate_limit_enabled": true,
  "rate_limit_window_secs": 60,
  "rate_limit_posts": 3,
  "spam_filter_enabled": true,
  "spam_score_threshold": 0.8,
  "duplicate_check": true,
  "forced_anon": false,
  "allow_sage": true,
  "allow_tripcodes": false,
  "captcha_required": false,
  "nsfw": false
}
```

### `PUT /board/:slug/config`

Partial update of `BoardConfig`. All fields optional — only provided fields are changed.

**Body** (JSON, any subset of config fields):
```json
{ "bump_limit": 100, "forced_anon": true }
```

**Response** `200 OK` — the full updated config.

---

## Dashboard Routes

Each staff role has a dedicated dashboard. After login, the browser is redirected
to the appropriate URL by the login page JavaScript.

| Role | Dashboard URL | Notes |
|---|---|---|
| `admin` | `GET /admin/dashboard` | Full site admin interface |
| `janitor` | `GET /janitor/dashboard` | Site-wide flag queue, ban list, audit log |
| `board_owner` | `GET /board-owner/dashboard` | Lists owned boards |
| `board_owner` | `GET /board/:slug/dashboard` | Per-board config + volunteer management |
| `board_volunteer` | `GET /volunteer/dashboard` | Flags for assigned boards |
| `user` | `GET /user/dashboard` | Request history + new request form |

`GET /mod/dashboard` — backwards-compatibility shim. Redirects (`303 See Other`)
to the caller's own dashboard based on their role. Requires any staff login.

---

## User & Staff Request Endpoints

### `GET /user/dashboard`

User dashboard. Requires `Role::User` or above. Returns HTML.

### `GET /user/requests`

Paginated list of the authenticated user's own staff requests.

### `POST /user/requests`

Submit a new staff request.

**Body** (JSON):
```json
{
  "request_type": "board_create",
  "payload": {
    "slug": "cooking",
    "title": "/cooking/ — Food & Recipes",
    "rules": "Keep it food related.",
    "reason": "There is no food board and I would like to run one."
  }
}
```

For `become_volunteer`: `payload` must include `target_slug` (the board being requested) and `reason`.
For `become_janitor`: `payload` must include `reason`.

**Response** `201 Created`.

### Admin: `GET /admin/requests`

All pending staff requests. Admin only.

### Admin: `POST /admin/requests/:id/approve`

Approve a request. For `board_create`, body may include overrides:

**Body** (JSON, all fields optional):
```json
{ "slug": "ck", "title": "/ck/ — Cooking", "rules": "..." }
```

**Response** `204 No Content`.

### Admin: `POST /admin/requests/:id/deny`

**Body**: `{ "note": "We already have a similar board." }`

**Response** `204 No Content`.

### Board Owner: `POST /board/:slug/requests/:id/approve`

Approve a `become_volunteer` request for this board. Board owner or Admin only.

**Response** `204 No Content`.

### Board Owner: `POST /board/:slug/requests/:id/deny`

**Body**: `{ "note": "..." }` (optional)

**Response** `204 No Content`.

---

## Moderation Endpoints

Delete/ban/flag actions require staff credentials. The specific minimum role for each endpoint is noted below.

### `GET /mod/flags`

Pending flag queue, paginated.

### `POST /mod/flags/:id/resolve`

**Body**:
```json
{ "resolution": "approved" }  // or "rejected"
```

**Response** `204 No Content`.

### `POST /mod/posts/:id/delete`

Delete a single post. If it's the OP, the entire thread is deleted.

**Response** `204 No Content`.

### `POST /mod/threads/:id/sticky`

Toggle sticky status. **Response** `204 No Content`.

### `POST /mod/threads/:id/close`

Toggle closed status. **Response** `204 No Content`.

### `POST /mod/bans`

Issue an IP ban.

**Body**:
```json
{
  "ip_hash": "64-char hex string",
  "reason": "repeated spam",
  "expires_at": "2026-12-31T00:00:00Z"  // null for permanent
}
```

**Response** `201 Created`.

### `POST /mod/bans/:id/expire`

Immediately expire a ban. **Response** `204 No Content`.

---

## Admin Endpoints

All require `Admin` role.

### `POST /admin/boards`

Create a board. Also creates a default `BoardConfig`.

**Body**: `{ "slug": "g", "title": "/g/ — Technology", "rules": "..." }`

**Response** `201 Created`.

### `PUT /admin/boards/:id`

Update board title/rules.

### `DELETE /admin/boards/:id`

Delete a board and cascade-delete all threads, posts, flags, and config.

**Response** `204 No Content`.

### `POST /admin/users`

Create a staff account. Role must be one of: `"admin"`, `"janitor"`, `"board_owner"`, `"board_volunteer"`.

**Body**: `{ "username": "jdoe", "password": "...", "role": "janitor" }`

**Response** `201 Created`.

### `POST /admin/users/:id/deactivate`

Soft-delete a user account. **Response** `204 No Content`.

### `POST /admin/boards/:id/owners`

Assign a user as board owner.

**Body**: `{ "user_id": "uuid" }`

**Response** `204 No Content`.

### `DELETE /admin/boards/:id/owners/:user_id`

Remove board owner assignment. **Response** `204 No Content`.
