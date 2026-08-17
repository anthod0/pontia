ALTER TABLE workflow_nodes
    ADD COLUMN submitted_runtime_instance_id TEXT;

ALTER TABLE workflow_nodes
    ADD COLUMN exit_request_started_at TEXT;
