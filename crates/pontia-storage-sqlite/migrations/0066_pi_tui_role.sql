-- Renaming the role in both projections preserves their existing revisions.
DROP TRIGGER session_runtimes_revision_update;
UPDATE session_runtimes SET role = 'pi_tui' WHERE role = 'tui';
CREATE TRIGGER session_runtimes_revision_update AFTER UPDATE ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

ALTER TABLE workflow_runtime_snapshots ADD COLUMN is_control INTEGER NOT NULL DEFAULT 0;

UPDATE workflow_runtime_snapshots SET role = 'pi_tui' WHERE role = 'tui';
UPDATE workflow_runtime_snapshots AS r SET is_control = 1
WHERE EXISTS (
    SELECT 1 FROM workflow_session_snapshots AS s
    WHERE s.session_id = r.session_id AND (s.client_type, r.role) IN (
        ('pi', 'pi_tui'), ('acp', 'acp'), ('claude-agent', 'claude_sdk_host')
    )
);

UPDATE events SET payload = json_set(payload, '$.role', 'pi_tui')
WHERE json_extract(payload, '$.role') = 'tui';
UPDATE workflow_agent_events SET payload = json_set(payload, '$.role', 'pi_tui')
WHERE json_extract(payload, '$.role') = 'tui';
