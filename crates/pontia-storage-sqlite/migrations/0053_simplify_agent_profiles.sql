CREATE TABLE agent_profiles (
    profile_id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    client_type TEXT,
    client_key TEXT,
    model TEXT CHECK (
        (client_type IS NULL AND client_key IS NULL AND model IS NULL)
        OR (client_type IS NOT NULL AND client_key IS NOT NULL AND model IS NOT NULL)
    ),
    system_prompt TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO agent_profiles
    (profile_id, name, description, client_type, client_key, model, system_prompt, created_at, updated_at)
SELECT profile_id, name, description, default_client_type, default_client_key, default_model,
       system_prompt_template, created_at, updated_at
FROM execution_profiles
WHERE rowid IN (
    SELECT MAX(rowid) FROM execution_profiles
    WHERE active = 1 AND archived_at IS NULL GROUP BY profile_id
);
DROP TABLE execution_profiles;

DROP TRIGGER IF EXISTS workflow_nodes_immutable_definition;
ALTER TABLE workflow_nodes DROP COLUMN execution_profile_version;
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
  OR OLD.introduced_revision IS NOT NEW.introduced_revision
BEGIN
    SELECT RAISE(ABORT, 'accepted workflow node definitions are immutable');
END;
