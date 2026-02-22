//! # rb-storage-local
//! rusty-board/crates/rb-storage-local/src/lib.rs
//! Local filesystem implementation of `MediaStore`.
//! Features: Content-addressable storage, directory sharding, and thumbnailing.

use async_trait::async_trait;
use rb_core::traits::MediaStore;
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};
use tokio::fs;
use image::io::Reader as ImageReader;
use std::io::Cursor;

pub struct LocalMediaStore {
    /// Root directory for all uploads (e.g., "./data/uploads")
    root_path: PathBuf,
    /// Public URL prefix (e.g., "/static/uploads")
    url_prefix: String,
}

impl LocalMediaStore {
    pub fn new(root: PathBuf, url_prefix: String) -> Self {
        Self { root_path: root, url_prefix }
    }

    /// Generates a sharded path: "ab/cd/ef...hash"
    fn get_sharded_path(&self, hash: &str) -> PathBuf {
        let mut path = self.root_path.clone();
        path.push(&hash[0..2]);
        path.push(&hash[2..4]);
        path.push(hash);
        path
    }
}

#[async_trait]
impl MediaStore for LocalMediaStore {
    /// Saves an upload using its SHA-256 hash as the filename.
    /// This automatically deduplicates files.
    async fn save_upload(&self, data: Vec<u8>, _content_type: &str) -> anyhow::Result<String> {
        // 1. Calculate Hash
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = format!("{:x}", hasher.finalize());

        let target_path = self.get_sharded_path(&hash);
        let parent = target_path.parent().unwrap();
        
        // 2. Ensure directory exists
        fs::create_dir_all(parent).await?;

        // 3. Save Original (if not exists)
        if !target_path.exists() {
            fs::write(&target_path, &data).await?;
            
            // 4. Generate Thumbnail (Background processing in a production scale, inline for MVP)
            self.generate_thumbnail(&target_path, &hash).await?;
        }

        Ok(hash)
    }

    async fn get_url(&self, media_id: &str) -> String {
        let rel_path = format!("{}/{}/{}", &media_id[0..2], &media_id[2..4], media_id);
        format!("{}/{}", self.url_prefix, rel_path)
    }

    async fn get_thumbnail_url(&self, media_id: &str) -> String {
        let rel_path = format!("{}/{}/thumb_{}.webp", &media_id[0..2], &media_id[2..4], media_id);
        format!("{}/{}", self.url_prefix, rel_path)
    }
}

impl LocalMediaStore {
    /// Internal helper to generate a 250px WebP thumbnail.
    async fn generate_thumbnail(&self, source_path: &Path, hash: &str) -> anyhow::Result<()> {
        let data = fs::read(source_path).await?;
        let img = ImageReader::new(Cursor::new(data))
            .with_guessed_format()?
            .decode()?;

        let thumb = img.thumbnail(250, 250);
        let mut thumb_path = source_path.parent().unwrap().to_path_buf();
        thumb_path.push(format!("thumb_{}.webp", hash));

        // Note: Using image-rs for MVP; libvips would replace this in Phase 3.
        thumb.save_with_format(thumb_path, image::ImageFormat::WebP)?;
        Ok(())
    }
}