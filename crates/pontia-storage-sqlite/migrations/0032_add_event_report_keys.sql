ALTER TABLE events ADD COLUMN report_key TEXT;

CREATE UNIQUE INDEX idx_events_report_key
ON events(session_id, event_type, report_key)
WHERE report_key IS NOT NULL;
