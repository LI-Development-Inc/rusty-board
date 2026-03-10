-- Migration 010: Audit log table
--
-- Records all moderation actions with actor, target, and structured details.
-- actor_id NULL = action taken by the system or an anonymous actor.
-- Actions: delete_post, delete_thread, sticky_thread, close_thread,
--          ban_ip, expire_ban, resolve_flag, update_board_config,
--          create_board, delete_board, create_user, deactivate_user
CREATE TABLE IF NOT EXISTS audit_logs (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id      UUID        REFERENCES users(id),
    actor_ip_hash TEXT,
    action        TEXT        NOT NULL,
    target_id     UUID,
    target_type   TEXT,       -- 'post','thread','user','board','ban','flag'
    details       JSONB,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_audit_actor  ON audit_logs(actor_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_target ON audit_logs(target_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_time   ON audit_logs(created_at DESC);
