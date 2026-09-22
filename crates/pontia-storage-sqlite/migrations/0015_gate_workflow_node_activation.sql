ALTER TABLE workflows
ADD COLUMN activating_node_id TEXT REFERENCES workflow_nodes(node_id);
