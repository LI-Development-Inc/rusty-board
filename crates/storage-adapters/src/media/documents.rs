//! PDF document processor using pdfium-render for first-page thumbnail extraction.
//!
//! This module is only compiled when the `documents` feature is active.
//!
//! # Licensing note
//! pdfium-render requires a pre-built PDFium binary. PDFium is BSD-licensed.
//! Validate the distribution model before shipping. Treat this feature as
//! experimental for v1.0.
//!
//! See `TECHNICALSPECS.md §2` for notes on PDFium binary distribution.

// TODO(v1.0): Implement DocumentMediaProcessor using pdfium-render first-page rendering.
// Validate licensing and binary distribution before implementing.

use async_trait::async_trait;
use domains::errors::DomainError;
use domains::ports::{MediaProcessor, ProcessedMedia, RawMedia};
use mime::Mime;

const SUPPORTED_DOCUMENT_MIMES: &[&str] = &["application/pdf"];

/// Media processor for PDF documents.
///
/// Renders the first page as a PNG thumbnail using PDFium.
pub struct DocumentMediaProcessor;

impl DocumentMediaProcessor {
    /// Create a new document processor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DocumentMediaProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MediaProcessor for DocumentMediaProcessor {
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

        // TODO(v1.0): Render first PDF page as PNG thumbnail using pdfium-render
        Err(DomainError::media_processing("PDF processing not yet implemented"))
    }

    fn accepts(&self, mime: &Mime) -> bool {
        super::images::ImageMediaProcessor::new().accepts(mime)
            || SUPPORTED_DOCUMENT_MIMES.contains(&mime.as_ref())
    }
}
