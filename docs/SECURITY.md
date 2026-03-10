# SECURITY.md
# rusty-board — Security Model and Disclosure

## Reporting a Vulnerability

If you discover a security vulnerability in rusty-board, **do not open a public GitHub issue.**

Email the maintainers at: **security@your-domain.example**

Please include:
- A description of the vulnerability and its impact
- Reproduction steps or proof-of-concept (if possible)
- The version or commit hash you tested against

You should receive an acknowledgement within 48 hours. We aim to issue a fix or mitigation within 14 days for high-severity issues.

---

## Security Model (v1.0)

### Identity

rusty-board is an anonymous-first imageboard. **No user accounts exist for posters.** Staff accounts authenticate via JWT HS256 bearer tokens issued on login. Five roles exist: Admin, Janitor, BoardOwner, BoardVolunteer, User (registered account with no moderation powers).

| Principal | Authentication | Persistence |
|-----------|---------------|-------------|
| Anonymous poster | None | None |
| Staff (Admin / Janitor / BoardOwner / BoardVolunteer) | JWT bearer token | User row in DB |

### IP Address Handling

**Raw IP addresses are never stored.** On every incoming request, the client IP is:

1. Extracted from the peer address (or `X-Forwarded-For` behind a reverse proxy)
2. Immediately hashed: `SHA-256(raw_ip + daily_salt)`
3. The hash is stored; the raw IP is discarded

The daily salt rotates at UTC midnight and is held only in process memory. This means:
- IP hashes from different days for the same IP are unlinkable
- A database dump reveals no raw IPs
- After a server restart, the salt is lost and previous hashes are irrecoverable

### Password Hashing

Staff passwords are hashed with **Argon2id** (OWASP-recommended parameters):
- Memory: 19,456 KiB
- Iterations: 2
- Parallelism: 1

### EXIF Stripping

All uploaded images pass through `ImageMediaProcessor` before storage. EXIF metadata is stripped **unconditionally** by the media processing pipeline regardless of any `BoardConfig` toggle. This is an architectural invariant enforced at the code level, not a configuration option.

### Transport Security

rusty-board does not handle TLS directly — terminate TLS at a reverse proxy (nginx, Caddy, Traefik). The application assumes it is running behind such a proxy in production.

### HTTP Security Headers

Applied to **every response** by `security_headers_middleware`:

| Header | Value |
|--------|-------|
| `Content-Security-Policy` | `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'` |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `SAMEORIGIN` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |

### XSS Prevention

All HTML output is rendered by **Askama** templates which HTML-escape variable substitutions at compile time. Direct string interpolation into HTML is structurally prevented.

### CSRF

CSRF protection applies only to cookie-based session auth (v1.1+). The v1.0 JWT bearer token scheme is not vulnerable to CSRF — browsers do not automatically send `Authorization` headers.

### Rate Limiting

Per-IP, per-board rate limiting is enforced by `RateLimiter` port (backed by Redis in production). The limit parameters are configurable per board via `BoardConfig`.

Rate limiting can be bypassed if the Redis adapter fails — the default failure mode is **fail-open** (allow the post) to avoid Redis outages causing full posting outages. This is a deliberate trade-off.

### Secret Management

- `JWT_SECRET` is loaded from the environment and wrapped in `secrecy::Secret<String>` which prevents accidental logging via `Debug`/`Display`
- S3 credentials similarly wrapped
- `.env.example` contains no real secrets — only placeholder values

### Docker Image Security

- Multi-stage build — only the final binary is in the runtime image
- Runtime image: `debian:bookworm-slim` with `libssl` and `ca-certificates`
- Binary runs as non-root user `rusty` (uid 10001)
- No shell, no package manager in the runtime layer

---

## Known Limitations (v1.0)

| Limitation | Severity | Planned Fix |
|------------|----------|-------------|
| No CAPTCHA support | Medium | v1.1 — `BoardConfig.captcha_required` stub present |
| Rate limiter fail-open on Redis outage | Low | Accepted trade-off; monitor Redis health |
| No brute-force protection on login | Low | v1.1 — account lockout after N failures |
| SHA-256 IP hashing (not SHA-3) | Info | Cryptographically sufficient for this use case |
| No Tor/proxy detection (DNSBL) | Low | v1.2 — port defined, adapter not yet built |
| JWT `HS256` (symmetric) | Info | Sufficient for single-deployment; RS256 for v1.1 if needed |

---

## Dependency Auditing

Run `cargo audit` to check for known vulnerabilities in all workspace dependencies. The CI pipeline fails on any unresolved high or critical advisory.
