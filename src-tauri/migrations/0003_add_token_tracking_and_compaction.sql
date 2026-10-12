-- Add token_usage and compacted_summary to sessions table
ALTER TABLE sessions ADD COLUMN token_usage INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN compacted_summary TEXT;
