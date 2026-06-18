DROP INDEX idx_sessions_workspace_handle;

CREATE UNIQUE INDEX idx_sessions_workspace_handle
ON sessions(workspace_id, handle)
WHERE handle IS NOT NULL
  AND state NOT IN ('exited', 'error');
