//! S3-compatible object storage adapter (`media-s3` feature).
//!
//! Wraps `aws-sdk-s3` to implement `MediaStorage`. Works with AWS S3, MinIO,
//! Cloudflare R2, and any other S3-compatible endpoint.

use async_trait::async_trait;
use bytes::Bytes;
use domains::errors::DomainError;
use domains::models::MediaKey;
use domains::ports::MediaStorage;
use std::time::Duration;
use tracing::{debug, instrument};

/// S3-compatible media storage backed by `aws-sdk-s3`.
///
/// Wraps an `aws_sdk_s3::Client` pre-configured with bucket name and credentials.
/// The bucket must exist before the application starts.
pub struct S3MediaStorage {
    client:   aws_sdk_s3::Client,
    bucket:   String,
    base_url: Option<String>, // optional custom endpoint base URL for presigned URLs
}

impl S3MediaStorage {
    /// Create a new `S3MediaStorage`.
    ///
    /// `client` should be pre-configured with the correct credentials and endpoint.
    /// `bucket` is the S3 bucket name.
    /// `base_url` is an optional override for generating presigned URLs (e.g. MinIO public URL).
    pub fn new(client: aws_sdk_s3::Client, bucket: String, base_url: Option<String>) -> Self {
        Self { client, bucket, base_url }
    }
}

#[async_trait]
impl MediaStorage for S3MediaStorage {
    #[instrument(skip(self, data), fields(key = %key, content_type = content_type))]
    async fn store(
        &self,
        key: &MediaKey,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), DomainError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key.0)
            .content_type(content_type)
            .body(data.into())
            .send()
            .await
            .map_err(|e| DomainError::internal(format!("S3 put_object failed: {e}")))?;
        debug!(key = %key, "media stored to S3");
        Ok(())
    }

    #[instrument(skip(self), fields(key = %key))]
    async fn get_url(&self, key: &MediaKey, ttl: Duration) -> Result<String, DomainError> {
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key.0)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(ttl)
                    .map_err(|e| DomainError::internal(format!("presigning config error: {e}")))?,
            )
            .await
            .map_err(|e| DomainError::internal(format!("S3 presigning failed: {e}")))?;
        Ok(presigned.uri().to_string())
    }

    #[instrument(skip(self), fields(key = %key))]
    async fn delete(&self, key: &MediaKey) -> Result<(), DomainError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key.0)
            .send()
            .await
            .map_err(|e| DomainError::internal(format!("S3 delete_object failed: {e}")))?;
        debug!(key = %key, "media deleted from S3");
        Ok(())
    }
}
