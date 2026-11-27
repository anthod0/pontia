ALTER TABLE inbox_messages RENAME TO inbox_messages_legacy;

CREATE TABLE inbox_messages (
    message_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(session_id),
    input_summary TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO inbox_messages (message_id, session_id, input_summary, metadata, created_at)
SELECT message_id, session_id, input_summary, metadata, created_at
FROM inbox_messages_legacy
WHERE state = 'pending'
ORDER BY rowid;

DROP TABLE inbox_messages_legacy;

CREATE INDEX idx_inbox_messages_session_created
ON inbox_messages(session_id, created_at, message_id);

CREATE TRIGGER inbox_messages_revision_insert AFTER INSERT ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
CREATE TRIGGER inbox_messages_revision_delete AFTER DELETE ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
