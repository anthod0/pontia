CREATE TABLE event_subscriptions (
    subscriber_id TEXT PRIMARY KEY NOT NULL,
    cursor_position INTEGER NOT NULL DEFAULT 0 CHECK (cursor_position >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- An absent legacy cursor contributes no initial progress.
CREATE TABLE IF NOT EXISTS workflow_event_cursor (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    position INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS bot_event_cursor (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    position INTEGER NOT NULL
);

INSERT INTO event_subscriptions (subscriber_id, cursor_position)
SELECT 'workflow.projection', position FROM workflow_event_cursor
UNION ALL
SELECT 'bot.helper_notifications', position FROM bot_event_cursor;
