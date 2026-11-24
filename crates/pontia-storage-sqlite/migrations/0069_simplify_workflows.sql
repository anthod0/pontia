UPDATE workflows SET activating_node_id=NULL, active_patch_id=NULL;
DELETE FROM workflow_recoveries;
DELETE FROM workflow_patches;
DELETE FROM workflow_events;
DELETE FROM workflow_nodes;
DELETE FROM workflows;
DELETE FROM workflow_agent_events;
DELETE FROM workflow_session_snapshots;
DELETE FROM workflow_turn_snapshots;
DELETE FROM workflow_runtime_snapshots;
DELETE FROM workflow_inbox_snapshots;

DROP TRIGGER workflow_nodes_immutable_definition;
DROP INDEX idx_workflow_nodes_workflow_revision;
ALTER TABLE workflows DROP COLUMN active_patch_id;
ALTER TABLE workflows DROP COLUMN active_replanner_session_id;
ALTER TABLE workflows DROP COLUMN current_revision;
ALTER TABLE workflows ADD COLUMN initiator_session_id TEXT NOT NULL;
ALTER TABLE workflows ADD COLUMN last_report_sequence INTEGER NOT NULL DEFAULT 0;
DROP TABLE workflow_patches;
ALTER TABLE workflow_nodes DROP COLUMN retired_revision;
ALTER TABLE workflow_nodes DROP COLUMN introduced_revision;

CREATE TRIGGER workflow_nodes_immutable_definition
BEFORE UPDATE ON workflow_nodes
WHEN OLD.workflow_id IS NOT NEW.workflow_id
  OR OLD.parent_node_id IS NOT NEW.parent_node_id
  OR OLD.node_type IS NOT NEW.node_type
  OR OLD.phase IS NOT NEW.phase
  OR OLD.title IS NOT NEW.title
  OR OLD.instructions IS NOT NEW.instructions
  OR OLD.inputs IS NOT NEW.inputs
  OR OLD.output IS NOT NEW.output
  OR OLD.execution_profile_id IS NOT NEW.execution_profile_id
BEGIN
    SELECT RAISE(ABORT, 'accepted workflow node definitions are immutable');
END;
