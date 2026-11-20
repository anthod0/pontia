DROP INDEX idx_sessions_execution_profile;
ALTER TABLE sessions DROP COLUMN execution_profile_id;
ALTER TABLE sessions DROP COLUMN execution_profile_version;
