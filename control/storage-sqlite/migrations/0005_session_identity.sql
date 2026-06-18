ALTER TABLE sessions ADD COLUMN handle TEXT;

CREATE UNIQUE INDEX idx_sessions_workspace_handle
ON sessions(workspace_id, handle)
WHERE handle IS NOT NULL;
