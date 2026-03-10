-- Migration 003: Board configuration table
--
-- One row per board. Created automatically when a board is created.
-- All configuration columns — including spam heuristics — are present from the start.
-- This is the runtime behavior surface controlled by operator dashboards.
CREATE TABLE IF NOT EXISTS board_configs (
    board_id                    UUID     PRIMARY KEY REFERENCES boards(id) ON DELETE CASCADE,

    -- Content rules
    bump_limit                  INTEGER  NOT NULL DEFAULT 500  CHECK (bump_limit > 0),
    max_files                   SMALLINT NOT NULL DEFAULT 4   CHECK (max_files BETWEEN 1 AND 10),
    max_file_size_kb            INTEGER  NOT NULL DEFAULT 10240 CHECK (max_file_size_kb > 0),
    allowed_mimes               TEXT[]   NOT NULL DEFAULT ARRAY['image/jpeg','image/png','image/gif','image/webp'],
    max_post_length             INTEGER  NOT NULL DEFAULT 4000 CHECK (max_post_length BETWEEN 1 AND 32000),

    -- Rate limiting
    rate_limit_enabled          BOOLEAN  NOT NULL DEFAULT true,
    rate_limit_window_secs      INTEGER  NOT NULL DEFAULT 60  CHECK (rate_limit_window_secs > 0),
    rate_limit_posts            SMALLINT NOT NULL DEFAULT 3   CHECK (rate_limit_posts > 0),

    -- Spam filtering
    spam_filter_enabled         BOOLEAN  NOT NULL DEFAULT true,
    spam_score_threshold        REAL     NOT NULL DEFAULT 0.75 CHECK (spam_score_threshold BETWEEN 0 AND 1),
    duplicate_check             BOOLEAN  NOT NULL DEFAULT true,

    -- Spam heuristics
    link_blacklist              TEXT[]   NOT NULL DEFAULT '{}',
    name_rate_limit_window_secs INTEGER  NOT NULL DEFAULT 0
                                CHECK (name_rate_limit_window_secs >= 0),

    -- Posting behavior
    forced_anon                 BOOLEAN  NOT NULL DEFAULT false,
    allow_sage                  BOOLEAN  NOT NULL DEFAULT true,
    allow_tripcodes             BOOLEAN  NOT NULL DEFAULT false,
    captcha_required            BOOLEAN  NOT NULL DEFAULT false,
    nsfw                        BOOLEAN  NOT NULL DEFAULT false,

    -- Future capabilities (fields present now; adapters ship in later versions)
    search_enabled              BOOLEAN  NOT NULL DEFAULT false,
    archive_enabled             BOOLEAN  NOT NULL DEFAULT false,
    federation_enabled          BOOLEAN  NOT NULL DEFAULT false
);
