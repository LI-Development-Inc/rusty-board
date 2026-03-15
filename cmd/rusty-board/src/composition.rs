//! Composition root — the **only** file in the codebase that contains
//! `#[cfg(feature = "...")]` branches.
//!
//! This module:
//! 1. Reads `Settings` to get infrastructure coordinates
//! 2. Constructs all concrete adapter instances based on active features
//! 3. Injects adapters into generic service structs (monomorphization)
//! 4. Defines `*Deps` type aliases for multi-parameter services
//! 5. Returns the configured `axum::Router`
//!
//! # INVARIANT
//! Feature flag branches appear **only** here. Services, handlers, and adapters
//! never contain `#[cfg(feature)]`. See `ARCHITECTURE.md §5`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use configs::Settings;

// ── Services ─────────────────────────────────────────────────────────────────
use services::board::BoardService;
use services::moderation::ModerationService;
use services::post::PostService;
use services::thread::ThreadService;

// ── Health probes ─────────────────────────────────────────────────────────────
// Implemented here so composition can borrow the concrete pool types without
// creating a circular dependency between api-adapters ↔ storage-adapters.

#[cfg(feature = "db-postgres")]
struct PgProbe(sqlx::PgPool);

#[cfg(feature = "db-postgres")]
#[async_trait::async_trait]
impl api_adapters::axum::health::DatabaseProbe for PgProbe {
    async fn ping(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.0).await.is_ok()
    }
}

#[cfg(feature = "redis")]
struct RedisProbeImpl(deadpool_redis::Pool);

#[cfg(feature = "redis")]
#[async_trait::async_trait]
impl api_adapters::axum::health::RedisProbe for RedisProbeImpl {
    async fn ping(&self) -> bool {
        match self.0.get().await {
            Ok(mut conn) => deadpool_redis::redis::cmd("PING")
                .query_async::<String>(&mut conn)
                .await
                .is_ok(),
            Err(_) => false,
        }
    }
}
use services::user::UserService;

// ── Storage adapters (feature-gated) ─────────────────────────────────────────
#[cfg(feature = "db-postgres")]
use storage_adapters::postgres::{
    connection::create_pool,
    repositories::{
        PgAuditRepository, PgBanRepository, PgBoardRepository, PgFlagRepository,
        PgPostRepository, PgSessionRepository, PgStaffMessageRepository,
        PgStaffRequestRepository, PgThreadRepository, PgUserRepository,
    },
};

#[cfg(feature = "media-local")]
use storage_adapters::media::local_fs::LocalFsMediaStorage;

#[cfg(feature = "media-s3")]
use storage_adapters::media::s3::S3MediaStorage;

#[cfg(feature = "redis")]
use storage_adapters::redis::{connection::create_pool as create_redis_pool, RedisRateLimiter};

use storage_adapters::media::ImageMediaProcessor;

// ── Auth adapters (feature-gated) ─────────────────────────────────────────────
#[cfg(feature = "auth-jwt")]
use auth_adapters::jwt_bearer::JwtAuthProvider;

// ── Cache ─────────────────────────────────────────────────────────────────────
use storage_adapters::cache::BoardConfigCache;

// ── API adapters (feature-gated) ──────────────────────────────────────────────
#[cfg(feature = "web-axum")]
use axum::Router;

// ─── Type aliases for readable service instantiation ─────────────────────────

/// Concrete `PostService` type — local-fs media storage variant.
#[cfg(all(feature = "db-postgres", feature = "media-local", feature = "redis"))]
#[allow(dead_code)]
type AppPostService = PostService<
    PgPostRepository,
    PgThreadRepository,
    PgBanRepository,
    LocalFsMediaStorage,
    RedisRateLimiter,
    ImageMediaProcessor,
>;

/// Concrete `PostService` type — S3 media storage variant.
/// Only active when `media-s3` is enabled but `media-local` is not.
#[allow(dead_code)]
#[cfg(all(feature = "db-postgres", feature = "media-s3", feature = "redis", not(feature = "media-local")))]
type AppPostService = PostService<
    PgPostRepository,
    PgThreadRepository,
    PgBanRepository,
    S3MediaStorage,
    RedisRateLimiter,
    ImageMediaProcessor,
>;

/// Concrete `ModerationService` type (same repo types regardless of media backend).
#[cfg(feature = "db-postgres")]
#[allow(dead_code)]
type AppModerationService = ModerationService<
    PgBanRepository,
    PgPostRepository,
    PgThreadRepository,
    PgFlagRepository,
    PgAuditRepository,
    PgUserRepository,
>;

/// Compose all adapters and services, and return the configured router.
///
/// This is called once from `main.rs`. The returned `Router` is ready to serve requests.
///
/// # Panics
/// Panics on startup misconfiguration (missing env vars, unreachable DB, etc.).
/// This is intentional — a misconfigured application must not start silently.
pub async fn compose(settings: &Settings) -> anyhow::Result<Router> {
    // ── Database pool ─────────────────────────────────────────────────────────
    #[cfg(feature = "db-postgres")]
    let pool = {
        use secrecy::ExposeSecret;
        create_pool(
            settings.db_url.expose_secret(),
            settings.db_max_connections,
            settings.db_min_connections,
        )
        .await
        .context("failed to connect to PostgreSQL")?
    };

    // Run migrations
    #[cfg(feature = "db-postgres")]
    {
        sqlx::migrate!("../../crates/storage-adapters/src/migrations")
            .run(&pool)
            .await
            .context("failed to run database migrations")?;
    }

    // ── Repositories (Postgres) ───────────────────────────────────────────────
    #[cfg(feature = "db-postgres")]
    #[allow(unused_variables)]
    let (board_repo, thread_repo, post_repo, ban_repo, flag_repo, audit_repo, user_repo,
         session_repo, staff_request_repo, staff_message_repo) = {
        (
            PgBoardRepository::new(pool.clone()),
            PgThreadRepository::new(pool.clone()),
            PgPostRepository::new(pool.clone()),
            PgBanRepository::new(pool.clone()),
            PgFlagRepository::new(pool.clone()),
            PgAuditRepository::new(pool.clone()),
            PgUserRepository::new(pool.clone()),
            PgSessionRepository::new(pool.clone()),
            PgStaffRequestRepository::new(pool.clone()),
            PgStaffMessageRepository::new(pool.clone()),
        )
    };

    // ── Media storage ─────────────────────────────────────────────────────────
    #[cfg(feature = "media-local")]
    let media_storage = LocalFsMediaStorage::new(
        settings.media_path.clone(),
        settings.media_url_base.clone(),
    );

    #[cfg(feature = "media-s3")]
    let media_storage = {
        let s3_cfg = &settings.s3;
        use secrecy::ExposeSecret;
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(s3_cfg.region.clone()))
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                s3_cfg.access_key.expose_secret(),
                s3_cfg.secret_key.expose_secret(),
                None,
                None,
                "static",
            ))
            .load()
            .await;
        let client = if let Some(endpoint) = &s3_cfg.endpoint {
            aws_sdk_s3::Client::from_conf(
                aws_sdk_s3::config::Builder::from(&aws_config)
                    .endpoint_url(endpoint)
                    .build(),
            )
        } else {
            aws_sdk_s3::Client::new(&aws_config)
        };
        S3MediaStorage::new(client, s3_cfg.bucket.clone(), s3_cfg.endpoint.clone())
    };

    // ── Media processor ───────────────────────────────────────────────────────
    let media_processor = ImageMediaProcessor::new();

    // ── Rate limiter ──────────────────────────────────────────────────────────
    #[cfg(feature = "redis")]
    let rate_limiter = {
        use secrecy::ExposeSecret;
        let redis_pool = create_redis_pool(settings.redis_url.expose_secret())
            .context("failed to create Redis pool")?;
        RedisRateLimiter::new(redis_pool, 3) // default 3 posts/window; overridden per-board by BoardConfig
    };

    // ── Auth provider ─────────────────────────────────────────────────────────
    #[cfg(feature = "auth-jwt")]
    let auth_provider = {
        use secrecy::ExposeSecret;
        JwtAuthProvider::new(
            settings.jwt_secret.expose_secret().as_bytes(),
            settings.argon2_m_cost,
            settings.argon2_t_cost,
            settings.argon2_p_cost,
        )
    };

    // Cookie-session auth provider — replaces JWT when `auth-cookie` feature is active.
    // Uses PgSessionRepository for durable, revocable sessions.
    #[cfg(feature = "auth-cookie")]
    let auth_provider = {
        auth_adapters::cookie_session::CookieAuthProvider::new(
            session_repo.clone(),
            settings.cookie_session_ttl_secs.unwrap_or(604_800) as i64,
            settings.argon2_m_cost,
            settings.argon2_t_cost,
            settings.argon2_p_cost,
        )
    };

    // ── BoardConfig cache ─────────────────────────────────────────────────────
    let board_config_cache = Arc::new(BoardConfigCache::new(Duration::from_secs(settings.config_cache_ttl_secs)));

    // ── Services ──────────────────────────────────────────────────────────────
    let board_service = BoardService::new(board_repo.clone());
    let thread_service = ThreadService::new(thread_repo.clone(), post_repo.clone());
    let post_service = PostService::new(
        post_repo.clone(),
        thread_repo.clone(),
        ban_repo.clone(),
        media_storage,
        rate_limiter,
        media_processor,
        settings.tripcode_pepper.clone().unwrap_or_default(),
    );
    let moderation_service = ModerationService::new(
        ban_repo.clone(),
        post_repo.clone(),
        thread_repo.clone(),
        flag_repo.clone(),
        audit_repo.clone(),
        user_repo.clone(),
    );
    let user_service = UserService::new(
        user_repo.clone(),
        auth_provider.clone(),
        settings.jwt_ttl_secs,
    );

    // ── Prometheus metrics registry ───────────────────────────────────────────
    let mut metrics_registry = prometheus_client::registry::Registry::default();
    let _app_metrics = api_adapters::axum::metrics::AppMetrics::new(&mut metrics_registry);
    let metrics_registry = Arc::new(metrics_registry);

    // ── Health state (DB + Redis probes) ─────────────────────────────────────
    #[cfg(all(feature = "db-postgres", feature = "redis"))]
    let health_state = {
        use api_adapters::axum::health::HealthState;
        use secrecy::ExposeSecret;
        HealthState {
            db:    Arc::new(PgProbe(pool.clone())),
            redis: Arc::new(RedisProbeImpl(create_redis_pool(
                settings.redis_url.expose_secret(),
            ).context("health: failed to create redis pool")?)),
        }
    };

    // ── StaffRequestService — now backed by PgStaffRequestRepository ──────────
    let staff_request_svc = services::staff_request::StaffRequestService::new(
        staff_request_repo,
        user_repo.clone(),
    );

    // ── StaffMessageService — backed by PgStaffMessageRepository ─────────────
    let staff_message_svc = services::staff_message::StaffMessageService::new(
        staff_message_repo,
    );

    // ── Build router ──────────────────────────────────────────────────────────
    #[cfg(feature = "web-axum")]
    let router = build_axum_router(
        board_service,
        post_service,
        post_repo,
        thread_service,
        moderation_service,
        user_service,
        staff_request_svc,
        staff_message_svc,
        board_config_cache,
        Arc::new(auth_provider),
        metrics_registry,
        health_state,
        settings.open_registration,
    );

    Ok(router)
}

/// Build the Axum router with all routes and middleware.
///
/// Called once from `compose()`. All concrete adapter types are resolved at
/// the call site; this function receives them via generic parameters and
/// monomorphizes into a single concrete `Router`.
#[cfg(feature = "web-axum")]
// Composition root: wires all service and port instances into one Router.
// Each argument is a distinct generic type that cannot be collapsed without
// introducing a new struct that pushes the complexity elsewhere. This function
// is called exactly once at startup.
#[allow(clippy::too_many_arguments)]
fn build_axum_router<BS, PR, TR, BR, MS, RL, MP, FR, AR, UR, AP, RR, MR>(
    board_service:         BS,
    post_service:          PostService<PR, TR, BR, MS, RL, MP>,
    post_repo:             PR,
    thread_service:        services::thread::ThreadService<TR, PR>,
    moderation_service:    ModerationService<BR, PR, TR, FR, AR, UR>,
    user_service:          UserService<UR, AP>,
    staff_request_service: services::staff_request::StaffRequestService<RR, UR>,
    staff_message_service: services::staff_message::StaffMessageService<MR>,
    board_config_cache:    Arc<BoardConfigCache>,
    auth_provider:         Arc<dyn domains::ports::AuthProvider>,
    metrics_registry:      Arc<prometheus_client::registry::Registry>,
    health_state:          api_adapters::axum::health::HealthState,
    open_registration:     bool,
) -> Router
where
    // Board service
    BS: services::board::BoardRepo
        + api_adapters::axum::middleware::board_config::BoardConfigSource
        + 'static,
    // Thread repository (shared between ThreadService and PostService/ModerationService)
    TR: domains::ports::ThreadRepository + 'static,
    // Post service ports
    PR: domains::ports::PostRepository + Clone + 'static,
    BR: domains::ports::BanRepository + 'static,
    MS: domains::ports::MediaStorage + 'static,
    RL: domains::ports::RateLimiter + 'static,
    MP: domains::ports::MediaProcessor + 'static,
    // Moderation service additional ports
    FR: domains::ports::FlagRepository + 'static,
    AR: domains::ports::AuditRepository + 'static,
    // User service ports
    UR: domains::ports::UserRepository + 'static,
    AP: domains::ports::AuthProvider + 'static,
    // Staff request repository
    RR: domains::ports::StaffRequestRepository + 'static,
    // Staff message repository
    MR: domains::ports::StaffMessageRepository + 'static,
{
    use axum::{routing::get, Router};
    use tower_http::services::ServeDir;
    use tower_http::compression::CompressionLayer;
    use api_adapters::axum::{
        health::health_check,
        metrics::metrics_handler,
        middleware::{
            board_config::{BoardConfigState, board_config_middleware},
            security_headers::security_headers_middleware,
        },
        routes::{
            admin_routes::admin_routes,
            auth_routes::auth_routes,
            board_owner_routes::board_owner_routes,
            board_routes::{board_admin_routes, board_public_routes},
            moderation_routes::moderation_routes,
            overboard_routes::overboard_routes,
            post_routes::post_routes,
            staff_message_routes::staff_message_routes,
            thread_routes::thread_routes,
            user_routes::user_routes,
        },
    };
    use tower_http::{
        request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
        trace::TraceLayer,
    };
    use axum::middleware as axum_middleware;
    use axum::extract::DefaultBodyLimit;

    let board_svc  = Arc::new(board_service);
    let thread_svc = Arc::new(thread_service);
    let post_svc   = Arc::new(post_service);
    let mod_svc    = Arc::new(moderation_service);
    let user_svc         = Arc::new(user_service);
    let request_svc      = Arc::new(staff_request_service);
    let message_svc      = Arc::new(staff_message_service);

    // Board config middleware state — resolves `:slug` → BoardId + BoardConfig
    let board_config_source: Arc<dyn api_adapters::axum::middleware::board_config::BoardConfigSource> =
        board_svc.clone();
    let board_config_state = BoardConfigState {
        source: board_config_source,
        cache:  board_config_cache.clone(),
    };

    // Board-scoped routes need the board_config middleware to inject ExtractedBoardConfig
    let board_scoped = Router::new()
        .merge(thread_routes(thread_svc.clone()))
        .merge(post_routes(post_svc.clone()))
        .merge(board_owner_routes(board_svc.clone(), request_svc.clone()))
        .route_layer(axum_middleware::from_fn_with_state(
            board_config_state,
            board_config_middleware,
        ));

    // Build route groups
    let public_routes = Router::new()
        .route("/", get(|| async {
            axum::response::Redirect::to("/overboard")
        }))
        .route("/healthz", get(health_check).with_state(health_state))
        .merge(board_public_routes(board_svc.clone(), post_repo.clone()))
        .merge(overboard_routes(board_svc.clone(), post_svc.clone()))
        .merge(board_scoped);

    let auth_router   = auth_routes(user_svc.clone(), open_registration);
    let admin_router  = admin_routes(user_svc.clone(), board_svc.clone(), request_svc.clone());
    let board_admin_r = board_admin_routes(board_svc.clone());
    let mod_router    = moderation_routes(mod_svc.clone(), board_svc.clone());
    let user_router   = user_routes(user_svc.clone(), request_svc.clone());
    let msg_router    = staff_message_routes(message_svc.clone());

    let auth_for_middleware = auth_provider.clone();

    // Combine all routes and apply global middleware
    let base_router = Router::new()
        // Static assets: CSS, JS, favicon
        .nest_service("/static", ServeDir::new("static"));

    #[cfg(feature = "media-local")]
    let base_router = base_router
        // Local media files (only active with media-local feature; S3 uses signed URLs)
        .nest_service("/media", ServeDir::new("media"));

    base_router
        // Prometheus metrics — scoped state so it doesn't pollute the parent router
        .merge(
            Router::new()
                .route("/metrics", get(metrics_handler))
                .with_state(metrics_registry),
        )
        .merge(public_routes)
        .merge(auth_router)
        .merge(admin_router)
        .merge(board_admin_r)
        .merge(mod_router)
        .merge(user_router)
        .merge(msg_router)
        // Soft auth middleware — injects CurrentUser into extensions if token valid.
        // Never rejects — individual extractors (AuthenticatedUser, ModeratorUser, AdminUser)
        // enforce role requirements per-route.
        .layer(axum_middleware::from_fn(move |req, next| {
            let provider = auth_for_middleware.clone();
            async move {
                api_adapters::axum::middleware::auth::auth_middleware(provider, req, next).await
            }
        }))
        // Security response headers on every response
        .layer(axum_middleware::from_fn(security_headers_middleware))
        // Allow multipart uploads up to 12 MB (board max is 10 MB; the extra
        // 2 MB covers multipart boundary overhead and multiple small files).
        // Without this, Axum's default 2 MB limit rejects image uploads silently.
        .layer(DefaultBodyLimit::max(12 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(PropagateRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
        ))
        // Redirect trailing-slash URLs to their canonical (no-slash) equivalent.
        // Using .fallback() rather than middleware because axum 0.8 Router::layer()
        // wraps matched handlers; unmatched /board/b/ never triggers middleware.
        // The fallback catches the 404 path and issues a 308 permanent redirect.
        .fallback(trailing_slash_redirect)
}

/// Fallback handler: redirect any path that ends with `/` to the canonical
/// path without the trailing slash.  Runs only for *unmatched* routes — axum's
/// Router does not match `/board/b/` when the route is declared as `/board/:slug`,
/// so trailing-slash URLs fall through here and receive a 308 Permanent Redirect.
async fn trailing_slash_redirect(uri: axum::http::Uri) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};
    let path = uri.path();
    if path.len() > 1 && path.ends_with('/') {
        let new_path = path.trim_end_matches('/');
        let location = match uri.query() {
            Some(q) => format!("{new_path}?{q}"),
            None    => new_path.to_owned(),
        };
        (
            StatusCode::PERMANENT_REDIRECT,
            [(axum::http::header::LOCATION, location)],
        ).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

