//! Local filesystem media storage adapter (`media-local` feature).
//!
//! Stores media files in a configurable directory on the local filesystem.
//! Intended for development and single-instance small deployments.
//! In production, use `S3MediaStorage` (`media-s3` feature).

use async_trait::async_trait;
use bytes::Bytes;
use domains::errors::DomainError;
use domains::models::MediaKey;
use domains::ports::MediaStorage;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, instrument};

/// Local filesystem media storage.
///
/// Files are stored under `base_path/<key>`. The `get_url` method returns
/// a static path prefixed by `public_url_base` (e.g. `/media/`). TTL is ignored.
pub struct LocalFsMediaStorage {
    base_path:       PathBuf,
    public_url_base: String,
}

impl LocalFsMediaStorage {
    /// Create a new `LocalFsMediaStorage`.
    ///
    /// `base_path` is the root directory where files are stored.
    /// `public_url_base` is the URL prefix returned by `get_url` (e.g. `/media`).
    pub fn new(base_path: PathBuf, public_url_base: String) -> Self {
        Self { base_path, public_url_base }
    }
}

#[async_trait]
impl MediaStorage for LocalFsMediaStorage {
    #[instrument(skip(self, data), fields(key = %key))]
    async fn store(
        &self,
        key: &MediaKey,
        data: Bytes,
        _content_type: &str,
    ) -> Result<(), DomainError> {
        let file_path = self.base_path.join(&key.0);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DomainError::internal(format!("failed to create directory: {e}")))?;
        }
        fs::write(&file_path, &data)
            .await
            .map_err(|e| DomainError::internal(format!("failed to write file: {e}")))?;
        debug!(key = %key, path = %file_path.display(), "media stored locally");
        Ok(())
    }

    async fn get_url(&self, key: &MediaKey, _ttl: Duration) -> Result<String, DomainError> {
        // Local filesystem returns a static path. TTL is not applicable.
        let url = format!(
            "{}/{}",
            self.public_url_base.trim_end_matches('/'),
            key.0
        );
        Ok(url)
    }

    #[instrument(skip(self), fields(key = %key))]
    async fn delete(&self, key: &MediaKey) -> Result<(), DomainError> {
        let file_path = self.base_path.join(&key.0);
        match fs::remove_file(&file_path).await {
            Ok(()) => {
                debug!(key = %key, "media deleted from local fs");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Idempotent delete — not found is fine
                Ok(())
            }
            Err(e) => Err(DomainError::internal(format!("failed to delete file: {e}"))),
        }
    }
}
