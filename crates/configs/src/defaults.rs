//! Default values for `Settings` fields.
//!
//! These functions are referenced by `#[serde(default = "defaults::fn_name")]`
//! attributes on each field in `Settings`. Extracting them to a separate module
//! keeps `lib.rs` focused on the struct definition and its `load()` logic.
//!
//! All defaults are conservative and appropriate for a single-node development
//! deployment. Production operators should override them via environment variables.

use std::path::PathBuf;

/// HTTP bind address. Default: listen on all interfaces.
pub fn host() -> String {
    "0.0.0.0".to_owned()
}

/// HTTP port. Default: 8080.
pub fn port() -> u16 {
    8080
}

/// Graceful shutdown drain timeout in seconds.
/// In-flight requests are given this long to complete after a SIGTERM.
pub fn shutdown_timeout_secs() -> u64 {
    30
}

/// Maximum database pool connections.
pub fn db_max_connections() -> u32 {
    10
}

/// Minimum idle database pool connections.
pub fn db_min_connections() -> u32 {
    2
}

/// JWT lifetime in seconds. Default: 24 hours.
pub fn jwt_ttl_secs() -> u64 {
    86_400
}

/// Argon2id memory cost (KiB). OWASP minimum: 19456 KiB (19 MiB).
pub fn argon2_m_cost() -> u32 {
    19_456
}

/// Argon2id iteration count. OWASP minimum: 2.
pub fn argon2_t_cost() -> u32 {
    2
}

/// Argon2id parallelism. OWASP minimum: 1.
pub fn argon2_p_cost() -> u32 {
    1
}

/// Local filesystem media storage path (used when `media-local` feature is active).
pub fn media_path() -> PathBuf {
    PathBuf::from("./media")
}

/// Base URL for serving locally stored media files.
pub fn media_url_base() -> String {
    "/media".to_owned()
}

/// Pre-signed S3 URL TTL in seconds.
pub fn media_url_ttl_secs() -> u64 {
    86_400
}

/// Maximum width (and height) of generated thumbnails in pixels.
pub fn thumbnail_width_px() -> u32 {
    320
}

/// JPEG thumbnail quality (0–100). Higher = better quality, larger file.
pub fn thumbnail_quality() -> u8 {
    85
}

/// How often the in-memory IP hashing salt rotates in seconds.
/// Default: once per day (86400 seconds). Set to 0 to never rotate (not recommended).
pub fn ip_salt_rotation_secs() -> u64 {
    86_400
}

/// `BoardConfig` cache TTL in seconds.
/// Dashboard updates take effect within this window on all instances.
pub fn config_cache_ttl_secs() -> u64 {
    60
}

/// Whether public self-registration is open by default.
/// Operators can set `OPEN_REGISTRATION=false` to disable it.
pub fn open_registration() -> bool {
    true
}
