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

CREATE INDEX idx_workspaces_last_used ON workspaces(last_used_at, workspace_id);

ALTER TABLE sessions ADD COLUMN workspace_id TEXT REFERENCES workspaces(workspace_id);

CREATE INDEX idx_sessions_workspace ON sessions(workspace_id, state, updated_at, session_id);

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

CREATE INDEX idx_tasks_state_created ON tasks(state, created_at, task_id);
CREATE INDEX idx_tasks_workspace ON tasks(workspace_id, created_at, task_id);
CREATE INDEX idx_tasks_session ON tasks(session_id, created_at, task_id);

CREATE TABLE task_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(task_id) REFERENCES tasks(task_id)
);

CREATE INDEX idx_task_events_task ON task_events(task_id, created_at, event_id);
