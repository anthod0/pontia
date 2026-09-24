CREATE TABLE workflow_recoveries (
    recovery_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id),
    failure_event_id TEXT NOT NULL UNIQUE REFERENCES workflow_events(event_id),
    exit_event_id TEXT NOT NULL REFERENCES events(event_id),
    node_id TEXT NOT NULL REFERENCES workflow_nodes(node_id),
    session_id TEXT NOT NULL REFERENCES sessions(session_id),
    message_id TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('requested', 'preparing', 'dispatching', 'completed', 'failed')),
    runtime_instance_id TEXT,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE UNIQUE INDEX idx_workflow_recovery_active ON workflow_recoveries(workflow_id)
    WHERE state IN ('requested', 'preparing', 'dispatching');
