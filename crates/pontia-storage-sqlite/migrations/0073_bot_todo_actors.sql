ALTER TABLE bot_todos RENAME TO previous_bot_todos;

CREATE TABLE bot_todos (
    message_id TEXT PRIMARY KEY NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('open', 'done')),
    opened_by TEXT NOT NULL CHECK (opened_by IN ('user', 'bot')),
    opened_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    resolved_by TEXT CHECK (resolved_by IN ('user', 'bot')),
    resolved_message_id TEXT REFERENCES bot_messages(id) ON DELETE SET NULL,
    resolved_at TEXT,
    CHECK (
        (state = 'open' AND resolved_by IS NULL AND resolved_at IS NULL AND resolved_message_id IS NULL)
        OR
        (state = 'done' AND resolved_by IS NOT NULL AND resolved_at IS NOT NULL)
    )
);

CREATE INDEX idx_bot_todos_open ON bot_todos(opened_at DESC, message_id DESC) WHERE state = 'open';
CREATE INDEX idx_bot_todos_done ON bot_todos(resolved_at DESC, message_id DESC) WHERE state = 'done';

INSERT INTO bot_todos(message_id, state, opened_by, opened_at, resolved_by, resolved_message_id, resolved_at)
SELECT t.message_id, t.state, 'bot', m.created_at, t.resolved_by, t.resolved_message_id, t.resolved_at
FROM previous_bot_todos t JOIN bot_messages m ON m.id = t.message_id;

DROP TABLE previous_bot_todos;
