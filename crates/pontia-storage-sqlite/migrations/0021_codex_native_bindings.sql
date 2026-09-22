CREATE TABLE native_turn_bindings (
    session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    client_turn_id TEXT NOT NULL,
    turn_id TEXT NOT NULL UNIQUE,
    PRIMARY KEY (session_id, client_turn_id)
);

CREATE TABLE codex_tui_bindings (
    owner_session_id TEXT PRIMARY KEY NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    target_session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    runtime_instance_id TEXT NOT NULL,
    connected BOOLEAN NOT NULL DEFAULT FALSE,
    connection_id TEXT,
    tmux_socket_path TEXT,
    tmux_pane_id TEXT
);
