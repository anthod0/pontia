CREATE TABLE runtime_bindings_new (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_kind TEXT NOT NULL,
    runtime_instance_id TEXT,
    start_command TEXT,
    launch_cwd TEXT,
    last_seen_at TEXT,
    tmux_socket_path TEXT,
    tmux_pane_id TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);

INSERT INTO runtime_bindings_new (
    session_id,
    runtime_kind,
    runtime_instance_id,
    start_command,
    launch_cwd,
    last_seen_at,
    tmux_socket_path,
    tmux_pane_id,
    metadata,
    updated_at
)
SELECT
    session_id,
    runtime_kind,
    json_extract(metadata, '$.runtime_instance_id'),
    json_extract(metadata, '$.start_command'),
    COALESCE(json_extract(metadata, '$.launch_cwd'), json_extract(metadata, '$.workspace')),
    COALESCE(json_extract(metadata, '$.last_seen_at'), updated_at),
    json_extract(metadata, '$.tmux_socket_path'),
    json_extract(metadata, '$.tmux_pane_id'),
    json_set(
        CASE
            WHEN json_type(metadata, '$.tmux_session') IS NOT NULL
             AND json_type(metadata, '$.tmux.session_name') IS NULL
            THEN json_set(metadata, '$.tmux.session_name', json_extract(metadata, '$.tmux_session'))
            ELSE metadata
        END,
        '$.legacy_runtime_ref',
        runtime_ref
    ),
    updated_at
FROM runtime_bindings;

DROP TABLE runtime_bindings;
ALTER TABLE runtime_bindings_new RENAME TO runtime_bindings;
