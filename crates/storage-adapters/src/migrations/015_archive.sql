-- Migration 015: Thread archive store
--
-- When board_config.archive_enabled = true, threads that would be pruned
-- (oldest non-sticky, when board exceeds max_threads) are moved here instead
-- of deleted. Archived threads are read-only: no new posts can be added.
-- The archive retains all thread metadata and is accessible via GET /board/:slug/archive.

CREATE TABLE IF NOT EXISTS archived_threads (
    id            UUID        PRIMARY KEY,
    board_id      UUID        NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    op_post_id    UUID,
    reply_count   INT         NOT NULL DEFAULT 0,
    bumped_at     TIMESTAMPTZ NOT NULL,
    sticky        BOOLEAN     NOT NULL DEFAULT FALSE,
    closed        BOOLEAN     NOT NULL DEFAULT TRUE,
    cycle         BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL,
    archived_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS archived_threads_board_idx  ON archived_threads(board_id, archived_at DESC);
