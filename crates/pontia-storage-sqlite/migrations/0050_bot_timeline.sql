-- Discard sub-millisecond digits before conversion. SQLite date functions round fractional seconds.
ALTER TABLE bot_messages ADD COLUMN timeline_ms INTEGER GENERATED ALWAYS AS (
    unixepoch(substr(created_at, 1, 19) ||
        CASE WHEN substr(created_at, -1) = 'Z' THEN 'Z' ELSE substr(created_at, -6) END) * 1000
    + CASE WHEN substr(created_at, 20, 1) = '.' THEN
        CAST(substr(substr(created_at, 21, length(created_at) - 20 -
            CASE WHEN substr(created_at, -1) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
      ELSE 0 END
) VIRTUAL;

ALTER TABLE events ADD COLUMN timeline_ms INTEGER GENERATED ALWAYS AS (
    unixepoch(substr(occurred_at, 1, 19) ||
        CASE WHEN substr(occurred_at, -1) = 'Z' THEN 'Z' ELSE substr(occurred_at, -6) END) * 1000
    + CASE WHEN substr(occurred_at, 20, 1) = '.' THEN
        CAST(substr(substr(occurred_at, 21, length(occurred_at) - 20 -
            CASE WHEN substr(occurred_at, -1) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
      ELSE 0 END
) VIRTUAL;

CREATE INDEX idx_bot_messages_timeline
    ON bot_messages(bot_id, timeline_ms, id);

CREATE INDEX idx_bot_messages_latest_normal
    ON bot_messages(bot_id, timeline_ms, id) WHERE status = 'normal';

CREATE INDEX idx_events_type_timeline
    ON events(session_id, timeline_ms, event_id)
    WHERE event_type IN ('turn.failed', 'session.error', 'session.exited');

CREATE INDEX idx_turns_active_session
    ON turns(session_id) WHERE state IN ('queued', 'running');
