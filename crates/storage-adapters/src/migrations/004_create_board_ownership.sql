-- Migration 004: Board ownership and volunteer assignment
--
-- board_owners: users with full config management rights over a board.
--   Assigned by admins. A board can have multiple owners.
--
-- board_volunteers: users assigned by a board owner to help moderate their board.
--   Can delete posts and issue bans on that board only.
--   Distinct from global janitors who cover all boards.
CREATE TABLE IF NOT EXISTS board_owners (
    board_id    UUID        NOT NULL REFERENCES boards(id)  ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (board_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_board_owners_user  ON board_owners(user_id);
CREATE INDEX IF NOT EXISTS idx_board_owners_board ON board_owners(board_id);

CREATE TABLE IF NOT EXISTS board_volunteers (
    board_id     UUID        NOT NULL REFERENCES boards(id)  ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    assigned_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    assigned_by  UUID        REFERENCES users(id) ON DELETE SET NULL,
    PRIMARY KEY (board_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_board_volunteers_user  ON board_volunteers(user_id);
CREATE INDEX IF NOT EXISTS idx_board_volunteers_board ON board_volunteers(board_id);
