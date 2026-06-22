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

CREATE INDEX idx_session_lineage_parent ON session_lineage(parent_session_id, created_at, child_session_id);
