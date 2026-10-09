CREATE TABLE session_runtimes_without_fingerprint_check (
    runtime_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('starting', 'running', 'exited')),
    start_command TEXT,
    tmux_socket_path TEXT,
    tmux_pane_id TEXT,
    process_fingerprint TEXT,
    created_at TEXT NOT NULL,
    CHECK ((tmux_socket_path IS NULL) = (tmux_pane_id IS NULL))
);

INSERT INTO session_runtimes_without_fingerprint_check
SELECT runtime_id, session_id, role, state, start_command, tmux_socket_path, tmux_pane_id,
       process_fingerprint, created_at
FROM session_runtimes;

DROP TABLE session_runtimes;
ALTER TABLE session_runtimes_without_fingerprint_check RENAME TO session_runtimes;
CREATE INDEX idx_session_runtimes_session ON session_runtimes(session_id);
CREATE INDEX idx_session_runtimes_tmux ON session_runtimes(tmux_socket_path, tmux_pane_id);
