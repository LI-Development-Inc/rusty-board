-- Migration 009: Flags (reports) table
CREATE TABLE IF NOT EXISTS flags (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id          UUID        NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    reason           TEXT        NOT NULL,
    reporter_ip_hash TEXT        NOT NULL,
    status           TEXT        NOT NULL DEFAULT 'pending'
                                 CHECK (status IN ('pending', 'approved', 'rejected')),
    resolved_by      UUID        REFERENCES users(id),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_flags_status ON flags(status);
CREATE INDEX IF NOT EXISTS idx_flags_post   ON flags(post_id);
