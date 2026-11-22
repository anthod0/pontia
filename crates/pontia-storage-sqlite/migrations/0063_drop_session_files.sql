DROP TABLE session_files;
DELETE FROM domain_event_positions
WHERE event_id IN (SELECT event_id FROM events WHERE event_type = 'session.files_shared');
DELETE FROM events WHERE event_type = 'session.files_shared';
DELETE FROM workflow_agent_events WHERE event_type = 'session.files_shared';
