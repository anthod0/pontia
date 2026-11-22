ALTER TABLE interactions ADD COLUMN report_key TEXT;
ALTER TABLE interactions ADD COLUMN authorization_digest TEXT;
CREATE UNIQUE INDEX idx_interactions_report_key ON interactions(session_id, report_key) WHERE report_key IS NOT NULL;
