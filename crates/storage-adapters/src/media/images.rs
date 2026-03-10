//! Image media processor: validate, EXIF-strip, thumbnail-generate.
//!
//! This module is **always compiled** — image support is not feature-gated.
//! All uploaded images go through this processor regardless of other active features.
//!
//! # INVARIANT: EXIF stripping is unconditional
//! EXIF metadata is stripped from every image regardless of board configuration,
//! Settings, or any operator toggle. It is a hard-coded business rule, not a
//! configurable parameter. This protects poster privacy.

use async_trait::async_trait;
use bytes::Bytes;
use domains::errors::DomainError;
use domains::models::{ContentHash, MediaKey};
use domains::ports::{MediaProcessor, ProcessedMedia, RawMedia};
use image::ImageFormat;
use mime::Mime;
use sha2::{Digest, Sha256};
use std::io::Cursor;

/// Thumbnail width in pixels. Matches `Settings.thumbnail_width_px` default.
const THUMBNAIL_WIDTH_PX: u32 = 320;

/// Supported image MIME types.
const SUPPORTED_MIMES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
];

/// Media processor that handles images using the `image` crate.
///
/// Processes JPEG, PNG, GIF, and WebP:
/// 1. Validates MIME type
/// 2. Strips EXIF metadata by re-encoding through the `image` crate
/// 3. Generates a 320px-wide thumbnail as PNG
/// 4. Computes SHA-256 content hash of the re-encoded original
pub struct ImageMediaProcessor {
    thumbnail_width: u32,
}

impl ImageMediaProcessor {
    /// Create a new processor with the default thumbnail width (320px).
    pub fn new() -> Self {
        Self {
            thumbnail_width: THUMBNAIL_WIDTH_PX,
        }
    }

    /// Create a processor with a custom thumbnail width (for testing).
    pub fn with_thumbnail_width(thumbnail_width: u32) -> Self {
        Self { thumbnail_width }
    }
}

impl Default for ImageMediaProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MediaProcessor for ImageMediaProcessor {
    async fn process(&self, input: RawMedia) -> Result<ProcessedMedia, DomainError> {
        let mime_str = input.mime.to_string();

        // Step 1: Validate MIME type
        if !self.accepts(&input.mime) {
            return Err(DomainError::Validation(
                domains::errors::ValidationError::DisallowedMime { mime: mime_str.clone() },
            ));
        }

        // Step 2: Decode image (this strips EXIF by going through image decode/encode)
        // INVARIANT: We re-encode through the `image` crate which discards all
        // metadata including EXIF. This is the only place this happens.
        let img = image::load_from_memory(&input.data).map_err(|e| {
            DomainError::media_processing(format!("failed to decode image: {e}"))
        })?;

        // Step 3: Re-encode original as PNG (EXIF stripped by the decode/encode cycle)
        let format = match mime_str.as_str() {
            "image/jpeg" => ImageFormat::Jpeg,
            "image/png"  => ImageFormat::Png,
            "image/gif"  => ImageFormat::Gif,
            "image/webp" => ImageFormat::WebP,
            _ => unreachable!("MIME already validated above"),
        };
        let mut original_buf = Cursor::new(Vec::new());
        img.write_to(&mut original_buf, format).map_err(|e| {
            DomainError::media_processing(format!("failed to re-encode image: {e}"))
        })?;
        let original_bytes = Bytes::from(original_buf.into_inner());

        // Step 4: Compute content hash of re-encoded original
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(&original_bytes);
            ContentHash::new(hex::encode(hasher.finalize()))
        };

        // Step 5: Generate thumbnail
        let thumb_img = img.thumbnail(self.thumbnail_width, u32::MAX);
        let mut thumb_buf = Cursor::new(Vec::new());
        thumb_img.write_to(&mut thumb_buf, ImageFormat::Png).map_err(|e| {
            DomainError::media_processing(format!("failed to generate thumbnail: {e}"))
        })?;

        // Step 6: Compress thumbnail with oxipng
        let raw_thumb_bytes = thumb_buf.into_inner();
        let thumb_bytes = oxipng::optimize_from_memory(
            &raw_thumb_bytes,
            &oxipng::Options::default(),
        )
        .unwrap_or(raw_thumb_bytes); // fall back to uncompressed if optimisation fails
        let thumb_bytes = Bytes::from(thumb_bytes);

        let size_kb = (original_bytes.len() as u32).div_ceil(1024);
        let ext = extension_for_mime(&mime_str);
        let key_base = format!("{}", uuid::Uuid::new_v4());
        let original_key = MediaKey::new(format!("{key_base}.{ext}"));
        let thumbnail_key = MediaKey::new(format!("{key_base}_thumb.png"));

        Ok(ProcessedMedia {
            original_key,
            original_data: original_bytes,
            thumbnail_key: Some(thumbnail_key),
            thumbnail_data: Some(thumb_bytes),
            hash,
            size_kb,
        })
    }

    fn accepts(&self, mime: &Mime) -> bool {
        SUPPORTED_MIMES.contains(&mime.as_ref())
    }
}

fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png"  => "png",
        "image/gif"  => "gif",
        "image/webp" => "webp",
        _            => "bin",
    }
}
