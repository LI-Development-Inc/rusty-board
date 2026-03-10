-- Migration 002: Boards table
--
-- post_counter is included from the start — tracks the next sequential post number
-- for this board so posts can be numbered without a full table scan.
CREATE TABLE IF NOT EXISTS boards (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug         TEXT        NOT NULL UNIQUE
                             CHECK (slug ~ '^[a-z0-9_-]{1,16}$'),
    title        TEXT        NOT NULL CHECK (length(title) BETWEEN 1 AND 64),
    rules        TEXT        NOT NULL DEFAULT '',
    post_counter BIGINT      NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
