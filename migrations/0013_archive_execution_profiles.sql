ALTER TABLE execution_profiles ADD COLUMN active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1));
ALTER TABLE execution_profiles ADD COLUMN archived_at TEXT;
ALTER TABLE execution_profiles ADD COLUMN archived_reason TEXT;

CREATE INDEX idx_execution_profiles_active_latest ON execution_profiles(profile_id, active, archived_at, created_at, version);
