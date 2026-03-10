# `configs` — Infrastructure Settings

Loads and validates all infrastructure configuration from environment variables and `.env` files. Contains only deployment-time settings — never per-board behavioral parameters.

---

## What Lives Here

```
src/
├── lib.rs       # Settings struct
└── defaults.rs  # Feature-aware infrastructure defaults
```

---

## `Settings` Fields

| Field | Env var | Default | Notes |
|-------|---------|---------|-------|
| `server_addr` | `SERVER_ADDR` | `0.0.0.0:8080` | |
| `db_url` | `DATABASE_URL` | — | Required |
| `redis_url` | `REDIS_URL` | `redis://localhost:6379` | `redis` feature |
| `jwt_secret` | `JWT_SECRET` | — | Required for `auth-jwt` |
| `jwt_ttl_secs` | `JWT_TTL_SECS` | `86400` | 24 hours |
| `media_path` | `MEDIA_PATH` | `./media` | `media-local` feature |
| `s3.*` | `S3_*` | — | `media-s3` feature |
| `argon2_*` | `ARGON2_*` | OWASP recommended | Memory, iterations, parallelism |
| `thumbnail_*` | `THUMBNAIL_*` | 320px | Max dimension for thumbnails |
| `ip_salt_rotation_secs` | `IP_SALT_ROTATION_SECS` | `86400` | Daily salt rotation for IP hashing |
| `registration_open` | `REGISTRATION_OPEN` | `true` | Controls `GET /auth/register` visibility |

---

## The Critical Distinction

`Settings` contains **infrastructure** configuration only:
- Where is the database?
- What is the JWT secret?
- Where are media files stored?

It does **not** contain per-board behavioral parameters. Those live in `BoardConfig` in the database, managed through admin dashboards. This separation means:

- Changing board behavior (rate limits, spam thresholds, NSFW status) requires **no redeployment**
- Changing infrastructure (database URL, secrets, storage path) requires **a restart**

See `ARCHITECTURE.md §4` for the full compile-time vs. runtime boundary table.

---

## Usage

```sh
# From environment
DATABASE_URL=postgresql://user:pass@localhost/rusty_board \
JWT_SECRET=my-secret \
cargo run --features web-axum,db-postgres,auth-jwt,media-local

# Or with .env file
cp .env.example .env
# edit .env
cargo run --features web-axum,db-postgres,auth-jwt,media-local
```

See `.env.example` in the repository root for all available variables.
