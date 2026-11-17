CREATE TABLE bots (
    bot_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    client_type TEXT NOT NULL,
    client_key TEXT NOT NULL,
    model TEXT NOT NULL,
    current_session_id TEXT,
    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE bot_messages (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(bot_id),
    kind TEXT NOT NULL CHECK (kind IN ('user_input', 'bot_reply')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_bot_messages_bot_created
    ON bot_messages(bot_id, created_at, id);
