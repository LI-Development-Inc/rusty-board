-- schema.sql

-- Boards: The different sections (e.g., /b/, /prog/)
CREATE TABLE IF NOT EXISTS boards (
    id BLOB PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT,
    settings TEXT NOT NULL, -- JSON string
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Threads: The parent containers for conversations
CREATE TABLE IF NOT EXISTS threads (
    id BLOB PRIMARY KEY,
    board_id BLOB NOT NULL,
    last_bump DATETIME NOT NULL,
    is_sticky BOOLEAN DEFAULT 0,
    is_locked BOOLEAN DEFAULT 0,
    metadata TEXT, -- JSON string
    FOREIGN KEY(board_id) REFERENCES boards(id)
);

-- Posts: The actual content
CREATE TABLE IF NOT EXISTS posts (
    id BLOB PRIMARY KEY,
    thread_id BLOB NOT NULL,
    user_id_in_thread TEXT, -- The "ID" or Tripcode
    content TEXT NOT NULL,
    media_id BLOB,
    is_op BOOLEAN DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT, -- JSON string
    FOREIGN KEY(thread_id) REFERENCES threads(id)
);