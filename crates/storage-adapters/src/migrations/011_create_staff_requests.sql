-- Migration 011: Staff request pipeline
--
-- Stores promotion requests submitted by 'user' accounts.
-- request_type:
--   board_create      → payload: {slug, title, rules, reason}
--   become_volunteer  → target_slug required; payload: {reason}
--   become_janitor    → payload: {reason}
--
-- Approval authority:
--   board_create / become_janitor → admin only
--   become_volunteer              → admin OR board owner of target_slug
CREATE TABLE IF NOT EXISTS staff_requests (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    from_user_id  UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_type  TEXT        NOT NULL
                              CHECK (request_type IN ('board_create', 'become_volunteer', 'become_janitor')),
    target_slug   TEXT,
    payload       JSONB       NOT NULL DEFAULT '{}',
    status        TEXT        NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'approved', 'denied')),
    reviewed_by   UUID        REFERENCES users(id) ON DELETE SET NULL,
    review_note   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_staff_req_user   ON staff_requests(from_user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_staff_req_status ON staff_requests(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_staff_req_slug   ON staff_requests(target_slug)
    WHERE target_slug IS NOT NULL;

CREATE OR REPLACE FUNCTION staff_requests_set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_staff_requests_updated_at
    BEFORE UPDATE ON staff_requests
    FOR EACH ROW EXECUTE FUNCTION staff_requests_set_updated_at();
