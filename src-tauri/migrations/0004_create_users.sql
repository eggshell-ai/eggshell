-- Per-project users mirror.
--
-- The authoritative user records live in each project's Symfony/MySQL `user`
-- table. This local table exists so the desktop UI can list the sign-in
-- accounts for a project (and show their generated passwords) without talking
-- to MySQL. It is kept in sync by the backend user-management tooling.
--
-- NOTE: `password` is stored in plain text on purpose. These are local,
-- development-only credentials for a throwaway app the user is building; the
-- value is surfaced in the chat sidebar so the human can sign in. Do not
-- reuse this pattern for anything handling real user data.
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    email TEXT NOT NULL CHECK (length(trim(email)) > 0),
    role TEXT NOT NULL DEFAULT 'ROLE_USER',
    password TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE (project_id, email)
);

CREATE INDEX idx_users_project_id ON users (project_id);
