-- Migration 006: Posts table
--
-- post_number is included from the start — sequential per-board post number.
-- Each board maintains an atomic counter (boards.post_counter). Every new post
-- atomically claims the next number via a CTE-based UPDATE … RETURNING.
-- ip_hash stores SHA-256(ip + daily_salt) — never the raw IP address.
CREATE TABLE IF NOT EXISTS posts (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id   UUID        NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    body        TEXT        NOT NULL DEFAULT '',
    ip_hash     TEXT        NOT NULL,
    name        TEXT,                   -- NULL = anonymous
    tripcode    TEXT,                   -- NULL unless tripcodes enabled
    email       TEXT,                   -- 'sage' disables bump; otherwise unused
    post_number BIGINT      NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Add OP post foreign key to threads now that posts table exists.
ALTER TABLE threads
    ADD CONSTRAINT fk_threads_op_post
    FOREIGN KEY (op_post_id) REFERENCES posts(id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX IF NOT EXISTS idx_posts_thread       ON posts(thread_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_posts_ip_hash      ON posts(ip_hash);
CREATE INDEX IF NOT EXISTS idx_posts_board_number ON posts(thread_id, post_number ASC);
CREATE INDEX IF NOT EXISTS idx_posts_body_fts     ON posts USING gin(to_tsvector('english', body));
