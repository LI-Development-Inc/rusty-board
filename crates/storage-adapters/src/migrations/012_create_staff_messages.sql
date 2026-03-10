-- Migration 012: Staff messages
--
-- Internal text-only messages between staff accounts.
-- Sender constraints (enforced at service layer, not DB):
--   Admin      → any staff
--   BoardOwner → their volunteers + janitors
-- Messages expire after 14 days (periodic cleanup via purge_expired, not a DB trigger).
-- Body max 4000 chars. No attachments.
CREATE TABLE IF NOT EXISTS staff_messages (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    from_user_id UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    to_user_id   UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body         TEXT        NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    read_at      TIMESTAMPTZ,         -- NULL = unread
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_staff_msg_to_unread ON staff_messages(to_user_id, created_at DESC)
    WHERE read_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_staff_msg_to        ON staff_messages(to_user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_staff_msg_from      ON staff_messages(from_user_id, created_at DESC);
