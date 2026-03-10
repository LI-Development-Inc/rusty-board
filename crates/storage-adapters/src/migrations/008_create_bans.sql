-- Migration 008: Bans table
--
-- expires_at NULL = permanent ban.
-- Expired ban filtering (expires_at > now()) is handled at query time;
-- now() is STABLE not IMMUTABLE so it cannot be used in index predicates.
CREATE TABLE IF NOT EXISTS bans (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ip_hash     TEXT        NOT NULL,
    user_id     UUID        REFERENCES users(id),   -- NULL if banned anonymously
    reason      TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ,
    banned_by   UUID        NOT NULL REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_bans_ip_hash      ON bans(ip_hash);
CREATE INDEX IF NOT EXISTS idx_bans_ip_permanent ON bans(ip_hash) WHERE expires_at IS NULL;
