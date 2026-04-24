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
);

CREATE INDEX idx_events_session_id ON events(session_id, created_at, event_id);
CREATE INDEX idx_events_turn_id ON events(turn_id, created_at, event_id) WHERE turn_id IS NOT NULL;

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
);

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
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);

CREATE INDEX idx_turns_session_id ON turns(session_id, created_at, turn_id);

CREATE TABLE runtime_bindings (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_kind TEXT NOT NULL,
    runtime_ref TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);

CREATE TABLE artifacts (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    source_ref TEXT NOT NULL,
    size_bytes INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    FOREIGN KEY(turn_id) REFERENCES turns(turn_id)
);

CREATE INDEX idx_artifacts_session_turn ON artifacts(session_id, turn_id, artifact_id);

CREATE TABLE ingest_warnings (
    warning_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    warning TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE idempotency_keys (
    operation TEXT NOT NULL,
    key TEXT NOT NULL,
    response TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(operation, key)
);
