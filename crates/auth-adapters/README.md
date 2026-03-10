# `auth-adapters` — Authentication Adapters

Concrete implementations of the `AuthProvider` port. Feature-gated — select exactly one auth mechanism at compile time.

---

## Feature Flags

| Flag | Adapter | Mechanism |
|------|---------|-----------|
| `auth-jwt` | `JwtAuthProvider` | Stateless JWT bearer tokens + argon2id |
| `auth-cookie` | `CookieAuthProvider<SR>` | Server-side sessions with CSRF double-submit protection |

Only one should be enabled at a time. The default build uses `auth-jwt`.

---

## Module Map

```
src/
├── jwt_bearer/              # feature: auth-jwt
│   ├── mod.rs               # JwtAuthProvider — sign/verify JWTs, hash passwords
│   └── errors.rs            # AuthError variants
├── cookie_session/          # feature: auth-cookie (v1.1+)
│   └── mod.rs               # CookieAuthProvider<SR: SessionRepository>
└── common/
    ├── hashing.rs           # argon2id hash + verify (shared, always compiled)
    └── utils.rs             # Claims struct, TTL helpers
```

---

## `JwtAuthProvider` (`auth-jwt`)

Implements `AuthProvider` from `domains::ports`.

- **Tokens**: HS256 JWTs signed with the instance secret from `Settings.jwt_secret`
- **Claims**: `sub` (UserId), `role`, `owned_boards: Vec<BoardId>`, `exp`
- **Password hashing**: argon2id with tuned parameters (memory: 64MB, iterations: 3, parallelism: 4)
- **TTL**: configurable via `Settings.jwt_ttl_secs` (default: 86400 = 24 hours)

The JWT carries `owned_boards` so permission checks (`can_manage_board_config`) require no extra DB round-trip per request.

### Token lifecycle

```
POST /auth/login  →  UserService::login()  →  AuthProvider::create_token()  →  JWT string
GET  /board/:slug/thread/:id  →  auth middleware reads Authorization: Bearer <token>
                               →  AuthProvider::verify_token()  →  CurrentUser extension
```

---

## `CookieAuthProvider` (`auth-cookie`, v1.1+)

Implements `AuthProvider` with server-side session storage. Generic over `SessionRepository`:

```rust
pub struct CookieAuthProvider<SR: SessionRepository> {
    session_repo: SR,
    secret: String,
    ttl_secs: u64,
}
```

- **Session storage**: `InMemorySessionRepository` (dev) or `PgSessionRepository` (prod)
- **CSRF protection**: double-submit cookie pattern — CSRF token in both cookie and `X-CSRF-Token` header
- **Session revocation**: `logout()` calls `SessionRepository::delete()` — token is immediately invalid

---

## `common/hashing.rs`

argon2id password hashing shared between both adapters. Not feature-gated.

```rust
pub fn hash_password(password: &str) -> Result<PasswordHash, AuthError>;
pub fn verify_password(password: &str, hash: &PasswordHash) -> Result<bool, AuthError>;
```

Parameters match OWASP recommendations: memory 65536 KiB, iterations 3, parallelism 4.

---

## Invariants

1. No business logic. Password validation (minimum length, complexity) belongs in `UserService`.
2. Returns `DomainError` at the port boundary — never exposes `jsonwebtoken::Error` or argon2 errors to callers.
3. `#[cfg(feature)]` only in `lib.rs` module declarations, not inside method bodies.

---

## Testing

Run: `cargo test -p auth-adapters`

Current: **5 tests** — JWT roundtrip, expired token, wrong secret, hash roundtrip, wrong password.
