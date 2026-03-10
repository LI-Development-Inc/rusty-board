-- Migration 007: Attachments table
--
-- hash stores SHA-256 of original file bytes — used for deduplication in v1.2.
CREATE TABLE IF NOT EXISTS attachments (
    id            UUID     PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id       UUID     NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    filename      TEXT     NOT NULL,
    mime          TEXT     NOT NULL,
    hash          TEXT     NOT NULL,
    size_kb       INTEGER  NOT NULL,
    media_key     TEXT     NOT NULL,   -- storage key (S3 path or local path)
    thumbnail_key TEXT,               -- NULL if no thumbnail generated
    spoiler       BOOLEAN  NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS idx_attachments_post ON attachments(post_id);
CREATE INDEX IF NOT EXISTS idx_attachments_hash ON attachments(hash);
