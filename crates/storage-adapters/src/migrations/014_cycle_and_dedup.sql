-- Migration 014: Thread cycle mode + file deduplication index
--
-- cycle mode: when a thread is in cycle mode and hits the bump limit,
-- the oldest unpinned post is pruned instead of the thread being locked.
-- pinned posts are excluded from cycle pruning.

ALTER TABLE threads ADD COLUMN cycle BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE posts   ADD COLUMN pinned BOOLEAN NOT NULL DEFAULT FALSE;

-- Index for fast deduplication lookup by content hash
CREATE INDEX IF NOT EXISTS attachments_hash_idx ON attachments(hash);
