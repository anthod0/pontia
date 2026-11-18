CREATE TABLE bot_helpers (
    reference TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(bot_id),
    session_id TEXT UNIQUE
);
CREATE INDEX idx_bot_helpers_bot ON bot_helpers(bot_id);

CREATE TABLE bot_notifications (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(bot_id),
    source_session_id TEXT,
    content TEXT NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0 CHECK (delivered IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_bot_notifications_pending ON bot_notifications(delivered, bot_id);

CREATE TABLE bot_event_cursor (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    position INTEGER NOT NULL
);
INSERT INTO bot_event_cursor VALUES (1, 0);

CREATE TABLE bot_known_workspaces (
    bot_id TEXT NOT NULL REFERENCES bots(bot_id),
    workspace_id TEXT NOT NULL,
    PRIMARY KEY (bot_id, workspace_id)
);
