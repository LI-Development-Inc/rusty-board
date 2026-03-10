//! `storage-adapters` — concrete implementations of domain storage ports.
//!
//! All modules here are feature-gated except `media` (image processing is always
//! compiled) and `cache` (in-process BoardConfig cache). The composition root
//! selects which concrete adapters to instantiate based on active Cargo features.
//!
//! # Feature flags
//! - `db-postgres` — PostgreSQL repositories via sqlx
//! - `media-s3` — S3-compatible object storage
//! - `media-local` — local filesystem media storage
//! - `video` — video keyframe extraction via ffmpeg-next
//! - `documents` — PDF first-page rendering via pdfium-render
//! - `redis` — Redis rate limiter via deadpool-redis

pub mod cache;
pub mod media;

#[cfg(feature = "db-postgres")]
pub mod postgres;

#[cfg(feature = "redis")]
pub mod redis;

pub mod in_memory;
pub mod stubs;
