ALTER TABLE interactions ADD COLUMN settlement_report_key TEXT;
ALTER TABLE interactions ADD COLUMN settlement_outcome TEXT;
CREATE UNIQUE INDEX idx_interactions_settlement_key ON interactions(session_id, settlement_report_key) WHERE settlement_report_key IS NOT NULL;
