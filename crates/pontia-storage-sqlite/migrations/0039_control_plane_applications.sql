PRAGMA defer_foreign_keys = ON;
CREATE TABLE domain_event_positions (
    position INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE
);
INSERT INTO domain_event_positions(event_id) SELECT event_id FROM events ORDER BY rowid;
CREATE TRIGGER events_assign_position AFTER INSERT ON events BEGIN
    INSERT INTO domain_event_positions(event_id) VALUES (NEW.event_id);
END;
ALTER TABLE sessions ADD COLUMN application_id TEXT;
ALTER TABLE sessions ADD COLUMN application_ref TEXT;
ALTER TABLE sessions ADD COLUMN runtime_environment TEXT NOT NULL DEFAULT '{}';
CREATE UNIQUE INDEX idx_sessions_application ON sessions(application_id, application_ref);
DROP INDEX idx_sessions_workflow_replanner_creation_token;
UPDATE sessions SET application_id='workflow',
    application_ref='node:' || (SELECT node_id FROM workflow_nodes n WHERE n.session_id=sessions.session_id),
    runtime_environment=json_object('PONTIA_WORKFLOW_ID', (SELECT workflow_id FROM workflow_nodes n WHERE n.session_id=sessions.session_id))
WHERE EXISTS (SELECT 1 FROM workflow_nodes n WHERE n.session_id=sessions.session_id);
UPDATE sessions SET application_id='workflow',
    application_ref='patch:' || json_extract(metadata,'$.workflow_patch_id'),
    runtime_environment=json_object('PONTIA_WORKFLOW_ID',json_extract(metadata,'$.workflow_id'), 'PONTIA_WORKFLOW_PATCH_ID',json_extract(metadata,'$.workflow_patch_id'))
WHERE json_extract(metadata,'$.role')='workflow_replanner';
CREATE TABLE workflows_new (
    workflow_id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    cwd TEXT NOT NULL,
    state TEXT NOT NULL,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    started_at TEXT,
    completed_at TEXT
, activating_node_id TEXT REFERENCES workflow_nodes(node_id), current_revision INTEGER NOT NULL DEFAULT 1
    CHECK (current_revision >= 1), active_patch_id TEXT REFERENCES workflow_patches(patch_id), active_replanner_session_id TEXT);
INSERT INTO workflows_new SELECT * FROM workflows;
DROP TABLE workflows;
ALTER TABLE workflows_new RENAME TO workflows;

CREATE TABLE workflow_patches_new (
    patch_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id),
    requesting_node_id TEXT NOT NULL REFERENCES workflow_nodes(node_id),
    requesting_session_id TEXT NOT NULL,
    requesting_turn_id TEXT NOT NULL,
    requesting_runtime_id TEXT NOT NULL,
    replanner_session_id TEXT,
    replanner_turn_id TEXT,
    replanner_runtime_id TEXT,
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
, replanner_exit_requested_at TEXT);
INSERT INTO workflow_patches_new (patch_id, workflow_id, requesting_node_id, requesting_session_id, requesting_turn_id, requesting_runtime_id, replanner_session_id, replanner_turn_id, replanner_runtime_id, base_revision, result_revision, state, request_document_ref, request_size_bytes, decision_document_ref, reason_document_ref, blocked_draft_ref, interruption_attempted_at, interruption_requested_at, replanning_unlocked_at, continuation_message_id, continuation_queued_at, requested_at, planning_at, resolved_at, created_at, updated_at, replanner_exit_requested_at) SELECT patch_id, workflow_id, requesting_node_id, requesting_session_id, requesting_turn_id, requesting_runtime_id, replanner_session_id, replanner_turn_id, replanner_runtime_id, base_revision, result_revision, state, request_document_ref, request_size_bytes, decision_document_ref, reason_document_ref, blocked_draft_ref, interruption_attempted_at, interruption_requested_at, replanning_unlocked_at, continuation_message_id, continuation_queued_at, requested_at, planning_at, resolved_at, created_at, updated_at, replanner_exit_requested_at FROM workflow_patches;
DROP TABLE workflow_patches;
ALTER TABLE workflow_patches_new RENAME TO workflow_patches;
CREATE UNIQUE INDEX idx_workflow_patches_one_active
    ON workflow_patches(workflow_id)
    WHERE state IN ('requested', 'planning');
CREATE UNIQUE INDEX idx_workflow_patches_replanner_session
    ON workflow_patches(replanner_session_id)
    WHERE replanner_session_id IS NOT NULL;
CREATE INDEX idx_workflow_patches_workflow_requested
    ON workflow_patches(workflow_id, requested_at, patch_id);
CREATE TABLE workflow_recoveries_new (
    recovery_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id),
    failure_event_id TEXT NOT NULL UNIQUE REFERENCES workflow_events(event_id),
    exit_event_id TEXT NOT NULL,
    node_id TEXT NOT NULL REFERENCES workflow_nodes(node_id),
    session_id TEXT NOT NULL,
    message_id TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('requested', 'preparing', 'dispatching', 'completed', 'failed')),
    runtime_id TEXT,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
INSERT INTO workflow_recoveries_new SELECT * FROM workflow_recoveries;
DROP TABLE workflow_recoveries;
ALTER TABLE workflow_recoveries_new RENAME TO workflow_recoveries;
CREATE UNIQUE INDEX idx_workflow_recovery_active ON workflow_recoveries(workflow_id)
    WHERE state IN ('requested', 'preparing', 'dispatching');
CREATE TABLE workflow_agent_events (
    position INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    source TEXT NOT NULL,
    event_type TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_workflow_agent_events_session ON workflow_agent_events(session_id,position);
CREATE TABLE workflow_event_cursor (
    id INTEGER PRIMARY KEY CHECK (id=1),
    position INTEGER NOT NULL
);
INSERT INTO workflow_agent_events
SELECT p.position,e.event_id,e.session_id,e.turn_id,e.source,e.event_type,e.occurred_at,e.payload,e.created_at
FROM events e JOIN domain_event_positions p USING(event_id)
JOIN sessions s USING(session_id) WHERE s.application_id='workflow';
INSERT INTO workflow_event_cursor VALUES (1, (SELECT COALESCE(MAX(position),0) FROM domain_event_positions));

CREATE TABLE workflow_session_snapshots (
    session_id TEXT PRIMARY KEY NOT NULL,
    client_type TEXT NOT NULL,
    state TEXT NOT NULL,
    current_turn_id TEXT,
    position INTEGER NOT NULL,
    has_agent_binding INTEGER NOT NULL
);
CREATE TABLE workflow_turn_snapshots (
    turn_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL
);
CREATE INDEX idx_workflow_turn_snapshots_session ON workflow_turn_snapshots(session_id);
CREATE TABLE workflow_runtime_snapshots (
    runtime_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    state TEXT NOT NULL
);
CREATE INDEX idx_workflow_runtime_snapshots_session ON workflow_runtime_snapshots(session_id);
CREATE TABLE workflow_inbox_snapshots (
    message_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL,
    turn_id TEXT
);
CREATE INDEX idx_workflow_inbox_snapshots_session ON workflow_inbox_snapshots(session_id);
INSERT INTO workflow_session_snapshots SELECT s.session_id,s.client_type,s.state,s.current_turn_id,(SELECT MAX(position) FROM workflow_event_cursor),EXISTS(SELECT 1 FROM agent_bindings b WHERE b.session_id=s.session_id) FROM sessions s WHERE application_id='workflow';
INSERT INTO workflow_turn_snapshots SELECT turn_id,session_id,state FROM turns WHERE session_id IN (SELECT session_id FROM workflow_session_snapshots);
INSERT INTO workflow_runtime_snapshots SELECT runtime_id,session_id,role,state FROM session_runtimes WHERE session_id IN (SELECT session_id FROM workflow_session_snapshots);
INSERT INTO workflow_inbox_snapshots SELECT message_id,session_id,state,turn_id FROM inbox_messages WHERE session_id IN (SELECT session_id FROM workflow_session_snapshots);

-- Validate the rebuilt graph before clearing SQLite's deferred DROP bookkeeping.
CREATE TEMP TABLE application_boundary_integrity (violations INTEGER CHECK (violations = 0));
INSERT INTO application_boundary_integrity SELECT COUNT(*) FROM pragma_foreign_key_check;
DROP TABLE application_boundary_integrity;
PRAGMA defer_foreign_keys = OFF;

CREATE TABLE control_state_revision (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL
);
INSERT INTO control_state_revision VALUES (1, 0);
CREATE TRIGGER sessions_revision_insert AFTER INSERT ON sessions BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER sessions_revision_update AFTER UPDATE ON sessions BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER sessions_revision_delete AFTER DELETE ON sessions BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER turns_revision_insert AFTER INSERT ON turns BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER turns_revision_update AFTER UPDATE ON turns BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER turns_revision_delete AFTER DELETE ON turns BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER session_runtimes_revision_insert AFTER INSERT ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER session_runtimes_revision_update AFTER UPDATE ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER session_runtimes_revision_delete AFTER DELETE ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER agent_bindings_revision_insert AFTER INSERT ON agent_bindings BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER agent_bindings_revision_update AFTER UPDATE ON agent_bindings BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER agent_bindings_revision_delete AFTER DELETE ON agent_bindings BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER inbox_messages_revision_insert AFTER INSERT ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER inbox_messages_revision_update AFTER UPDATE ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER inbox_messages_revision_delete AFTER DELETE ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
ALTER TABLE workflow_session_snapshots ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
