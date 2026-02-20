-- Enable foreign key support
PRAGMA foreign_keys = ON;

-- 1. Boards Table
CREATE TABLE IF NOT EXISTS boards (
    id BLOB PRIMARY KEY NOT NULL, -- UUID v7 stored as bytes
    slug TEXT UNIQUE NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    settings TEXT NOT NULL DEFAULT '{}', -- JSON string
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 2. Threads Table
CREATE TABLE IF NOT EXISTS threads (
    id BLOB PRIMARY KEY NOT NULL,
    board_id BLOB NOT NULL,
    last_bump DATETIME NOT NULL,
    is_sticky INTEGER NOT NULL DEFAULT 0,
    is_locked INTEGER NOT NULL DEFAULT 0,
    metadata TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (board_id) REFERENCES boards(id) ON DELETE CASCADE
);
CREATE INDEX idx_threads_board_bump ON threads(board_id, last_bump DESC);

-- 3. Posts Table
CREATE TABLE IF NOT EXISTS posts (
    id BLOB PRIMARY KEY NOT NULL,
    thread_id BLOB NOT NULL,
    user_id_in_thread TEXT NOT NULL,
    content TEXT NOT NULL,
    media_id TEXT,
    is_op INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);
CREATE INDEX idx_posts_thread ON posts(thread_id);

-- 4. Bans Table
CREATE TABLE IF NOT EXISTS bans (
    id BLOB PRIMARY KEY NOT NULL,
    ip_address TEXT NOT NULL,
    reason TEXT NOT NULL,
    expires_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_bans_ip ON bans(ip_address);