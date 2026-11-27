ALTER TABLE workflow_session_snapshots ADD COLUMN has_unsettled_input INTEGER NOT NULL DEFAULT 0;
DROP TABLE workflow_inbox_snapshots;
