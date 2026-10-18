-- Max mode task list.
--
-- In "Max" mode the agent first breaks a large requirement down into simple,
-- business-oriented tasks (a screen, CRUD, or report each), then runs one
-- sequential agent per task. Each row here is one of those tasks, persisted so
-- a long build can be resumed after the app restarts or the run is interrupted.
--
-- `status` is one of: 'pending', 'running', 'completed', 'failed', 'skipped'.
CREATE TABLE IF NOT EXISTS max_tasks (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    session_id INTEGER NOT NULL,
    task_index INTEGER NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    result_summary TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE (project_id, session_id, task_index)
);

CREATE INDEX idx_max_tasks_session ON max_tasks (project_id, session_id, task_index);
