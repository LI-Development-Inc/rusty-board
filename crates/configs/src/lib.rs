//! `configs` — infrastructure configuration for rusty-board.
//!
//! `Settings` is loaded at startup from environment variables and an optional
//! `.env` file. It contains **only** infrastructure configuration — connection
//! URLs, secrets, resource limits, and processing parameters.
//!
//! Per-board behavioural configuration lives in `BoardConfig` in the database.
//! Never add per-board toggles to `Settings`.
//!
//! # Environment variable naming
//! Flat top-level fields use `SNAKE_UPPER_CASE` (e.g. `DB_URL`, `JWT_SECRET`).
//! Nested struct fields (e.g. `S3Config`) use `DOUBLE__UNDERSCORE` to separate levels
//! (e.g. `S3__BUCKET`, `S3__REGION`). Single underscores within a name are preserved.

use secrecy::SecretString;
use serde::Deserialize;
use std::path::PathBuf;

/// Infrastructure configuration for a rusty-board deployment.
///
/// Loaded once at startup via `Settings::load()`. Immutable after loading.
/// Accessed in `composition.rs` to construct concrete adapter instances.
///
/// # INVARIANT
/// `Settings` contains only infrastructure configuration.
/// Per-board behavioural configuration belongs in `BoardConfig`, not here.
#[derive(Debug, Deserialize)]
pub struct Settings {
    // ── Server ────────────────────────────────────────────────────────────
    /// Address to bind. Default: `0.0.0.0`.
    #[serde(default = "defaults::host")]
    pub host: String,

    /// Port to bind. Default: `8080`.
    #[serde(default = "defaults::port")]
    pub port: u16,

    /// How long to wait for in-flight requests during graceful shutdown (seconds).
    /// Default: 30.
    #[serde(default = "defaults::shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,

    // ── Database ──────────────────────────────────────────────────────────
    /// PostgreSQL database URL. Required.
    pub db_url: SecretString,

    /// Maximum database pool connections. Default: 10.
    #[serde(default = "defaults::db_max_connections")]
    pub db_max_connections: u32,

    /// Minimum database pool connections. Default: 2.
    #[serde(default = "defaults::db_min_connections")]
    pub db_min_connections: u32,

    // ── Redis ─────────────────────────────────────────────────────────────
    /// Redis URL. Required when compiled with `redis` feature.
    #[cfg(feature = "redis")]
    pub redis_url: SecretString,

    // ── Auth ──────────────────────────────────────────────────────────────
    /// JWT signing secret. Required when compiled with `auth-jwt` feature.
    #[cfg(feature = "auth-jwt")]
    pub jwt_secret: SecretString,

    /// JWT token validity period in seconds. Default: 86400 (24h).
    #[serde(default = "defaults::jwt_ttl_secs")]
    pub jwt_ttl_secs: u64,

    /// Cookie session TTL in seconds. Only used when `auth-cookie` feature is active.
    /// Default: `None` (falls back to 7 days = 604800 seconds in composition.rs).
    #[serde(default)]
    pub cookie_session_ttl_secs: Option<u64>,

    /// Server-side secret used for `##` secure tripcodes.
    ///
    /// When set, `##password` trips are computed as `SHA-256(pepper || "::" || password)`.
    /// This makes tripcodes server-specific — the same password produces different trips
    /// on different servers. If unset or empty, `##` degrades to `SHA-256("::" || password)`.
    ///
    /// **Changing this value invalidates all existing `##` tripcodes on the site.**
    /// Set once and do not rotate.
    #[serde(default)]
    pub tripcode_pepper: Option<String>,

    /// Argon2id memory cost in kilobytes. Default: 19456 (OWASP recommended).
    #[serde(default = "defaults::argon2_m_cost")]
    pub argon2_m_cost: u32,

    /// Argon2id time cost (iterations). Default: 2 (OWASP recommended).
    #[serde(default = "defaults::argon2_t_cost")]
    pub argon2_t_cost: u32,

    /// Argon2id parallelism cost. Default: 1.
    #[serde(default = "defaults::argon2_p_cost")]
    pub argon2_p_cost: u32,

    // ── Media storage ─────────────────────────────────────────────────────
    /// S3 configuration. Required when compiled with `media-s3` feature.
    #[cfg(feature = "media-s3")]
    pub s3: S3Config,

    /// Local media storage path. Required when compiled with `media-local` feature.
    #[cfg(feature = "media-local")]
    #[serde(default = "defaults::media_path")]
    pub media_path: PathBuf,

    /// Base URL for serving local media files. Default: `/media`.
    #[serde(default = "defaults::media_url_base")]
    pub media_url_base: String,

    /// Presigned URL TTL for S3 media (seconds). Default: 86400 (24h).
    #[serde(default = "defaults::media_url_ttl_secs")]
    pub media_url_ttl_secs: u64,

    // ── Media processing ──────────────────────────────────────────────────
    /// Generated thumbnail width in pixels. Default: 320.
    #[serde(default = "defaults::thumbnail_width_px")]
    pub thumbnail_width_px: u32,

    /// Thumbnail quality / compression level. Default: 85.
    #[serde(default = "defaults::thumbnail_quality")]
    pub thumbnail_quality: u8,

    // ── IP privacy ────────────────────────────────────────────────────────
    /// How often the IP hash salt rotates (seconds). Default: 86400 (24h).
    #[serde(default = "defaults::ip_salt_rotation_secs")]
    pub ip_salt_rotation_secs: u64,

    // ── BoardConfig cache ─────────────────────────────────────────────────
    /// In-process BoardConfig cache TTL in seconds. Default: 60.
    #[serde(default = "defaults::config_cache_ttl_secs")]
    pub config_cache_ttl_secs: u64,

    // ── Registration ──────────────────────────────────────────────────────
    /// Allow public self-registration at `POST /auth/register`.
    ///
    /// When `true` (default), any visitor can create a `Role::User` account.
    /// Set `OPEN_REGISTRATION=false` to close registration; accounts can
    /// still be created by an admin via `POST /admin/users`.
    #[serde(default = "defaults::open_registration")]
    pub open_registration: bool,
}

/// S3 / S3-compatible storage credentials and configuration.
#[derive(Debug, Deserialize)]
pub struct S3Config {
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region (e.g. `us-east-1`). For MinIO, can be any non-empty string.
    pub region: String,
    /// Custom endpoint URL for S3-compatible services (e.g. MinIO). Optional.
    pub endpoint: Option<String>,
    /// S3 access key.
    pub access_key: SecretString,
    /// S3 secret key.
    pub secret_key: SecretString,
}

impl Settings {
    /// Load settings from environment variables and an optional `.env` file.
    ///
    /// Reads `.env` if present (does not fail if absent). Environment variables
    /// override `.env` file values.
    ///
    /// Returns an error if required variables are missing or cannot be parsed.
    pub fn load() -> Result<Self, config::ConfigError> {
        // Load .env file if present (silently skip if absent)
        let _ = dotenvy::dotenv();

        config::Config::builder()
            .add_source(
                config::Environment::default()
                    .separator("__"),  // double underscore separates nested segments
            )
            .build()?
            .try_deserialize()
    }
}

/// Default value functions used by serde.
pub mod defaults;
