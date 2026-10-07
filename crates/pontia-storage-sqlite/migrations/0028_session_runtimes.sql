CREATE TABLE session_runtimes (
    runtime_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('starting', 'running', 'exited')),
    start_command TEXT,
    tmux_socket_path TEXT,
    tmux_pane_id TEXT,
    process_fingerprint TEXT CHECK (process_fingerprint IS NULL OR (
        json_valid(process_fingerprint) AND COALESCE(
            json_type(process_fingerprint) = 'object'
            AND json_type(process_fingerprint, '$.boot_id') = 'text'
            AND json_type(process_fingerprint, '$.tmux_socket_path') = 'text'
            AND json_type(process_fingerprint, '$.tmux_pane_id') = 'text'
            AND json_type(process_fingerprint, '$.pane_pid') = 'integer'
            AND json_type(process_fingerprint, '$.pane_start_time_ticks') = 'integer'
            AND json_type(process_fingerprint, '$.agent_pid') = 'integer'
            AND json_type(process_fingerprint, '$.agent_start_time_ticks') = 'integer'
            AND json_type(process_fingerprint, '$.agent_comm') = 'text'
            AND json_type(process_fingerprint, '$.agent_argv0') IN ('text', 'null')
            AND json_remove(process_fingerprint, '$.boot_id', '$.tmux_socket_path', '$.tmux_pane_id',
                '$.pane_pid', '$.pane_start_time_ticks', '$.agent_pid', '$.agent_start_time_ticks',
                '$.agent_comm', '$.agent_argv0') = '{}', 0)
    )),
    created_at TEXT NOT NULL,
    CHECK ((tmux_socket_path IS NULL) = (tmux_pane_id IS NULL))
);
CREATE INDEX idx_session_runtimes_session ON session_runtimes(session_id);
CREATE INDEX idx_session_runtimes_tmux ON session_runtimes(tmux_socket_path, tmux_pane_id);

-- Stable runtime identities also replace the historical per-launch identities.
-- The map is transaction-local and disappears after reference conversion.
CREATE TEMP TABLE runtime_identity_map AS
SELECT session_id, 'runtime_' || lower(hex(randomblob(16))) AS runtime_id FROM sessions;

INSERT INTO session_runtimes
    (runtime_id, session_id, role, state, start_command, tmux_socket_path, tmux_pane_id, process_fingerprint, created_at)
SELECT m.runtime_id, r.session_id, 'tui',
    CASE WHEN s.state IN ('exited', 'error') THEN 'exited'
         WHEN r.binding_state = 'confirmed' THEN 'running' ELSE 'starting' END,
    r.start_command,
    CASE WHEN r.tmux_socket_path IS NOT NULL AND r.tmux_pane_id IS NOT NULL THEN r.tmux_socket_path END,
    CASE WHEN r.tmux_socket_path IS NOT NULL AND r.tmux_pane_id IS NOT NULL THEN r.tmux_pane_id END,
    CASE WHEN r.process_fingerprint IS NOT NULL AND r.tmux_socket_path IS NOT NULL AND r.tmux_pane_id IS NOT NULL
              AND json_valid(r.process_fingerprint)
              AND json_type(r.process_fingerprint, '$.boot_id') = 'text'
              AND json_type(r.process_fingerprint, '$.pane_pid') = 'integer'
              AND json_type(r.process_fingerprint, '$.pane_start_time_ticks') = 'integer'
              AND json_type(r.process_fingerprint, '$.agent_pid') = 'integer'
              AND json_type(r.process_fingerprint, '$.agent_start_time_ticks') = 'integer'
              AND json_type(r.process_fingerprint, '$.agent_comm') = 'text'
              AND json_type(r.process_fingerprint, '$.agent_argv0') IN ('text','null')
         THEN json_object(
            'boot_id', json_extract(r.process_fingerprint, '$.boot_id'),
            'tmux_socket_path', r.tmux_socket_path,
            'tmux_pane_id', r.tmux_pane_id,
            'pane_pid', json_extract(r.process_fingerprint, '$.pane_pid'),
            'pane_start_time_ticks', json_extract(r.process_fingerprint, '$.pane_start_time_ticks'),
            'agent_pid', json_extract(r.process_fingerprint, '$.agent_pid'),
            'agent_start_time_ticks', json_extract(r.process_fingerprint, '$.agent_start_time_ticks'),
            'agent_comm', json_extract(r.process_fingerprint, '$.agent_comm'),
            'agent_argv0', json_extract(r.process_fingerprint, '$.agent_argv0')) END,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM runtime_bindings r JOIN sessions s USING(session_id) JOIN runtime_identity_map m USING(session_id)
-- A frozen Codex backend/thread is not a TUI runtime. Its native history and
-- dedicated TUI records stay intact.
WHERE s.client_type != 'codex';

UPDATE inbox_messages SET required_runtime_instance_id = (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = inbox_messages.session_id) WHERE required_runtime_instance_id IS NOT NULL;
ALTER TABLE inbox_messages RENAME COLUMN required_runtime_instance_id TO required_runtime_id;

UPDATE workflow_nodes SET submitted_runtime_instance_id = (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = workflow_nodes.session_id) WHERE submitted_runtime_instance_id IS NOT NULL;
ALTER TABLE workflow_nodes RENAME COLUMN submitted_runtime_instance_id TO submitted_runtime_id;

UPDATE workflow_patches SET requesting_runtime_instance_id = (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = workflow_patches.requesting_session_id) WHERE requesting_runtime_instance_id IS NOT NULL;
ALTER TABLE workflow_patches RENAME COLUMN requesting_runtime_instance_id TO requesting_runtime_id;

UPDATE workflow_patches SET replanner_runtime_instance_id = (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = workflow_patches.replanner_session_id) WHERE replanner_runtime_instance_id IS NOT NULL;
ALTER TABLE workflow_patches RENAME COLUMN replanner_runtime_instance_id TO replanner_runtime_id;

UPDATE workflow_recoveries SET runtime_instance_id = (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = workflow_recoveries.session_id) WHERE runtime_instance_id IS NOT NULL;
ALTER TABLE workflow_recoveries RENAME COLUMN runtime_instance_id TO runtime_id;

UPDATE events SET payload = json_set(json_remove(payload, '$.runtime_instance_id'), '$.runtime_id',
    (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = events.session_id))
WHERE json_type(payload, '$.runtime_instance_id') IS NOT NULL;
UPDATE workflow_events SET payload = json_set(json_remove(payload, '$.runtime_instance_id'), '$.runtime_id',
    (SELECT runtime_id FROM runtime_identity_map m WHERE m.session_id = json_extract(workflow_events.payload, '$.session_id')))
WHERE json_type(payload, '$.runtime_instance_id') IS NOT NULL;
DROP TABLE runtime_bindings;
DROP TABLE runtime_identity_map;
