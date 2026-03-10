//! Video media processor using ffmpeg-next for keyframe extraction.
//!
//! This module is only compiled when the `video` feature is active.
//! It extends image processing with video thumbnail generation by extracting
//! the first keyframe of a video as a PNG thumbnail.
//!
//! # Build requirements
//! Requires libav* system libraries: `libavcodec-dev libavformat-dev libavutil-dev libswscale-dev`
//! See `TECHNICALSPECS.md §8` for Docker build instructions.

// TODO(v1.0): Implement VideoMediaProcessor using ffmpeg-next keyframe extraction.
// Validate the ffmpeg build in the target Docker environment before implementing.
// If ffmpeg-next does not build cleanly, defer the video feature to v1.1.

use async_trait::async_trait;
use domains::errors::DomainError;
use domains::ports::{MediaProcessor, ProcessedMedia, RawMedia};
use mime::Mime;

const SUPPORTED_VIDEO_MIMES: &[&str] = &[
    "video/mp4",
    "video/webm",
    "video/ogg",
];

/// Media processor that handles images and extracts video thumbnails via ffmpeg.
///
/// Extends `ImageMediaProcessor` for image types. For video types, extracts
/// the first keyframe as a PNG thumbnail.
pub struct VideoMediaProcessor;

impl VideoMediaProcessor {
    /// Create a new video processor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for VideoMediaProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MediaProcessor for VideoMediaProcessor {
    async fn process(&self, input: RawMedia) -> Result<ProcessedMedia, DomainError> {
        let mime_str = input.mime.to_string();

        // Delegate images to ImageMediaProcessor
        if super::images::ImageMediaProcessor::new().accepts(&input.mime) {
            return super::images::ImageMediaProcessor::new().process(input).await;
        }

        if !self.accepts(&input.mime) {
            return Err(DomainError::Validation(
                domains::errors::ValidationError::DisallowedMime { mime: mime_str },
            ));
        }

        // TODO(v1.0): Extract first keyframe using ffmpeg-next and return as thumbnail
        Err(DomainError::media_processing("video processing not yet implemented"))
    }

    fn accepts(&self, mime: &Mime) -> bool {
        super::images::ImageMediaProcessor::new().accepts(mime)
            || SUPPORTED_VIDEO_MIMES.contains(&mime.as_ref())
    }
}
