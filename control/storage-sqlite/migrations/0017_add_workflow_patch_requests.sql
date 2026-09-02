CREATE TABLE workflow_patches (
    patch_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id),
    requesting_node_id TEXT NOT NULL REFERENCES workflow_nodes(node_id),
    requesting_session_id TEXT NOT NULL REFERENCES sessions(session_id),
    requesting_turn_id TEXT NOT NULL REFERENCES turns(turn_id),
    requesting_runtime_instance_id TEXT NOT NULL,
    replanner_creation_token TEXT NOT NULL UNIQUE,
    replanner_session_id TEXT REFERENCES sessions(session_id),
    replanner_turn_id TEXT REFERENCES turns(turn_id),
    replanner_runtime_instance_id TEXT,
    base_revision INTEGER NOT NULL CHECK (base_revision >= 1),
    result_revision INTEGER CHECK (result_revision IS NULL OR result_revision >= base_revision),
    state TEXT NOT NULL CHECK (state IN ('requested', 'planning', 'applied', 'rejected', 'blocked')),
    request_document_ref TEXT NOT NULL,
    request_size_bytes INTEGER NOT NULL CHECK (request_size_bytes >= 0),
    decision_document_ref TEXT,
    reason_document_ref TEXT,
    blocked_draft_ref TEXT,
    interruption_attempted_at TEXT,
    interruption_requested_at TEXT,
    replanning_unlocked_at TEXT,
    continuation_message_id TEXT,
    continuation_queued_at TEXT,
    requested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    planning_at TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE UNIQUE INDEX idx_workflow_patches_one_active
    ON workflow_patches(workflow_id)
    WHERE state IN ('requested', 'planning');

CREATE UNIQUE INDEX idx_workflow_patches_replanner_session
    ON workflow_patches(replanner_session_id)
    WHERE replanner_session_id IS NOT NULL;

CREATE INDEX idx_workflow_patches_workflow_requested
    ON workflow_patches(workflow_id, requested_at, patch_id);

ALTER TABLE workflows
ADD COLUMN active_patch_id TEXT REFERENCES workflow_patches(patch_id);

ALTER TABLE workflows
ADD COLUMN active_replanner_session_id TEXT REFERENCES sessions(session_id);
