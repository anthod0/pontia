ALTER TABLE workflow_nodes
ADD COLUMN node_type TEXT NOT NULL DEFAULT 'agent';
