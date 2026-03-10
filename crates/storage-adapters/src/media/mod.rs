//! Media processing adapters.
//!
//! The composition root selects a concrete `MediaProcessor` implementation based
//! on active features:
//! - `ImageMediaProcessor` — always available (JPEG, PNG, GIF, WebP)
//! - `VideoMediaProcessor` — adds video support (`video` feature)
//! - `FullMediaProcessor` — adds video + PDF support (`video` + `documents` features)
//!
//! All processors unconditionally strip EXIF metadata from images.

pub mod images;

#[cfg(feature = "video")]
pub mod videos;

#[cfg(feature = "documents")]
pub mod documents;

#[cfg(feature = "media-s3")]
pub mod s3;

#[cfg(feature = "media-local")]
pub mod local_fs;

pub use images::ImageMediaProcessor;
