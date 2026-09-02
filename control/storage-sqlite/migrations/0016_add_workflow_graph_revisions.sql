ALTER TABLE workflows
ADD COLUMN current_revision INTEGER NOT NULL DEFAULT 1
    CHECK (current_revision >= 1);

ALTER TABLE workflow_nodes
ADD COLUMN introduced_revision INTEGER NOT NULL DEFAULT 1
    CHECK (introduced_revision >= 1);

ALTER TABLE workflow_nodes
ADD COLUMN retired_revision INTEGER
    CHECK (retired_revision IS NULL OR retired_revision > introduced_revision);

CREATE INDEX idx_workflow_nodes_workflow_revision
    ON workflow_nodes(workflow_id, introduced_revision, retired_revision);

CREATE TRIGGER workflows_revision_monotonic
BEFORE UPDATE OF current_revision ON workflows
WHEN NEW.current_revision != OLD.current_revision + 1
BEGIN
    SELECT RAISE(ABORT, 'workflow revision must advance by one');
END;

CREATE TRIGGER workflow_nodes_validate_introduction
BEFORE INSERT ON workflow_nodes
WHEN NOT EXISTS (
    SELECT 1
    FROM workflows
    WHERE workflows.workflow_id = NEW.workflow_id
      AND NEW.introduced_revision >= workflows.current_revision
      AND NEW.introduced_revision <= workflows.current_revision + 1
)
BEGIN
    SELECT RAISE(ABORT, 'workflow node introduction must belong to the current or next revision');
END;

CREATE TRIGGER workflow_nodes_validate_parent_insert
BEFORE INSERT ON workflow_nodes
WHEN NEW.parent_node_id IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM workflow_nodes AS parent
     WHERE parent.node_id = NEW.parent_node_id
       AND parent.workflow_id = NEW.workflow_id
       AND parent.introduced_revision <= NEW.introduced_revision
       AND (parent.retired_revision IS NULL
            OR parent.retired_revision > NEW.introduced_revision)
 )
BEGIN
    SELECT RAISE(ABORT, 'workflow node parent must be active in the same workflow revision');
END;

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
  OR OLD.execution_profile_version IS NOT NEW.execution_profile_version
  OR OLD.introduced_revision IS NOT NEW.introduced_revision
BEGIN
    SELECT RAISE(ABORT, 'accepted workflow node definitions are immutable');
END;

CREATE TRIGGER workflow_nodes_immutable_retirement
BEFORE UPDATE OF retired_revision ON workflow_nodes
WHEN OLD.retired_revision IS NOT NULL
  OR NEW.retired_revision IS NULL
BEGIN
    SELECT RAISE(ABORT, 'workflow node retirement is immutable');
END;

CREATE TRIGGER workflow_nodes_validate_retirement
BEFORE UPDATE OF retired_revision ON workflow_nodes
WHEN NEW.retired_revision IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM workflows
     WHERE workflows.workflow_id = OLD.workflow_id
       AND NEW.retired_revision = workflows.current_revision + 1
 )
BEGIN
    SELECT RAISE(ABORT, 'workflow node retirement must belong to the next revision');
END;

CREATE TRIGGER workflow_nodes_history_prevents_delete
BEFORE DELETE ON workflow_nodes
BEGIN
    SELECT RAISE(ABORT, 'workflow node history cannot be deleted');
END;
