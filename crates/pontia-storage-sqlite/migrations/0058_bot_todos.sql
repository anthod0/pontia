CREATE TABLE bot_todos (
    message_id TEXT PRIMARY KEY NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('open', 'done')),
    resolved_by TEXT CHECK (resolved_by IN ('user', 'bot')),
    resolved_message_id TEXT REFERENCES bot_messages(id) ON DELETE SET NULL,
    resolved_at TEXT
);
