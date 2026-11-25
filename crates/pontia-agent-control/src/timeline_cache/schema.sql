CREATE TABLE IF NOT EXISTS cached_sessions (
    session_id TEXT PRIMARY KEY,
    last_active_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS cached_activity ON cached_sessions(last_active_at);
CREATE TABLE IF NOT EXISTS cached_sources (
    session_id TEXT NOT NULL REFERENCES cached_sessions ON DELETE CASCADE,
    source_key TEXT NOT NULL,
    generation INTEGER NOT NULL,
    binding_id TEXT NOT NULL,
    native_key TEXT NOT NULL,
    format_version INTEGER NOT NULL,
    render_version INTEGER NOT NULL,
    validator TEXT,
    PRIMARY KEY (session_id, source_key, generation)
);
CREATE TABLE IF NOT EXISTS cached_compactions (
    session_id TEXT NOT NULL REFERENCES cached_sessions ON DELETE CASCADE,
    event_id TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, event_id)
);
CREATE TABLE IF NOT EXISTS cached_turns (
    session_id TEXT NOT NULL,
    source_key TEXT NOT NULL,
    generation INTEGER NOT NULL,
    turn_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    head TEXT,
    tail TEXT,
    previous_tail TEXT,
    next_turn TEXT,
    encoding INTEGER NOT NULL,
    payload BLOB NOT NULL,
    PRIMARY KEY (session_id, source_key, generation, turn_id),
    FOREIGN KEY (session_id, source_key, generation) REFERENCES cached_sources ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS cached_checkpoints (
    session_id TEXT NOT NULL,
    source_key TEXT NOT NULL,
    generation INTEGER NOT NULL,
    turn_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    item_json TEXT NOT NULL,
    PRIMARY KEY (session_id, source_key, generation, turn_id, position),
    FOREIGN KEY (session_id, source_key, generation, turn_id) REFERENCES cached_turns ON DELETE CASCADE
);
PRAGMA user_version = 1;
