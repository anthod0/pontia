CREATE TABLE agent_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    client_type TEXT NOT NULL,
    launch_cwd TEXT NOT NULL,
    client_session_key TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    UNIQUE(session_id, client_type, client_session_key)
);

CREATE INDEX idx_agent_bindings_session ON agent_bindings(session_id, id);
CREATE INDEX idx_agent_bindings_identity ON agent_bindings(client_type, launch_cwd, client_session_key);
