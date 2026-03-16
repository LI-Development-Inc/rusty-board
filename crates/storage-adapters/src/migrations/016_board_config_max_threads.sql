-- Migration 016: Add max_threads to board_configs
--
-- Controls how many live threads a board can hold before the oldest non-sticky
-- thread is pruned (or archived when archive_enabled = true).
-- Default 200 matches the BoardConfig Rust default.

ALTER TABLE board_configs ADD COLUMN max_threads INT NOT NULL DEFAULT 200;
