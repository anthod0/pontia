CREATE INDEX idx_sessions_active_updated
ON sessions(updated_at DESC, session_id DESC)
WHERE archived_at IS NULL
  AND state NOT IN ('exited', 'error');

CREATE INDEX idx_sessions_unarchived_updated
ON sessions(updated_at DESC, session_id DESC)
WHERE archived_at IS NULL;

CREATE INDEX idx_sessions_workspace_unarchived_updated
ON sessions(workspace_id, updated_at DESC, session_id DESC)
WHERE archived_at IS NULL;
