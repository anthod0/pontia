CREATE TABLE runtime_bindings_new (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_kind TEXT NOT NULL,
    runtime_instance_id TEXT,
    binding_state TEXT NOT NULL DEFAULT 'provisioned'
        CHECK (binding_state IN ('provisioned', 'confirmed')),
    runtime_handle TEXT,
    start_command TEXT,
    launch_cwd TEXT,
    internal_event_url TEXT,
    started_at TEXT,
    last_seen_at TEXT,
    restart_count INTEGER NOT NULL DEFAULT 0,
    tmux_socket_path TEXT,
    tmux_pane_id TEXT,
    process_fingerprint TEXT
        CHECK (process_fingerprint IS NULL OR json_valid(process_fingerprint)),
    capabilities TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(capabilities)),
    diagnostics TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(diagnostics)),
    adapter_details TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(adapter_details)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
);

INSERT INTO runtime_bindings_new (
    session_id,
    runtime_kind,
    runtime_instance_id,
    binding_state,
    runtime_handle,
    start_command,
    launch_cwd,
    internal_event_url,
    started_at,
    last_seen_at,
    restart_count,
    tmux_socket_path,
    tmux_pane_id,
    process_fingerprint,
    capabilities,
    diagnostics,
    adapter_details,
    updated_at
)
SELECT
    session_id,
    runtime_kind,
    runtime_instance_id,
    CASE WHEN COALESCE(json_extract(metadata, '$.binding_confirmed'), 0) = 1
         THEN 'confirmed' ELSE 'provisioned' END,
    COALESCE(
        json_extract(metadata, '$.in_process.runtime_handle'),
        json_extract(metadata, '$.in_process.runtime_key'),
        json_extract(metadata, '$.tmux_session')
    ),
    start_command,
    launch_cwd,
    json_extract(metadata, '$.internal_event_url'),
    json_extract(metadata, '$.started_at'),
    last_seen_at,
    COALESCE(json_extract(metadata, '$.restart_count'), 0),
    tmux_socket_path,
    tmux_pane_id,
    CASE WHEN json_type(metadata, '$.tmux_process_fingerprint') = 'object'
         THEN json_extract(metadata, '$.tmux_process_fingerprint') ELSE NULL END,
    COALESCE(json_extract(metadata, '$.capabilities'), '{}'),
    json_object(
        'launch_id', json_extract(metadata, '$.launch_id'),
        'log_dir', json_extract(metadata, '$.log_dir'),
        'runtime_log', json_extract(metadata, '$.runtime_log'),
        'log_path', json_extract(metadata, '$.log_path'),
        'pi_hook_log', json_extract(metadata, '$.pi_hook_log'),
        'claude_hook_log', json_extract(metadata, '$.claude_hook_log')
    ),
    json_object(
        'tmux', json_extract(metadata, '$.tmux'),
        'in_process', json_extract(metadata, '$.in_process')
    ),
    updated_at
FROM runtime_bindings;

CREATE TABLE pending_turn_contexts (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_instance_id TEXT NOT NULL,
    client_type TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES runtime_bindings_new(session_id) ON DELETE CASCADE
);

INSERT INTO pending_turn_contexts (session_id, runtime_instance_id, client_type, payload)
SELECT
    session_id,
    json_extract(metadata, '$.pending_current_turn.runtime_instance_id'),
    json_extract(metadata, '$.pending_current_turn.client_type'),
    json_extract(metadata, '$.pending_current_turn')
FROM runtime_bindings
WHERE json_type(metadata, '$.pending_current_turn') = 'object'
  AND json_extract(metadata, '$.pending_current_turn.runtime_instance_id') IS NOT NULL
  AND json_extract(metadata, '$.pending_current_turn.client_type') IS NOT NULL;

DROP TABLE runtime_bindings;
ALTER TABLE runtime_bindings_new RENAME TO runtime_bindings;

CREATE INDEX idx_runtime_bindings_tmux_unconfirmed
    ON runtime_bindings(tmux_socket_path, tmux_pane_id, binding_state);
