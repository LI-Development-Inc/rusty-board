-- Migration 005: Threads table
--
-- op_post_id is set after the OP post is inserted (self-reference, FK added in 006).
CREATE TABLE IF NOT EXISTS threads (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    board_id     UUID        NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    op_post_id   UUID,
    reply_count  INTEGER     NOT NULL DEFAULT 0,
    bumped_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    sticky       BOOLEAN     NOT NULL DEFAULT false,
    closed       BOOLEAN     NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_threads_board_bumped ON threads(board_id, bumped_at DESC);
CREATE INDEX IF NOT EXISTS idx_threads_board_sticky ON threads(board_id, sticky, bumped_at DESC);
