CREATE TABLE sessions (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_message TEXT,
    conversation_history TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(conversation_history) AND json_type(conversation_history) = 'array'),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX idx_sessions_project_created_at
    ON sessions (project_id, created_at DESC);
