-- Migration 001: Users table
--
-- All five roles defined from the start — no incremental role additions needed.
-- Roles:
--   admin          → full site control
--   janitor        → global site-wide moderation (can act on any board)
--   board_owner    → manages config and volunteers for their assigned boards
--   board_volunteer→ scoped moderation on assigned boards only
--   user           → registered tier; no moderation powers; can submit staff requests
--                    posting remains 100% anonymous — this account is for staff pipeline only
CREATE TABLE IF NOT EXISTS users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    username      TEXT        NOT NULL UNIQUE
                              CHECK (length(username) BETWEEN 3 AND 32),
    password_hash TEXT        NOT NULL,
    role          TEXT        NOT NULL
                              CHECK (role IN ('admin', 'janitor', 'board_owner', 'board_volunteer', 'user')),
    is_active     BOOLEAN     NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
