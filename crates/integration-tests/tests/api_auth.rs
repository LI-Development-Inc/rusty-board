//! Integration tests for `POST /auth/login` and `POST /auth/refresh`.
//!
//! These tests build a minimal Axum router with hand-rolled stub implementations
//! of `UserRepository` and `AuthProvider`. No real database or JWT signing is
//! required — the stubs return canned values so we can exercise the full
//! request → handler → response pipeline.

use api_adapters::axum::routes::auth_routes::auth_routes;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use domains::{
    errors::DomainError,
    models::*,
    ports::{AuthProvider, UserRepository},
};
use services::user::UserService;
use std::sync::Arc;
use tower::ServiceExt;

// ─── Stubs ────────────────────────────────────────────────────────────────────

struct AlwaysOkUserRepo {
    user: User,
}

impl AlwaysOkUserRepo {
    fn admin() -> Self {
        Self {
            user: User {
                id:            UserId(uuid::Uuid::new_v4()),
                username:      "admin".to_owned(),
                password_hash: PasswordHash::new(
                    "$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash",
                ),
                role:          Role::Admin,
                is_active:     true,
                created_at:    Utc::now(),
            },
        }
    }
}

#[async_trait::async_trait]
impl UserRepository for AlwaysOkUserRepo {
    async fn find_by_id(&self, _: UserId) -> Result<User, DomainError> {
        Ok(self.user.clone())
    }
    async fn find_by_username(&self, _: &str) -> Result<User, DomainError> {
        Ok(self.user.clone())
    }
    async fn find_all(&self, page: Page) -> Result<Paginated<User>, DomainError> {
        Ok(Paginated::new(vec![self.user.clone()], 1, page, 15))
    }
    async fn save(&self, _: &User) -> Result<(), DomainError> { Ok(()) }
    async fn deactivate(&self, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn find_owned_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn find_volunteer_boards(&self, _: UserId) -> Result<Vec<BoardId>, DomainError> { Ok(vec![]) }
    async fn add_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_volunteer(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn add_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
    async fn remove_board_owner(&self, _: BoardId, _: UserId) -> Result<(), DomainError> { Ok(()) }
}

/// `AuthProvider` that always succeeds — returns a canned token / claims.
struct AlwaysOkAuth;

#[async_trait::async_trait]
impl AuthProvider for AlwaysOkAuth {
    async fn create_token(&self, claims: &Claims) -> Result<Token, DomainError> {
        Ok(Token::new(format!("fake.token.{}", claims.role as u8)))
    }
    async fn verify_token(&self, token: &Token) -> Result<Claims, DomainError> {
        if token.0.starts_with("fake.token") {
            Ok(Claims {
                user_id:      UserId(uuid::Uuid::new_v4()),
        username: "testuser".into(),
                role:         Role::Admin,
                owned_boards: vec![],
        volunteer_boards: vec![],
                exp:          (Utc::now() + chrono::Duration::hours(24)).timestamp(),
            })
        } else {
            Err(DomainError::auth())
        }
    }
    async fn hash_password(&self, password: &str) -> Result<PasswordHash, DomainError> {
        Ok(PasswordHash::new(format!("hashed:{password}")))
    }
    async fn verify_password(&self, _password: &str, _hash: &PasswordHash) -> Result<(), DomainError> {
        // Always succeed so we don't need real argon2 in tests.
        Ok(())
    }
}

/// `AuthProvider` that always fails token verification.
struct AlwaysBadAuth;

#[async_trait::async_trait]
impl AuthProvider for AlwaysBadAuth {
    async fn create_token(&self, _: &Claims) -> Result<Token, DomainError> {
        Err(DomainError::internal("no"))
    }
    async fn verify_token(&self, _: &Token) -> Result<Claims, DomainError> {
        Err(DomainError::auth())
    }
    async fn hash_password(&self, _: &str) -> Result<PasswordHash, DomainError> {
        Err(DomainError::internal("no"))
    }
    async fn verify_password(&self, _: &str, _: &PasswordHash) -> Result<(), DomainError> {
        Err(DomainError::auth())
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn app_ok() -> axum::Router {
    let svc = Arc::new(UserService::new(AlwaysOkUserRepo::admin(), AlwaysOkAuth, 3600));
    auth_routes(svc, true)
        .layer(axum::Extension(api_adapters::axum::middleware::login_guard::LoginGuard::new()))
}

fn json_body(body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn login_returns_200_with_token_on_valid_credentials() {
    let app = app_ok();
    let body = r#"{"username":"admin","password":"correct"}"#;
    let resp = app.oneshot(json_body(body)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["token"].as_str().is_some(),
        "response must have a token field"
    );
    assert!(json["expires_at"].as_i64().is_some());
}

#[tokio::test]
async fn login_returns_401_when_credentials_invalid() {
    // Use a repo that returns a user but an auth provider that denies the password.
    let svc = Arc::new(UserService::new(AlwaysOkUserRepo::admin(), AlwaysBadAuth, 3600));
    let app = auth_routes(svc, true)
        .layer(axum::Extension(api_adapters::axum::middleware::login_guard::LoginGuard::new()));

    let body = r#"{"username":"admin","password":"wrong"}"#;
    let resp = app
        .oneshot(json_body(body))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_returns_422_on_missing_fields() {
    let app = app_ok();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"username":"admin"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Missing `password` field → unprocessable entity
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn login_page_returns_200() {
    let app = app_ok();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected HTML, got: {ct}");
}

#[tokio::test]
async fn refresh_requires_authentication() {
    let app = app_ok();
    // No Authorization header — refresh_token requires a valid CurrentUser extension,
    // which only exists after auth middleware runs.  Without middleware the extension
    // is absent → 401.
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/refresh")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Register tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn register_page_returns_200_when_open() {
    let app = app_ok(); // open_registration=true
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/register")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected HTML, got: {ct}");
}

#[tokio::test]
async fn register_page_returns_404_when_closed() {
    let svc = Arc::new(UserService::new(AlwaysOkUserRepo::admin(), AlwaysOkAuth, 3600));
    let app = auth_routes(svc, false); // registration closed
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/register")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn register_returns_201_on_valid_request() {
    let app = app_ok();
    let body = r#"{"username":"newuser","password":"strongpassword1234","password_confirm":"strongpassword1234"}"#;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn register_returns_400_when_passwords_do_not_match() {
    let app = app_ok();
    let body = r#"{"username":"newuser","password":"strongpassword1234","password_confirm":"differentpass"}"#;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_returns_422_on_missing_fields() {
    let app = app_ok();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"username":"newuser"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
