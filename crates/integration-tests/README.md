# `integration-tests` — Integration & Contract Tests

In-process integration tests for all route groups. No external dependencies — stub repositories replace real DB and storage adapters.

---

## Test Files

| File | Coverage | Tests |
|------|----------|-------|
| `api_admin.rs` | Admin routes: user CRUD, board ownership, audit log | 10 |
| `api_auth.rs` | Auth routes: login, register, refresh, pages | 10 |
| `api_board.rs` | Board CRUD, search | 12 |
| `api_board_owner.rs` | Board config, volunteers | 7 |
| `api_moderation.rs` | Flags, bans, delete, sticky, close, dashboards | 16 |
| `api_post.rs` | Post creation, rate limiting, ban enforcement | 5 |
| `api_user.rs` | Staff request submission, user dashboard | 9 |
| `media_upload.rs` | File attachment upload, MIME validation | 3 |
| `board_service.rs` | `BoardService` unit tests via mock repos | 13 |
| `thread_service.rs` | `ThreadService` unit tests | 9 |
| `post_service.rs` | `PostService` unit tests: all config branches | 13 |
| `domain_models.rs` | Domain model invariants | 39 |
| `port_contracts.rs` | Stub adapters satisfy port contracts | 4 |
| `fixtures.rs` | Test fixture correctness | 4 |
| `utils.rs` | `common/utils.rs` functions | 21 |

**Total: 175 tests, all passing.**

---

## Approach

Tests use Axum's `tower::ServiceExt::oneshot()` to send requests directly to the in-process router — no real TCP, no real DB, no real media storage.

Each test file constructs a minimal app with stub repositories:

```rust
fn mod_app() -> Router {
    moderation_routes(
        Arc::new(NopModService::new()),
        Arc::new(NopBoardService::new()),
    )
}

#[tokio::test]
async fn delete_post_returns_204() {
    let resp = mod_app()
        .oneshot(with_mod_user(plain_post("/mod/posts/00000000-0000-0000-0000-000000000001/delete")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
```

---

## Stub Repositories

All stubs implement the full port trait contract. New port methods must be added to every stub immediately — the compiler enforces this.

Stubs return either `Ok(fixture_value)` or `Err(DomainError::not_found(...))` depending on what the test requires. They never panic unexpectedly — `unimplemented!()` is only used for methods that the test in question genuinely cannot reach.

---

## Adding a Test

1. Use an existing `*_app()` helper or create one for the route group under test
2. Build the request with `json_post()`, `plain_post()`, or `Request::builder()` directly
3. Inject auth context with `with_admin_user()`, `with_mod_user()`, `with_board_owner_user()`, or `with_vol_user()` as needed
4. Assert status code and, where relevant, response body
5. Name the test `<action>_<returns>_<condition>` — e.g. `toggle_sticky_returns_204`

---

## Running

```sh
# All integration tests
cargo test -p integration-tests

# Single file
cargo test -p integration-tests --test api_moderation

# Single test
cargo test -p integration-tests --test api_moderation toggle_sticky
```
