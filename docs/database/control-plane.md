# Local control-plane SQLite database

```sql
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
  report_key TEXT
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
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  workspace_id TEXT REFERENCES workspaces (workspace_id),
  handle TEXT,
  role TEXT,
  description TEXT,
  execution_profile_id TEXT,
  execution_profile_version TEXT,
  title TEXT,
  pinned_at TEXT,
  archived_at TEXT
);

CREATE TABLE turns (
  turn_id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  state TEXT NOT NULL,
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

CREATE TABLE execution_profiles (
  profile_id TEXT NOT NULL,
  version TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  supported_client_types TEXT NOT NULL DEFAULT '[]',
  system_prompt_template TEXT,
  turn_prompt_template TEXT,
  default_session_role TEXT,
  default_session_description TEXT,
  handle_prefix TEXT,
  expected_output_schema TEXT,
  artifact_contract TEXT NOT NULL DEFAULT '{}',
  default_execution_policy TEXT NOT NULL DEFAULT '{}',
  default_review_policy TEXT NOT NULL DEFAULT '{}',
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
  archived_at TEXT,
  archived_reason TEXT,
  agent_kind TEXT NOT NULL DEFAULT 'executor',
  PRIMARY KEY (profile_id, version)
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
  completed_at TEXT,
  activating_node_id TEXT REFERENCES workflow_nodes (node_id),
  current_revision INTEGER NOT NULL DEFAULT 1 CHECK (current_revision >= 1),
  active_patch_id TEXT REFERENCES workflow_patches (patch_id),
  active_replanner_session_id TEXT REFERENCES sessions (session_id)
);

CREATE TABLE workflow_nodes (
  node_id TEXT PRIMARY KEY NOT NULL,
  workflow_id TEXT NOT NULL,
  parent_node_id TEXT,
  title TEXT NOT NULL,
  instructions TEXT NOT NULL,
  inputs TEXT NOT NULL DEFAULT '[]',
  output TEXT NOT NULL,
  execution_profile_id TEXT,
  execution_profile_version TEXT,
  session_id TEXT,
  submitted_at TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  node_type TEXT NOT NULL DEFAULT 'agent',
  phase TEXT NOT NULL DEFAULT '',
  submitted_runtime_id TEXT,
  exit_request_started_at TEXT,
  introduced_revision INTEGER NOT NULL DEFAULT 1 CHECK (introduced_revision >= 1),
  retired_revision INTEGER CHECK (
    retired_revision IS NULL
    OR retired_revision > introduced_revision
  ),
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

CREATE TABLE workflow_patches (
  patch_id TEXT PRIMARY KEY NOT NULL,
  workflow_id TEXT NOT NULL REFERENCES workflows (workflow_id),
  requesting_node_id TEXT NOT NULL REFERENCES workflow_nodes (node_id),
  requesting_session_id TEXT NOT NULL REFERENCES sessions (session_id),
  requesting_turn_id TEXT NOT NULL REFERENCES turns (turn_id),
  requesting_runtime_id TEXT NOT NULL,
  replanner_creation_token TEXT NOT NULL UNIQUE,
  replanner_session_id TEXT REFERENCES sessions (session_id),
  replanner_turn_id TEXT REFERENCES turns (turn_id),
  replanner_runtime_id TEXT,
  base_revision INTEGER NOT NULL CHECK (base_revision >= 1),
  result_revision INTEGER CHECK (
    result_revision IS NULL
    OR result_revision >= base_revision
  ),
  state TEXT NOT NULL CHECK (
    state IN (
      'requested',
      'planning',
      'applied',
      'rejected',
      'blocked'
    )
  ),
  request_document_ref TEXT NOT NULL,
  request_size_bytes INTEGER NOT NULL CHECK (request_size_bytes >= 0),
  decision_document_ref TEXT,
  reason_document_ref TEXT,
  blocked_draft_ref TEXT,
  interruption_attempted_at TEXT,
  interruption_requested_at TEXT,
  replanning_unlocked_at TEXT,
  continuation_message_id TEXT,
  continuation_queued_at TEXT,
  requested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  planning_at TEXT,
  resolved_at TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  replanner_exit_requested_at TEXT
);

CREATE TABLE native_turn_bindings (
  session_id TEXT NOT NULL REFERENCES sessions (session_id) ON DELETE CASCADE,
  client_turn_id TEXT NOT NULL,
  turn_id TEXT NOT NULL UNIQUE,
  PRIMARY KEY (session_id, client_turn_id)
);

CREATE TABLE workflow_recoveries (
  recovery_id TEXT PRIMARY KEY NOT NULL,
  workflow_id TEXT NOT NULL REFERENCES workflows (workflow_id),
  failure_event_id TEXT NOT NULL UNIQUE REFERENCES workflow_events (event_id),
  exit_event_id TEXT NOT NULL REFERENCES events (event_id),
  node_id TEXT NOT NULL REFERENCES workflow_nodes (node_id),
  session_id TEXT NOT NULL REFERENCES sessions (session_id),
  message_id TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL CHECK (
    state IN (
      'requested',
      'preparing',
      'dispatching',
      'completed',
      'failed'
    )
  ),
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
  options TEXT NOT NULL CHECK (json_valid(options)),
  state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'expired')),
  response_option_key TEXT,
  requested_at TEXT NOT NULL,
  resolved_at TEXT,
  CHECK (
    (state = 'resolved' AND response_option_key IS NOT NULL AND resolved_at IS NOT NULL)
    OR (state <> 'resolved' AND response_option_key IS NULL AND resolved_at IS NULL)
  )
);

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

CREATE INDEX idx_events_turn_id ON events (turn_id, created_at, event_id)
WHERE
  turn_id IS NOT NULL;

CREATE INDEX idx_turns_session_id ON turns (session_id, created_at, turn_id);

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

CREATE INDEX idx_execution_profiles_profile_created ON execution_profiles (profile_id, created_at, version);

CREATE INDEX idx_sessions_execution_profile ON sessions (
  workspace_id,
  execution_profile_id,
  execution_profile_version,
  state,
  updated_at,
  session_id
);

CREATE UNIQUE INDEX idx_sessions_workspace_handle ON sessions (workspace_id, handle)
WHERE
  handle IS NOT NULL
  AND state NOT IN ('exited', 'error');

CREATE INDEX idx_execution_profiles_active_latest ON execution_profiles (
  profile_id,
  active,
  archived_at,
  created_at,
  version
);

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

CREATE INDEX idx_workflow_nodes_workflow_revision ON workflow_nodes (
  workflow_id,
  introduced_revision,
  retired_revision
);

CREATE UNIQUE INDEX idx_workflow_patches_one_active ON workflow_patches (workflow_id)
WHERE
  state IN ('requested', 'planning');

CREATE UNIQUE INDEX idx_workflow_patches_replanner_session ON workflow_patches (replanner_session_id)
WHERE
  replanner_session_id IS NOT NULL;

CREATE INDEX idx_workflow_patches_workflow_requested ON workflow_patches (workflow_id, requested_at, patch_id);

CREATE UNIQUE INDEX idx_sessions_workflow_replanner_creation_token ON sessions (
  json_extract(metadata, '$.workflow_replanner_creation_token')
)
WHERE
  json_extract(metadata, '$.workflow_replanner_creation_token') IS NOT NULL;

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

CREATE TRIGGER turns_validate_topology_on_insert
BEFORE INSERT ON turns
WHEN NOT (
    (NEW.topology_status IN ('unknown', 'root') AND NEW.parent_turn_id IS NULL)
    OR (NEW.topology_status = 'linked' AND NEW.parent_turn_id IS NOT NULL AND trim(NEW.parent_turn_id) <> '')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology status/parent combination');
END;

CREATE TRIGGER turns_validate_topology_on_update
BEFORE UPDATE OF topology_status, parent_turn_id ON turns
WHEN NOT (
    (NEW.topology_status IN ('unknown', 'root') AND NEW.parent_turn_id IS NULL)
    OR (NEW.topology_status = 'linked' AND NEW.parent_turn_id IS NOT NULL AND trim(NEW.parent_turn_id) <> '')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology status/parent combination');
END;

CREATE TRIGGER turns_preserve_resolved_topology
BEFORE UPDATE OF topology_status, parent_turn_id ON turns
WHEN OLD.topology_status <> 'unknown'
 AND (NEW.topology_status IS NOT OLD.topology_status OR NEW.parent_turn_id IS NOT OLD.parent_turn_id)
BEGIN
    SELECT RAISE(ABORT, 'resolved Turn topology is immutable');
END;

CREATE TRIGGER events_preserve_turn_topology
BEFORE UPDATE OF turn_topology ON events
WHEN NEW.turn_topology IS NOT OLD.turn_topology
BEGIN
    SELECT RAISE(ABORT, 'event Turn topology enrichment is immutable');
END;

CREATE TRIGGER turns_preserve_turn_identity
BEFORE UPDATE OF session_id ON turns
WHEN NEW.session_id IS NOT OLD.session_id
BEGIN
    SELECT RAISE(ABORT, 'turn session_id is immutable');
END;

CREATE TRIGGER turn_events_require_turn_identity
BEFORE INSERT ON events
WHEN NEW.event_type LIKE 'turn.%'
 AND NEW.turn_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'turn event turn_id is required');
END;

CREATE TRIGGER turns_validate_linked_parent_on_insert
BEFORE INSERT ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_id < NEW.turn_id
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turns_validate_linked_parent_on_update
BEFORE UPDATE OF topology_status, parent_turn_id, session_id ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_id < NEW.turn_id
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turn_events_validate_linked_parent
BEFORE INSERT ON events
WHEN NEW.turn_topology IS NOT NULL
 AND json_valid(NEW.turn_topology)
 AND json_extract(NEW.turn_topology, '$.status') = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = json_extract(NEW.turn_topology, '$.parent_turn_id')
      AND parent.session_id = NEW.session_id
      AND parent.turn_id < NEW.turn_id
 )
BEGIN
    SELECT RAISE(ABORT, 'linked event parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER workflows_revision_monotonic
BEFORE UPDATE OF current_revision ON workflows
WHEN NEW.current_revision != OLD.current_revision + 1
BEGIN
    SELECT RAISE(ABORT, 'workflow revision must advance by one');
END;

CREATE TRIGGER workflow_nodes_validate_introduction
BEFORE INSERT ON workflow_nodes
WHEN NOT EXISTS (
    SELECT 1
    FROM workflows
    WHERE workflows.workflow_id = NEW.workflow_id
      AND NEW.introduced_revision >= workflows.current_revision
      AND NEW.introduced_revision <= workflows.current_revision + 1
)
BEGIN
    SELECT RAISE(ABORT, 'workflow node introduction must belong to the current or next revision');
END;

CREATE TRIGGER workflow_nodes_validate_parent_insert
BEFORE INSERT ON workflow_nodes
WHEN NEW.parent_node_id IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM workflow_nodes AS parent
     WHERE parent.node_id = NEW.parent_node_id
       AND parent.workflow_id = NEW.workflow_id
       AND parent.introduced_revision <= NEW.introduced_revision
       AND (parent.retired_revision IS NULL
            OR parent.retired_revision > NEW.introduced_revision)
 )
BEGIN
    SELECT RAISE(ABORT, 'workflow node parent must be active in the same workflow revision');
END;

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
  OR OLD.execution_profile_version IS NOT NEW.execution_profile_version
  OR OLD.introduced_revision IS NOT NEW.introduced_revision
BEGIN
    SELECT RAISE(ABORT, 'accepted workflow node definitions are immutable');
END;

CREATE TRIGGER workflow_nodes_immutable_retirement
BEFORE UPDATE OF retired_revision ON workflow_nodes
WHEN OLD.retired_revision IS NOT NULL
  OR NEW.retired_revision IS NULL
BEGIN
    SELECT RAISE(ABORT, 'workflow node retirement is immutable');
END;

CREATE TRIGGER workflow_nodes_validate_retirement
BEFORE UPDATE OF retired_revision ON workflow_nodes
WHEN NEW.retired_revision IS NOT NULL
 AND NOT EXISTS (
     SELECT 1
     FROM workflows
     WHERE workflows.workflow_id = OLD.workflow_id
       AND NEW.retired_revision = workflows.current_revision + 1
 )
BEGIN
    SELECT RAISE(ABORT, 'workflow node retirement must belong to the next revision');
END;

CREATE TRIGGER workflow_nodes_history_prevents_delete
BEFORE DELETE ON workflow_nodes
BEGIN
    SELECT RAISE(ABORT, 'workflow node history cannot be deleted');
END;

CREATE TRIGGER turn_events_validate_topology
BEFORE INSERT ON events
WHEN (NEW.turn_topology IS NOT NULL AND (
    NEW.event_type NOT IN ('turn.started', 'turn.topology_recovered')
    OR NOT json_valid(NEW.turn_topology)
    OR COALESCE(json_extract(NEW.turn_topology, '$.status'), '') NOT IN ('unknown', 'root', 'linked')
    OR (json_extract(NEW.turn_topology, '$.status') IN ('unknown', 'root')
        AND json_type(NEW.turn_topology, '$.parent_turn_id') IS NOT NULL)
    OR (json_extract(NEW.turn_topology, '$.status') = 'linked'
        AND COALESCE(trim(json_extract(NEW.turn_topology, '$.parent_turn_id')), '') = '')
)) OR (NEW.event_type = 'turn.topology_recovered' AND (
    NEW.source <> 'system_monitor'
    OR NEW.turn_topology IS NULL
    OR COALESCE(json_extract(NEW.turn_topology, '$.status'), '') NOT IN ('root', 'linked')
    OR NOT EXISTS (SELECT 1 FROM turns WHERE turn_id = NEW.turn_id AND session_id = NEW.session_id)
))
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology enrichment');
END;
```
