# Local control-plane SQLite database

```sql
CREATE TABLE bots (
    bot_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    client_type TEXT NOT NULL,
    client_key TEXT NOT NULL,
    model TEXT NOT NULL,
    current_session_id TEXT,
    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    awaiting_reply INTEGER NOT NULL DEFAULT 0 CHECK (awaiting_reply IN (0, 1)),
    awaiting_reply_after INTEGER,
    avatar_shape TEXT NOT NULL DEFAULT 'classic',
    avatar_color TEXT NOT NULL DEFAULT 'blue',
    idle_since TEXT,
    idle_turn_id TEXT,
    reasoning_effort TEXT
);

CREATE TABLE bot_messages (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(bot_id),
    kind TEXT NOT NULL CHECK (kind IN ('user_input', 'bot_reply')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    status TEXT NOT NULL DEFAULT 'normal'
        CHECK (status = 'normal' OR (status = 'failed' AND kind = 'user_input')),
    timeline_ms INTEGER GENERATED ALWAYS AS (
    unixepoch(substr(created_at, 1, 19) ||
        CASE WHEN substr(created_at, -1) = 'Z' THEN 'Z' ELSE substr(created_at, -6) END) * 1000
    + CASE WHEN substr(created_at, 20, 1) = '.' THEN
        CAST(substr(substr(created_at, 21, length(created_at) - 20 -
            CASE WHEN substr(created_at, -1) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
      ELSE 0 END
) VIRTUAL,
    link_source TEXT
        CHECK (link_source IS NULL OR (kind = 'user_input' AND link_source IN ('quote', 'bot')))
);

CREATE INDEX idx_bot_messages_bot_created
    ON bot_messages(bot_id, created_at, id);

CREATE INDEX idx_bot_messages_timeline
    ON bot_messages(bot_id, timeline_ms, id);

CREATE INDEX idx_bot_messages_latest_normal
    ON bot_messages(bot_id, timeline_ms, id) WHERE status = 'normal';

CREATE TABLE bot_message_links (
    message_id TEXT NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, target_id)
);

CREATE INDEX idx_bot_message_links_target ON bot_message_links(target_id);

CREATE TABLE bot_todos (
    message_id TEXT PRIMARY KEY NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('open', 'done')),
    opened_by TEXT NOT NULL CHECK (opened_by IN ('user', 'bot')),
    opened_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    resolved_by TEXT CHECK (resolved_by IN ('user', 'bot')),
    resolved_message_id TEXT REFERENCES bot_messages(id) ON DELETE SET NULL,
    resolved_at TEXT,
    CHECK (
        (state = 'open' AND resolved_by IS NULL AND resolved_at IS NULL AND resolved_message_id IS NULL)
        OR
        (state = 'done' AND resolved_by IS NOT NULL AND resolved_at IS NOT NULL)
    )
);

CREATE INDEX idx_bot_todos_open ON bot_todos(opened_at DESC, message_id DESC) WHERE state = 'open';
CREATE INDEX idx_bot_todos_done ON bot_todos(resolved_at DESC, message_id DESC) WHERE state = 'done';

CREATE TABLE bot_memories (
    id TEXT PRIMARY KEY NOT NULL,

    kind TEXT NOT NULL CHECK (kind IN (
        'fact', 'decision', 'event', 'lesson', 'observation'
    )),

    content TEXT NOT NULL CHECK (length(trim(content)) > 0),

    keywords TEXT NOT NULL
        CHECK (
            json_valid(keywords)
            AND json_type(keywords) = 'array'
            AND json_array_length(keywords) > 0
        ),

    workspace_id TEXT,
    session_id TEXT,

    organized INTEGER NOT NULL DEFAULT 0 CHECK (organized IN (0, 1)),

    archived INTEGER NOT NULL DEFAULT 0
        CHECK (archived IN (0, 1)),

    importance TEXT NOT NULL DEFAULT 'normal'
        CHECK (importance IN ('low', 'normal', 'high')),

    occurred_at TEXT,

    sources TEXT,

    created_by TEXT NOT NULL
        CHECK (created_by IN ('bot', 'dreaming')),

    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_read_at TEXT
);

CREATE TABLE bot_memory_edges (
    memory_a_id TEXT NOT NULL REFERENCES bot_memories(id),
    memory_b_id TEXT NOT NULL REFERENCES bot_memories(id),

    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    PRIMARY KEY (memory_a_id, memory_b_id),
    CHECK (memory_a_id < memory_b_id)
);

CREATE INDEX idx_bot_memory_edges_b
    ON bot_memory_edges(memory_b_id);

CREATE INDEX idx_bot_memories_pending
    ON bot_memories(id) WHERE archived = 0 AND organized = 0;

CREATE INDEX idx_bot_memories_active
    ON bot_memories(workspace_id, kind, updated_at DESC)
    WHERE archived = 0;

CREATE INDEX idx_bot_memories_session
    ON bot_memories(session_id)
    WHERE session_id IS NOT NULL;

CREATE VIRTUAL TABLE bot_memory_keywords USING fts5(keywords, tokenize = 'ascii');

CREATE TABLE agent_clients (
    client_type TEXT NOT NULL,
    client_key TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    version TEXT,
    version_checked_at TEXT,
    model_catalog TEXT,
    PRIMARY KEY (client_type, client_key)
);

-- session.created payloads store the Session mode as default, yolo, or restricted.
CREATE TABLE events (
  event_id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  turn_id TEXT,
  source TEXT NOT NULL,
  client_type TEXT NOT NULL,
  event_type TEXT NOT NULL,
  occurred_at TEXT NOT NULL,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  timeline_boundary TEXT,
  turn_topology TEXT,
  report_key TEXT,
    timeline_ms INTEGER GENERATED ALWAYS AS (
    unixepoch(substr(occurred_at, 1, 19) ||
        CASE WHEN substr(occurred_at, -1) = 'Z' THEN 'Z' ELSE substr(occurred_at, -6) END) * 1000
    + CASE WHEN substr(occurred_at, 20, 1) = '.' THEN
        CAST(substr(substr(occurred_at, 21, length(occurred_at) - 20 -
            CASE WHEN substr(occurred_at, -1) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
      ELSE 0 END
) VIRTUAL
);

CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    client_type TEXT NOT NULL,
    workspace_ref TEXT,
    state TEXT NOT NULL,
    current_turn_id TEXT,
    state_version INTEGER NOT NULL DEFAULT 0,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
, workspace_id TEXT REFERENCES workspaces(workspace_id), handle TEXT, role TEXT, description TEXT, title TEXT, pinned_at TEXT, archived_at TEXT, application_id TEXT, application_ref TEXT, runtime_environment TEXT NOT NULL DEFAULT '{}', exit_when_done INTEGER NOT NULL DEFAULT 0 CHECK (exit_when_done IN (0, 1)), credential_env TEXT, credential_sha256 BLOB);

CREATE TABLE turns (
  turn_id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  state TEXT NOT NULL, -- queued, running, completed, failed, interrupted
  state_version INTEGER NOT NULL DEFAULT 0,
  input_summary TEXT,
  output_summary TEXT,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  head_cursor TEXT,
  tail_cursor TEXT,
  parent_turn_id TEXT,
  topology_status TEXT NOT NULL DEFAULT 'unknown',
  FOREIGN KEY (session_id) REFERENCES sessions (session_id)
);

CREATE TABLE workspaces (
  workspace_id TEXT PRIMARY KEY NOT NULL,
  canonical_path TEXT NOT NULL UNIQUE,
  display_path TEXT NOT NULL,
  name TEXT,
  description TEXT,
  state TEXT NOT NULL DEFAULT 'active',
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  last_used_at TEXT
);

CREATE TABLE tasks (
  task_id TEXT PRIMARY KEY NOT NULL,
  state TEXT NOT NULL,
  input TEXT NOT NULL,
  workspace_id TEXT,
  session_id TEXT,
  turn_id TEXT,
  routing_state TEXT NOT NULL DEFAULT 'pending',
  routing_reason TEXT,
  routing_confidence REAL,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  FOREIGN KEY (workspace_id) REFERENCES workspaces (workspace_id),
  FOREIGN KEY (session_id) REFERENCES sessions (session_id),
  FOREIGN KEY (turn_id) REFERENCES turns (turn_id)
);

CREATE TABLE task_events (
  event_id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  FOREIGN KEY (task_id) REFERENCES tasks (task_id)
);

CREATE TABLE inbox_messages (
  message_id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  state TEXT NOT NULL,
  delivery_policy TEXT NOT NULL,
  input_summary TEXT NOT NULL,
  metadata TEXT NOT NULL DEFAULT '{}',
  turn_id TEXT,
  superseded_by_message_id TEXT,
  failure_message TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  dispatched_at TEXT,
  cancelled_at TEXT,
  branch_target_turn_id TEXT REFERENCES turns (turn_id),
  steer_target_turn_id TEXT REFERENCES turns (turn_id),
  retry_of_message_id TEXT REFERENCES inbox_messages (message_id),
  required_runtime_id TEXT,
  FOREIGN KEY (session_id) REFERENCES sessions (session_id),
  FOREIGN KEY (turn_id) REFERENCES turns (turn_id)
);

CREATE TABLE agent_profiles (
  profile_id TEXT NOT NULL PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  client_type TEXT,
  client_key TEXT,
  model TEXT CHECK (
      (client_type IS NULL AND client_key IS NULL AND model IS NULL)
      OR (client_type IS NOT NULL AND client_key IS NOT NULL AND model IS NOT NULL)
  ),
  system_prompt TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  reasoning_effort TEXT CHECK (reasoning_effort IS NULL OR model IS NOT NULL),
  use_yolo INTEGER NOT NULL DEFAULT 0 CHECK (use_yolo IN (0, 1))
);

CREATE TABLE agent_bindings (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  client_type TEXT NOT NULL,
  launch_cwd TEXT NOT NULL,
  client_session_key TEXT NOT NULL,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  discovered BOOLEAN NOT NULL DEFAULT FALSE,
  client_session_file TEXT,
  FOREIGN KEY (session_id) REFERENCES sessions (session_id),
  UNIQUE (session_id, client_type, client_session_key)
);

CREATE TABLE workspace_git_status (
  workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES workspaces (workspace_id) ON DELETE CASCADE,
  repo_root TEXT,
  branch TEXT,
  upstream TEXT,
  ahead INTEGER NOT NULL DEFAULT 0,
  behind INTEGER NOT NULL DEFAULT 0,
  staged_count INTEGER NOT NULL DEFAULT 0,
  unstaged_count INTEGER NOT NULL DEFAULT 0,
  untracked_count INTEGER NOT NULL DEFAULT 0,
  conflicted_count INTEGER NOT NULL DEFAULT 0,
  clean INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL,
  failure TEXT,
  observed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE session_lineage (
  child_session_id TEXT PRIMARY KEY NOT NULL,
  parent_session_id TEXT NOT NULL,
  relation_type TEXT NOT NULL,
  forked_from_turn_id TEXT,
  forked_from_client_node_id TEXT,
  parent_client_session_key TEXT,
  child_client_session_key TEXT,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  FOREIGN KEY (child_session_id) REFERENCES sessions (session_id) ON DELETE CASCADE,
  FOREIGN KEY (parent_session_id) REFERENCES sessions (session_id) ON DELETE CASCADE,
  CHECK (relation_type IN ('fork'))
);

CREATE TABLE workflows (
    workflow_id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    cwd TEXT NOT NULL,
    state TEXT NOT NULL,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    started_at TEXT,
    completed_at TEXT
, activating_node_id TEXT REFERENCES workflow_nodes(node_id), initiator_session_id TEXT NOT NULL, last_report_sequence INTEGER NOT NULL DEFAULT 0);

CREATE TABLE workflow_nodes (
  node_id TEXT PRIMARY KEY NOT NULL,
  workflow_id TEXT NOT NULL,
  parent_node_id TEXT,
  title TEXT NOT NULL,
  instructions TEXT NOT NULL,
  inputs TEXT NOT NULL DEFAULT '[]',
  output TEXT NOT NULL,
  execution_profile_id TEXT,
  session_id TEXT,
  submitted_at TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  node_type TEXT NOT NULL DEFAULT 'agent',
  phase TEXT NOT NULL DEFAULT '',
  submitted_runtime_id TEXT,
  exit_request_started_at TEXT,
  FOREIGN KEY (workflow_id) REFERENCES workflows (workflow_id),
  FOREIGN KEY (parent_node_id) REFERENCES workflow_nodes (node_id)
);

CREATE TABLE workflow_events (
  event_id TEXT PRIMARY KEY NOT NULL,
  workflow_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  FOREIGN KEY (workflow_id) REFERENCES workflows (workflow_id),
  UNIQUE (workflow_id, sequence)
);

CREATE TABLE native_turn_bindings (
  session_id TEXT NOT NULL REFERENCES sessions (session_id) ON DELETE CASCADE,
  client_turn_id TEXT NOT NULL,
  turn_id TEXT NOT NULL UNIQUE,
  PRIMARY KEY (session_id, client_turn_id)
);

CREATE TABLE workflow_recoveries (
    recovery_id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id),
    failure_event_id TEXT NOT NULL UNIQUE REFERENCES workflow_events(event_id),
    exit_event_id TEXT NOT NULL,
    node_id TEXT NOT NULL REFERENCES workflow_nodes(node_id),
    session_id TEXT NOT NULL,
    message_id TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('requested', 'preparing', 'dispatching', 'completed', 'failed')),
    runtime_id TEXT,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE interactions (
  interaction_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions (session_id),
  turn_id TEXT REFERENCES turns (turn_id),
  type TEXT NOT NULL CHECK (type = 'choice'),
  title TEXT NOT NULL,
  description TEXT,
  options TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'expired')),
  response_option_key TEXT,
  requested_at TEXT NOT NULL,
  resolved_at TEXT,
  report_key TEXT,
  authorization_digest TEXT,
  settlement_report_key TEXT,
  settlement_outcome TEXT,
  CHECK (
    (state = 'resolved' AND response_option_key IS NOT NULL AND resolved_at IS NOT NULL)
    OR (state <> 'resolved' AND response_option_key IS NULL AND resolved_at IS NULL)
  )
);

CREATE UNIQUE INDEX idx_interactions_report_key ON interactions (session_id, report_key) WHERE report_key IS NOT NULL;
CREATE UNIQUE INDEX idx_interactions_settlement_key ON interactions(session_id, settlement_report_key) WHERE settlement_report_key IS NOT NULL;
CREATE INDEX idx_interactions_session_order ON interactions (session_id, requested_at, interaction_id);
CREATE INDEX idx_interactions_session_state_order ON interactions (session_id, state, requested_at, interaction_id);

CREATE TABLE session_runtimes (
  runtime_id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL REFERENCES sessions (session_id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('starting', 'running', 'exited')),
  start_command TEXT,
  tmux_socket_path TEXT,
  tmux_pane_id TEXT,
  process_fingerprint TEXT,
  created_at TEXT NOT NULL,
  CHECK (
    (tmux_socket_path IS NULL) = (tmux_pane_id IS NULL)
  )
);

CREATE INDEX idx_events_session_id ON events (session_id, created_at, event_id);

CREATE INDEX idx_events_type_timeline
    ON events(session_id, timeline_ms, event_id)
    WHERE event_type IN ('turn.failed', 'session.error', 'session.exited');

CREATE INDEX idx_events_turn_id ON events (turn_id, created_at, event_id)
WHERE
  turn_id IS NOT NULL;

CREATE INDEX idx_turns_session_id ON turns (session_id, created_at, turn_id);

CREATE INDEX idx_turns_active_session
    ON turns(session_id) WHERE state IN ('queued', 'running');

CREATE INDEX idx_workspaces_last_used ON workspaces (last_used_at, workspace_id);

CREATE INDEX idx_sessions_workspace ON sessions (workspace_id, state, updated_at, session_id);

CREATE INDEX idx_tasks_state_created ON tasks (state, created_at, task_id);

CREATE INDEX idx_tasks_workspace ON tasks (workspace_id, created_at, task_id);

CREATE INDEX idx_tasks_session ON tasks (session_id, created_at, task_id);

CREATE INDEX idx_task_events_task ON task_events (task_id, created_at, event_id);

CREATE INDEX idx_inbox_messages_session_state ON inbox_messages (
  session_id,
  state,
  delivery_policy,
  created_at,
  message_id
);

CREATE INDEX idx_inbox_messages_turn ON inbox_messages (turn_id)
WHERE
  turn_id IS NOT NULL;

CREATE UNIQUE INDEX idx_sessions_workspace_handle ON sessions (workspace_id, handle)
WHERE
  handle IS NOT NULL
  AND state NOT IN ('exited', 'error');

CREATE INDEX idx_agent_bindings_session ON agent_bindings (session_id, id);

CREATE INDEX idx_agent_bindings_identity ON agent_bindings (client_type, launch_cwd, client_session_key);

CREATE INDEX idx_session_lineage_parent ON session_lineage (parent_session_id, created_at, child_session_id);

CREATE INDEX idx_sessions_management_list ON sessions (archived_at, pinned_at, updated_at, session_id);

CREATE UNIQUE INDEX idx_agent_bindings_one_per_session ON agent_bindings (session_id);

CREATE UNIQUE INDEX idx_agent_bindings_unique_client_identity ON agent_bindings (client_type, client_session_key);

CREATE INDEX idx_inbox_messages_branch_target ON inbox_messages (branch_target_turn_id);

CREATE INDEX idx_workflow_nodes_workflow_parent ON workflow_nodes (workflow_id, parent_node_id, created_at, node_id);

CREATE INDEX idx_workflow_nodes_session ON workflow_nodes (session_id)
WHERE
  session_id IS NOT NULL;

CREATE INDEX idx_workflow_events_workflow_sequence ON workflow_events (workflow_id, sequence);

CREATE UNIQUE INDEX idx_inbox_messages_retry_of ON inbox_messages (retry_of_message_id)
WHERE
  retry_of_message_id IS NOT NULL;

CREATE UNIQUE INDEX idx_workflow_recovery_active ON workflow_recoveries (workflow_id)
WHERE
  state IN ('requested', 'preparing', 'dispatching');

CREATE INDEX idx_session_runtimes_session ON session_runtimes (session_id);

CREATE INDEX idx_session_runtimes_tmux ON session_runtimes (tmux_socket_path, tmux_pane_id);

CREATE INDEX idx_sessions_active_updated ON sessions (updated_at DESC, session_id DESC)
WHERE
  archived_at IS NULL
  AND state NOT IN ('exited', 'error');

CREATE INDEX idx_sessions_unarchived_updated ON sessions (updated_at DESC, session_id DESC)
WHERE
  archived_at IS NULL;

CREATE INDEX idx_sessions_workspace_unarchived_updated ON sessions (workspace_id, updated_at DESC, session_id DESC)
WHERE
  archived_at IS NULL;

CREATE UNIQUE INDEX idx_events_report_key ON events (session_id, event_type, report_key)
WHERE
  report_key IS NOT NULL;
CREATE TABLE domain_event_positions (
    position INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE
);

CREATE TRIGGER workflow_nodes_immutable_definition
BEFORE UPDATE ON workflow_nodes
WHEN OLD.workflow_id IS NOT NEW.workflow_id
  OR OLD.parent_node_id IS NOT NEW.parent_node_id
  OR OLD.node_type IS NOT NEW.node_type
  OR OLD.phase IS NOT NEW.phase
  OR OLD.title IS NOT NEW.title
  OR OLD.instructions IS NOT NEW.instructions
  OR OLD.inputs IS NOT NEW.inputs
  OR OLD.output IS NOT NEW.output
  OR OLD.execution_profile_id IS NOT NEW.execution_profile_id
BEGIN
    SELECT RAISE(ABORT, 'accepted workflow node definitions are immutable');
END;

CREATE TRIGGER events_assign_position AFTER INSERT ON events BEGIN
    INSERT INTO domain_event_positions(event_id) VALUES (NEW.event_id);
END;

CREATE UNIQUE INDEX idx_sessions_application ON sessions(application_id, application_ref);

CREATE TABLE workflow_agent_events (
    position INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    source TEXT NOT NULL,
    event_type TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_workflow_agent_events_session ON workflow_agent_events(session_id,position);

CREATE TABLE workflow_event_cursor (
    id INTEGER PRIMARY KEY CHECK (id=1),
    position INTEGER NOT NULL
);

CREATE TABLE workflow_session_snapshots (
    session_id TEXT PRIMARY KEY NOT NULL,
    client_type TEXT NOT NULL,
    state TEXT NOT NULL,
    current_turn_id TEXT,
    position INTEGER NOT NULL,
    has_agent_binding INTEGER NOT NULL
, revision INTEGER NOT NULL DEFAULT 0);

CREATE TABLE workflow_turn_snapshots (
    turn_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL -- queued, running, completed, failed, interrupted
);

CREATE INDEX idx_workflow_turn_snapshots_session ON workflow_turn_snapshots(session_id);

CREATE TABLE workflow_runtime_snapshots (
    runtime_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    state TEXT NOT NULL,
    is_control INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_workflow_runtime_snapshots_session ON workflow_runtime_snapshots(session_id);

CREATE TABLE workflow_inbox_snapshots (
    message_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL,
    turn_id TEXT
);

CREATE INDEX idx_workflow_inbox_snapshots_session ON workflow_inbox_snapshots(session_id);
CREATE TABLE control_state_revision (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL
);

CREATE TRIGGER sessions_revision_insert AFTER INSERT ON sessions BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER sessions_revision_update AFTER UPDATE ON sessions BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER sessions_revision_delete AFTER DELETE ON sessions BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER turns_revision_insert AFTER INSERT ON turns BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER turns_revision_update AFTER UPDATE ON turns BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER turns_revision_delete AFTER DELETE ON turns BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER session_runtimes_revision_insert AFTER INSERT ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER session_runtimes_revision_update AFTER UPDATE ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER session_runtimes_revision_delete AFTER DELETE ON session_runtimes BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER agent_bindings_revision_insert AFTER INSERT ON agent_bindings BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER agent_bindings_revision_update AFTER UPDATE ON agent_bindings BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER agent_bindings_revision_delete AFTER DELETE ON agent_bindings BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER inbox_messages_revision_insert AFTER INSERT ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER inbox_messages_revision_update AFTER UPDATE ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;

CREATE TRIGGER inbox_messages_revision_delete AFTER DELETE ON inbox_messages BEGIN
    UPDATE control_state_revision SET revision = revision + 1 WHERE id = 1;
END;
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

```
