CREATE TABLE session_files (
    file_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions (session_id),
    turn_id TEXT REFERENCES turns (turn_id),
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    size INTEGER NOT NULL,
    media_type TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_session_files_page ON session_files (session_id, created_at DESC, file_id DESC);
