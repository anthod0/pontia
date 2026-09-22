-- Squashed baseline for the current Pontia SQLite schema.
PRAGMA foreign_keys = ON;

-- Tables
CREATE TABLE events (
    event_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    source TEXT NOT NULL,
    client_type TEXT NOT NULL,
    event_type TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    seq INTEGER,
    payload TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
, turn_index INTEGER, timeline_boundary TEXT, turn_topology TEXT);

CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    client_type TEXT NOT NULL,
    workspace_ref TEXT,
    state TEXT NOT NULL,
    current_turn_id TEXT,
    state_version INTEGER NOT NULL DEFAULT 0,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
, workspace_id TEXT REFERENCES workspaces(workspace_id), handle TEXT, role TEXT, description TEXT, execution_profile_id TEXT, execution_profile_version TEXT, title TEXT, pinned_at TEXT, archived_at TEXT, next_turn_index INTEGER NOT NULL DEFAULT 1);

CREATE TABLE turns (
    turn_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL,
    state_version INTEGER NOT NULL DEFAULT 0,
    input_summary TEXT,
    output_summary TEXT,
    failure_message TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), turn_index INTEGER, head_cursor TEXT, tail_cursor TEXT, parent_turn_id TEXT, topology_status TEXT NOT NULL DEFAULT 'unknown',
    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);

CREATE TABLE idempotency_keys (
    operation TEXT NOT NULL,
    key TEXT NOT NULL,
    response TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(operation, key)
);

CREATE TABLE workspaces (
    workspace_id TEXT PRIMARY KEY NOT NULL,
    canonical_path TEXT NOT NULL UNIQUE,
    display_path TEXT NOT NULL,
    name TEXT,
    state TEXT NOT NULL DEFAULT 'active',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_used_at TEXT
);

CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL,
    input TEXT NOT NULL,
    workspace_id TEXT,
    session_id TEXT,
    turn_id TEXT,
    routing_state TEXT NOT NULL DEFAULT 'pending',
    routing_reason TEXT,
    routing_confidence REAL,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    FOREIGN KEY(turn_id) REFERENCES turns(turn_id)
);

CREATE TABLE task_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(task_id) REFERENCES tasks(task_id)
);

CREATE TABLE inbox_messages (
    message_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL,
    delivery_policy TEXT NOT NULL,
    input_summary TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    turn_id TEXT,
    superseded_by_message_id TEXT,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    dispatched_at TEXT,
    cancelled_at TEXT,

    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    FOREIGN KEY(turn_id) REFERENCES turns(turn_id)
);

CREATE TABLE execution_profiles (
    profile_id TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    supported_client_types TEXT NOT NULL DEFAULT '[]',
    system_prompt_template TEXT,
    turn_prompt_template TEXT,
    default_session_role TEXT,
    default_session_description TEXT,
    handle_prefix TEXT,
    expected_output_schema TEXT,
    artifact_contract TEXT NOT NULL DEFAULT '{}',
    default_execution_policy TEXT NOT NULL DEFAULT '{}',
    default_review_policy TEXT NOT NULL DEFAULT '{}',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)), archived_at TEXT, archived_reason TEXT, agent_kind TEXT NOT NULL DEFAULT 'executor',
    PRIMARY KEY(profile_id, version)
);

INSERT INTO execution_profiles (
    profile_id,
    version,
    name,
    description,
    supported_client_types,
    system_prompt_template,
    turn_prompt_template,
    default_session_role,
    default_session_description,
    handle_prefix,
    expected_output_schema,
    artifact_contract,
    default_execution_policy,
    default_review_policy,
    metadata,
    active,
    agent_kind
) VALUES (
    'default',
    '1',
    'Default',
    'General coding agent execution template.',
    '["pi"]',
    'You are a coding agent. Follow the assigned task and report concise results.',
    '{{input}}',
    'General coding agent',
    'Executes coding tasks.',
    'agent',
    'free_text',
    '{}',
    '{}',
    '{}',
    '{"builtin":true}',
    1,
    'executor'
);

CREATE TABLE agent_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    client_type TEXT NOT NULL,
    launch_cwd TEXT NOT NULL,
    client_session_key TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), discovered BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    UNIQUE(session_id, client_type, client_session_key)
);

CREATE TABLE workspace_git_status (
  workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
  repo_root TEXT,
  branch TEXT,
  upstream TEXT,
  ahead INTEGER NOT NULL DEFAULT 0,
  behind INTEGER NOT NULL DEFAULT 0,
  staged_count INTEGER NOT NULL DEFAULT 0,
  unstaged_count INTEGER NOT NULL DEFAULT 0,
  untracked_count INTEGER NOT NULL DEFAULT 0,
  conflicted_count INTEGER NOT NULL DEFAULT 0,
  clean INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL,
  failure TEXT,
  observed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE "runtime_bindings" (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_kind TEXT NOT NULL,
    runtime_instance_id TEXT,
    start_command TEXT,
    launch_cwd TEXT,
    last_seen_at TEXT,
    tmux_socket_path TEXT,
    tmux_pane_id TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);

CREATE TABLE session_lineage (
    child_session_id TEXT PRIMARY KEY NOT NULL,
    parent_session_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    forked_from_turn_id TEXT,
    forked_from_client_node_id TEXT,
    parent_client_session_key TEXT,
    child_client_session_key TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(child_session_id) REFERENCES sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY(parent_session_id) REFERENCES sessions(session_id) ON DELETE CASCADE,
    CHECK (relation_type IN ('fork'))
);


-- Indexes
CREATE INDEX idx_events_session_id ON events(session_id, created_at, event_id);

CREATE INDEX idx_events_turn_id ON events(turn_id, created_at, event_id) WHERE turn_id IS NOT NULL;

CREATE INDEX idx_turns_session_id ON turns(session_id, created_at, turn_id);

CREATE INDEX idx_workspaces_last_used ON workspaces(last_used_at, workspace_id);

CREATE INDEX idx_sessions_workspace ON sessions(workspace_id, state, updated_at, session_id);

CREATE INDEX idx_tasks_state_created ON tasks(state, created_at, task_id);

CREATE INDEX idx_tasks_workspace ON tasks(workspace_id, created_at, task_id);

CREATE INDEX idx_tasks_session ON tasks(session_id, created_at, task_id);

CREATE INDEX idx_task_events_task ON task_events(task_id, created_at, event_id);

CREATE INDEX idx_inbox_messages_session_state
ON inbox_messages(session_id, state, delivery_policy, created_at, message_id);

CREATE INDEX idx_inbox_messages_turn
ON inbox_messages(turn_id)
WHERE turn_id IS NOT NULL;

CREATE INDEX idx_execution_profiles_profile_created ON execution_profiles(profile_id, created_at, version);

CREATE INDEX idx_sessions_execution_profile
    ON sessions(workspace_id, execution_profile_id, execution_profile_version, state, updated_at, session_id);

CREATE UNIQUE INDEX idx_sessions_workspace_handle
ON sessions(workspace_id, handle)
WHERE handle IS NOT NULL
  AND state NOT IN ('exited', 'error');

CREATE INDEX idx_execution_profiles_active_latest ON execution_profiles(profile_id, active, archived_at, created_at, version);

CREATE INDEX idx_agent_bindings_session ON agent_bindings(session_id, id);

CREATE INDEX idx_agent_bindings_identity ON agent_bindings(client_type, launch_cwd, client_session_key);

CREATE INDEX idx_session_lineage_parent ON session_lineage(parent_session_id, created_at, child_session_id);

CREATE INDEX idx_sessions_management_list ON sessions(archived_at, pinned_at, updated_at, session_id);

CREATE UNIQUE INDEX idx_turns_session_turn_index
ON turns(session_id, turn_index);

CREATE UNIQUE INDEX idx_agent_bindings_one_per_session
ON agent_bindings(session_id);

CREATE UNIQUE INDEX idx_agent_bindings_unique_client_identity
ON agent_bindings(client_type, client_session_key);


-- Triggers
CREATE TRIGGER turns_require_turn_index
BEFORE INSERT ON turns
WHEN NEW.turn_index IS NULL
BEGIN
    SELECT RAISE(ABORT, 'turn_index is required');
END;

CREATE TRIGGER turns_require_matching_event_turn_index
BEFORE INSERT ON turns
WHEN EXISTS (
    SELECT 1
    FROM events
    WHERE events.session_id = NEW.session_id
      AND events.turn_id = NEW.turn_id
      AND events.turn_index IS NOT NEW.turn_index
)
BEGIN
    SELECT RAISE(ABORT, 'turn projection index must match its event envelope indexes');
END;

CREATE TRIGGER turns_preserve_turn_identity
BEFORE UPDATE OF session_id, turn_index ON turns
WHEN NEW.session_id IS NOT OLD.session_id
  OR NEW.turn_index IS NOT OLD.turn_index
BEGIN
    SELECT RAISE(ABORT, 'turn session_id and turn_index are immutable');
END;

CREATE TRIGGER turn_events_require_turn_identity
BEFORE INSERT ON events
WHEN NEW.event_type LIKE 'turn.%'
 AND (NEW.turn_id IS NULL OR NEW.turn_index IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'turn event turn_id and turn_index are required');
END;

CREATE TRIGGER turn_events_require_matching_turn_index
BEFORE INSERT ON events
WHEN NEW.event_type LIKE 'turn.%'
 AND EXISTS (
    SELECT 1
    FROM turns
    WHERE turns.session_id = NEW.session_id
      AND turns.turn_id = NEW.turn_id
      AND turns.turn_index IS NOT NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'turn event index must match the Turn projection index');
END;

CREATE TRIGGER session_events_reject_turn_index
BEFORE INSERT ON events
WHEN NEW.event_type NOT LIKE 'turn.%'
 AND NEW.turn_index IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'session event cannot have turn_index');
END;

CREATE TRIGGER events_preserve_turn_index
BEFORE UPDATE OF turn_index ON events
WHEN NEW.turn_index IS NOT OLD.turn_index
BEGIN
    SELECT RAISE(ABORT, 'event turn_index is immutable');
END;

CREATE TRIGGER turns_validate_topology_on_insert
BEFORE INSERT ON turns
WHEN NOT (
    (NEW.topology_status IN ('unknown', 'root') AND NEW.parent_turn_id IS NULL)
    OR (NEW.topology_status = 'linked' AND NEW.parent_turn_id IS NOT NULL AND trim(NEW.parent_turn_id) <> '')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology status/parent combination');
END;

CREATE TRIGGER turns_validate_topology_on_update
BEFORE UPDATE OF topology_status, parent_turn_id ON turns
WHEN NOT (
    (NEW.topology_status IN ('unknown', 'root') AND NEW.parent_turn_id IS NULL)
    OR (NEW.topology_status = 'linked' AND NEW.parent_turn_id IS NOT NULL AND trim(NEW.parent_turn_id) <> '')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology status/parent combination');
END;

CREATE TRIGGER turns_validate_linked_parent_on_insert
BEFORE INSERT ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_index < NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turns_validate_linked_parent_on_update
BEFORE UPDATE OF topology_status, parent_turn_id, session_id, turn_index ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_index < NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turns_preserve_resolved_topology
BEFORE UPDATE OF topology_status, parent_turn_id ON turns
WHEN OLD.topology_status <> 'unknown'
 AND (NEW.topology_status IS NOT OLD.topology_status OR NEW.parent_turn_id IS NOT OLD.parent_turn_id)
BEGIN
    SELECT RAISE(ABORT, 'resolved Turn topology is immutable');
END;

CREATE TRIGGER turn_events_validate_topology
BEFORE INSERT ON events
WHEN NEW.turn_topology IS NOT NULL
 AND (
    NEW.event_type <> 'turn.started'
    OR NOT json_valid(NEW.turn_topology)
    OR COALESCE(json_extract(NEW.turn_topology, '$.status'), '') NOT IN ('unknown', 'root', 'linked')
    OR (
        json_extract(NEW.turn_topology, '$.status') IN ('unknown', 'root')
        AND json_type(NEW.turn_topology, '$.parent_turn_id') IS NOT NULL
    )
    OR (
        json_extract(NEW.turn_topology, '$.status') = 'linked'
        AND COALESCE(trim(json_extract(NEW.turn_topology, '$.parent_turn_id')), '') = ''
    )
 )
BEGIN
    SELECT RAISE(ABORT, 'invalid turn.started topology enrichment');
END;

CREATE TRIGGER turn_events_validate_linked_parent
BEFORE INSERT ON events
WHEN NEW.turn_topology IS NOT NULL
 AND json_valid(NEW.turn_topology)
 AND json_extract(NEW.turn_topology, '$.status') = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = json_extract(NEW.turn_topology, '$.parent_turn_id')
      AND parent.session_id = NEW.session_id
      AND parent.turn_index < NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'linked event parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER events_preserve_turn_topology
BEFORE UPDATE OF turn_topology ON events
WHEN NEW.turn_topology IS NOT OLD.turn_topology
BEGIN
    SELECT RAISE(ABORT, 'event Turn topology enrichment is immutable');
END;
