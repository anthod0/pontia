-- Normal application writes preserve event and Turn identity without SQL triggers.
DROP TRIGGER events_preserve_turn_topology;
DROP TRIGGER turns_preserve_turn_identity;

-- Workflow planning and transactional Patch commits own graph history rules.
DROP TRIGGER workflows_revision_monotonic;
DROP TRIGGER workflow_nodes_validate_introduction;
DROP TRIGGER workflow_nodes_validate_parent_insert;
DROP TRIGGER workflow_nodes_immutable_definition;
DROP TRIGGER workflow_nodes_immutable_retirement;
DROP TRIGGER workflow_nodes_validate_retirement;
DROP TRIGGER workflow_nodes_history_prevents_delete;
