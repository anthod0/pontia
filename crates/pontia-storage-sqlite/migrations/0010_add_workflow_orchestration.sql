CREATE TABLE workflows (
    workflow_id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    cwd TEXT NOT NULL,
    state TEXT NOT NULL,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    started_at TEXT,
    completed_at TEXT
);

CREATE TABLE workflow_nodes (
    node_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    parent_node_id TEXT,
    title TEXT NOT NULL,
    instructions TEXT NOT NULL,
    inputs TEXT NOT NULL DEFAULT '[]',
    output TEXT NOT NULL,
    execution_profile_id TEXT,
    execution_profile_version TEXT,
    session_id TEXT,
    submitted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(workflow_id) REFERENCES workflows(workflow_id),
    FOREIGN KEY(parent_node_id) REFERENCES workflow_nodes(node_id)
);

CREATE INDEX idx_workflow_nodes_workflow_parent
    ON workflow_nodes(workflow_id, parent_node_id, created_at, node_id);

CREATE INDEX idx_workflow_nodes_session
    ON workflow_nodes(session_id)
    WHERE session_id IS NOT NULL;

CREATE TABLE workflow_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(workflow_id) REFERENCES workflows(workflow_id),
    UNIQUE(workflow_id, sequence)
);

CREATE INDEX idx_workflow_events_workflow_sequence
    ON workflow_events(workflow_id, sequence);
