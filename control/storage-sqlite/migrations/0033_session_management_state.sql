ALTER TABLE sessions ADD COLUMN pinned_at TEXT;
ALTER TABLE sessions ADD COLUMN archived_at TEXT;

CREATE INDEX idx_sessions_management_list ON sessions(archived_at, pinned_at, updated_at, session_id);
