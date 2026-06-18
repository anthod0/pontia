ALTER TABLE sessions ADD COLUMN execution_profile_id TEXT;
ALTER TABLE sessions ADD COLUMN execution_profile_version TEXT;

CREATE INDEX idx_sessions_execution_profile
    ON sessions(workspace_id, execution_profile_id, execution_profile_version, state, updated_at, session_id);
