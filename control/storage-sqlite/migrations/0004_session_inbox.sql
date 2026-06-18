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

CREATE INDEX idx_inbox_messages_session_state
ON inbox_messages(session_id, state, delivery_policy, created_at, message_id);

CREATE INDEX idx_inbox_messages_turn
ON inbox_messages(turn_id)
WHERE turn_id IS NOT NULL;
