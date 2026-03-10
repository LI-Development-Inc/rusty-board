//! `rusty-board` — main entry point.
//!
//! Responsibilities:
//! 1. Load `Settings` from environment variables
//! 2. Initialise structured tracing
//! 3. Call `composition::compose()` to build all concrete adapters and services
//! 4. Start the HTTP server
//! 5. Graceful shutdown on SIGTERM or Ctrl-C
//!
//! This file contains the tokio runtime and server binding. All adapter
//! selection and dependency wiring lives in `composition.rs`.

mod composition;

use anyhow::Context;
use configs::Settings;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Crypto provider ───────────────────────────────────────────────────────
    // aws-sdk-s3 and jsonwebtoken 10 both use rustls. Without an explicit
    // provider, rustls panics if multiple candidates exist (ring vs aws-lc-rs).
    // Install ring as the process-default before any network or JWT operations.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // ── Tracing ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rusty_board=info,tower_http=debug".into()),
        )
        .json()
        .init();

    // ── Settings ──────────────────────────────────────────────────────────────
    let settings = Settings::load().context("failed to load settings")?;

    // Log which features are compiled in at startup
    log_compiled_features();

    // ── Compose ───────────────────────────────────────────────────────────────
    let router = composition::compose(&settings).await.context("failed to compose application")?;

    // ── Bind ──────────────────────────────────────────────────────────────────
    let addr = format!("{}:{}", settings.host, settings.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind to {addr}"))?;

    info!(addr = %addr, "rusty-board started");

    // ── Serve with graceful shutdown ──────────────────────────────────────────
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(settings.shutdown_timeout_secs))
    .await
    .context("server error")?;

    info!("rusty-board shut down cleanly");
    Ok(())
}

/// Wait for SIGTERM or Ctrl-C, then give in-flight requests time to drain.
async fn shutdown_signal(timeout_secs: u64) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c  => {},
        _ = sigterm => {},
    }

    info!(
        timeout_secs,
        "shutdown signal received; draining in-flight requests"
    );
    tokio::time::sleep(std::time::Duration::from_secs(timeout_secs)).await;
}

/// Log which Cargo features were compiled into this binary.
///
/// Useful for diagnosing deployment issues where the wrong binary was deployed.
/// Each push is guarded by a `#[cfg(feature = "...")]` attribute.
/// `vec![]` literals cannot contain cfg attributes, so the init-then-push
/// pattern is the only way to build this list conditionally.
#[allow(clippy::vec_init_then_push)]
fn log_compiled_features() {
    let mut features: Vec<&str> = Vec::new();

    #[cfg(feature = "web-axum")]
    features.push("web-axum");
    #[cfg(feature = "web-actix")]
    features.push("web-actix");
    #[cfg(feature = "db-postgres")]
    features.push("db-postgres");
    #[cfg(feature = "db-sqlite")]
    features.push("db-sqlite");
    #[cfg(feature = "auth-jwt")]
    features.push("auth-jwt");
    #[cfg(feature = "auth-cookie")]
    features.push("auth-cookie");
    #[cfg(feature = "media-s3")]
    features.push("media-s3");
    #[cfg(feature = "media-local")]
    features.push("media-local");
    #[cfg(feature = "video")]
    features.push("video");
    #[cfg(feature = "documents")]
    features.push("documents");
    #[cfg(feature = "redis")]
    features.push("redis");

    info!(features = ?features, "compiled features");
}
