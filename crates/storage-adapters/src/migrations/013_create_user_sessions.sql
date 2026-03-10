-- Migration 013: User sessions for cookie-based authentication
--
-- Stores server-side session records used by CookieAuthProvider (auth-cookie feature).
-- The opaque session_id is stored in a Set-Cookie header; the client presents it on
-- every request and the server looks it up here to retrieve claims.
--
-- Advantages over pure JWT:
--   - Immediate revocation: DELETE the row and the session is invalid instantly.
--   - No token content exposed to the client beyond an opaque ID.
--   - Force-logout all sessions for a user on deactivation or password change.
--
-- claims_json stores the serialised Claims struct — avoids a user table join on
-- every request while allowing claim refresh when roles change.
-- Expired sessions are cleaned up by a periodic maintenance task, not a DB trigger.
CREATE TABLE IF NOT EXISTS user_sessions (
    session_id  TEXT        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    claims_json TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_user    ON user_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON user_sessions(expires_at);
