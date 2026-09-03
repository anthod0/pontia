# Current SQLite database tables

## `events`

| Column | Type | Constraints / default |
|---|---|---|
| `event_id` | TEXT | primary key, NOT NULL |
| `session_id` | TEXT | NOT NULL |
| `turn_id` | TEXT | |
| `source` | TEXT | NOT NULL |
| `client_type` | TEXT | NOT NULL |
| `event_type` | TEXT | NOT NULL |
| `occurred_at` | TEXT | NOT NULL |
| `payload` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `timeline_boundary` | TEXT | |
| `turn_topology` | TEXT | |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_events_session_id` | No | `session_id`, `created_at`, `event_id` |  |
| `idx_events_turn_id` | No | `turn_id`, `created_at`, `event_id` | `turn_id IS NOT NULL` |

**Triggers**

| Name | Event | Rule |
|---|---|---|
| `events_preserve_turn_topology` | BEFORE UPDATE OF `turn_topology` | `turn_topology` is immutable. |
| `turn_events_require_turn_identity` | BEFORE INSERT | `turn.*` events require a non-null `turn_id`. |
| `turn_events_validate_linked_parent` | BEFORE INSERT | A linked parent must be an earlier Turn in the same Session. |
| `turn_events_validate_topology` | BEFORE INSERT | `turn_topology` is allowed only on `turn.started`; it must be valid JSON with a valid status and matching parent shape. |

## `sessions`

| Column | Type | Constraints / default |
|---|---|---|
| `session_id` | TEXT | primary key, NOT NULL |
| `client_type` | TEXT | NOT NULL |
| `workspace_ref` | TEXT | |
| `state` | TEXT | NOT NULL |
| `current_turn_id` | TEXT | |
| `state_version` | INTEGER | NOT NULL, default `0` |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `workspace_id` | TEXT | foreign key → `workspaces.workspace_id` |
| `handle` | TEXT | |
| `role` | TEXT | |
| `description` | TEXT | |
| `execution_profile_id` | TEXT | |
| `execution_profile_version` | TEXT | |
| `title` | TEXT | |
| `pinned_at` | TEXT | |
| `archived_at` | TEXT | |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_sessions_execution_profile` | No | `workspace_id`, `execution_profile_id`, `execution_profile_version`, `state`, `updated_at`, `session_id` |  |
| `idx_sessions_management_list` | No | `archived_at`, `pinned_at`, `updated_at`, `session_id` |  |
| `idx_sessions_workflow_replanner_creation_token` | Yes | `json_extract(metadata, '$.workflow_replanner_creation_token')` | token is not NULL |
| `idx_sessions_workspace` | No | `workspace_id`, `state`, `updated_at`, `session_id` |  |
| `idx_sessions_workspace_handle` | Yes | `workspace_id`, `handle` | `handle IS NOT NULL AND state NOT IN ('exited', 'error')` |

## `turns`

| Column | Type | Constraints / default |
|---|---|---|
| `turn_id` | TEXT | primary key, NOT NULL |
| `session_id` | TEXT | NOT NULL, foreign key → `sessions.session_id` |
| `state` | TEXT | NOT NULL |
| `state_version` | INTEGER | NOT NULL, default `0` |
| `input_summary` | TEXT | |
| `output_summary` | TEXT | |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `head_cursor` | TEXT | |
| `tail_cursor` | TEXT | |
| `parent_turn_id` | TEXT | |
| `topology_status` | TEXT | NOT NULL, default `'unknown'` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_turns_session_id` | No | `session_id`, `created_at`, `turn_id` |  |

**Triggers**

| Name | Event | Rule |
|---|---|---|
| `turns_preserve_resolved_topology` | BEFORE UPDATE OF `topology_status`, `parent_turn_id` | Topology is immutable after `topology_status` leaves `unknown`. |
| `turns_preserve_turn_identity` | BEFORE UPDATE OF `session_id` | `session_id` is immutable. |
| `turns_validate_linked_parent_on_insert` | BEFORE INSERT | A linked parent must be an earlier Turn in the same Session. |
| `turns_validate_linked_parent_on_update` | BEFORE UPDATE OF `topology_status`, `parent_turn_id`, `session_id` | A linked parent must be an earlier Turn in the same Session. |
| `turns_validate_topology_on_insert` | BEFORE INSERT | `unknown` and `root` require no parent; `linked` requires a non-empty parent. |
| `turns_validate_topology_on_update` | BEFORE UPDATE OF `topology_status`, `parent_turn_id` | `unknown` and `root` require no parent; `linked` requires a non-empty parent. |

## `workspaces`

| Column | Type | Constraints / default |
|---|---|---|
| `workspace_id` | TEXT | primary key, NOT NULL |
| `canonical_path` | TEXT | NOT NULL, unique constraint |
| `display_path` | TEXT | NOT NULL |
| `name` | TEXT | |
| `state` | TEXT | NOT NULL, default `'active'` |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `last_used_at` | TEXT | |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_workspaces_last_used` | No | `last_used_at`, `workspace_id` |  |

## `workspace_git_status`

| Column | Type | Constraints / default |
|---|---|---|
| `workspace_id` | TEXT | primary key, NOT NULL, foreign key → `workspaces.workspace_id` ON DELETE CASCADE |
| `repo_root` | TEXT | |
| `branch` | TEXT | |
| `upstream` | TEXT | |
| `ahead` | INTEGER | NOT NULL, default `0` |
| `behind` | INTEGER | NOT NULL, default `0` |
| `staged_count` | INTEGER | NOT NULL, default `0` |
| `unstaged_count` | INTEGER | NOT NULL, default `0` |
| `untracked_count` | INTEGER | NOT NULL, default `0` |
| `conflicted_count` | INTEGER | NOT NULL, default `0` |
| `clean` | INTEGER | NOT NULL, default `1` |
| `state` | TEXT | NOT NULL |
| `failure` | TEXT | |
| `observed_at` | TEXT | NOT NULL |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

## `tasks`

| Column | Type | Constraints / default |
|---|---|---|
| `task_id` | TEXT | primary key, NOT NULL |
| `state` | TEXT | NOT NULL |
| `input` | TEXT | NOT NULL |
| `workspace_id` | TEXT | foreign key → `workspaces.workspace_id` |
| `session_id` | TEXT | foreign key → `sessions.session_id` |
| `turn_id` | TEXT | foreign key → `turns.turn_id` |
| `routing_state` | TEXT | NOT NULL, default `'pending'` |
| `routing_reason` | TEXT | |
| `routing_confidence` | REAL | |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_tasks_session` | No | `session_id`, `created_at`, `task_id` |  |
| `idx_tasks_state_created` | No | `state`, `created_at`, `task_id` |  |
| `idx_tasks_workspace` | No | `workspace_id`, `created_at`, `task_id` |  |

## `task_events`

| Column | Type | Constraints / default |
|---|---|---|
| `event_id` | TEXT | primary key, NOT NULL |
| `task_id` | TEXT | NOT NULL, foreign key → `tasks.task_id` |
| `event_type` | TEXT | NOT NULL |
| `payload` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_task_events_task` | No | `task_id`, `created_at`, `event_id` |  |

## `inbox_messages`

| Column | Type | Constraints / default |
|---|---|---|
| `message_id` | TEXT | primary key, NOT NULL |
| `session_id` | TEXT | NOT NULL, foreign key → `sessions.session_id` |
| `state` | TEXT | NOT NULL |
| `delivery_policy` | TEXT | NOT NULL |
| `input_summary` | TEXT | NOT NULL |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `turn_id` | TEXT | foreign key → `turns.turn_id` |
| `superseded_by_message_id` | TEXT | |
| `failure_message` | TEXT | |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `dispatched_at` | TEXT | |
| `cancelled_at` | TEXT | |
| `branch_target_turn_id` | TEXT | foreign key → `turns.turn_id` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_inbox_messages_branch_target` | No | `branch_target_turn_id` |  |
| `idx_inbox_messages_session_state` | No | `session_id`, `state`, `delivery_policy`, `created_at`, `message_id` |  |
| `idx_inbox_messages_turn` | No | `turn_id` | `turn_id IS NOT NULL` |

## `execution_profiles`

| Column | Type | Constraints / default |
|---|---|---|
| `profile_id` | TEXT | composite primary key, NOT NULL |
| `version` | TEXT | composite primary key, NOT NULL |
| `name` | TEXT | NOT NULL |
| `description` | TEXT | |
| `supported_client_types` | TEXT | NOT NULL, default `'[]'` |
| `system_prompt_template` | TEXT | |
| `turn_prompt_template` | TEXT | |
| `default_session_role` | TEXT | |
| `default_session_description` | TEXT | |
| `handle_prefix` | TEXT | |
| `expected_output_schema` | TEXT | |
| `artifact_contract` | TEXT | NOT NULL, default `'{}'` |
| `default_execution_policy` | TEXT | NOT NULL, default `'{}'` |
| `default_review_policy` | TEXT | NOT NULL, default `'{}'` |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `active` | INTEGER | NOT NULL, default `1`, CHECK `active IN (0, 1)` |
| `archived_at` | TEXT | |
| `archived_reason` | TEXT | |
| `agent_kind` | TEXT | NOT NULL, default `'executor'` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_execution_profiles_active_latest` | No | `profile_id`, `active`, `archived_at`, `created_at`, `version` |  |
| `idx_execution_profiles_profile_created` | No | `profile_id`, `created_at`, `version` |  |

## `agent_bindings`

| Column | Type | Constraints / default |
|---|---|---|
| `id` | TEXT | primary key, NOT NULL |
| `session_id` | TEXT | NOT NULL, foreign key → `sessions.session_id` |
| `client_type` | TEXT | NOT NULL |
| `launch_cwd` | TEXT | NOT NULL |
| `client_session_key` | TEXT | NOT NULL |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `discovered` | BOOLEAN | NOT NULL, default `FALSE` |
| `client_session_file` | TEXT | |

Table constraint: unique constraint `UNIQUE(session_id, client_type, client_session_key)`.

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_agent_bindings_identity` | No | `client_type`, `launch_cwd`, `client_session_key` |  |
| `idx_agent_bindings_one_per_session` | Yes | `session_id` |  |
| `idx_agent_bindings_session` | No | `session_id`, `id` |  |
| `idx_agent_bindings_unique_client_identity` | Yes | `client_type`, `client_session_key` |  |

## `runtime_bindings`

| Column | Type | Constraints / default |
|---|---|---|
| `session_id` | TEXT | primary key, NOT NULL, foreign key → `sessions.session_id` ON DELETE CASCADE |
| `runtime_kind` | TEXT | NOT NULL |
| `runtime_instance_id` | TEXT | |
| `binding_state` | TEXT | NOT NULL, default `'provisioned'`, CHECK `binding_state IN ('provisioned', 'confirmed')` |
| `runtime_handle` | TEXT | |
| `start_command` | TEXT | |
| `launch_cwd` | TEXT | |
| `internal_event_url` | TEXT | |
| `started_at` | TEXT | |
| `last_seen_at` | TEXT | |
| `restart_count` | INTEGER | NOT NULL, default `0` |
| `tmux_socket_path` | TEXT | |
| `tmux_pane_id` | TEXT | |
| `process_fingerprint` | TEXT | CHECK `process_fingerprint IS NULL OR json_valid(process_fingerprint)` |
| `capabilities` | TEXT | NOT NULL, default `'{}'`, CHECK `json_valid(capabilities)` |
| `diagnostics` | TEXT | NOT NULL, default `'{}'`, CHECK `json_valid(diagnostics)`; migration 0020 removes the obsolete `claude_hook_log` key from existing rows |
| `adapter_details` | TEXT | NOT NULL, default `'{}'`, CHECK `json_valid(adapter_details)` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_runtime_bindings_tmux_unconfirmed` | No | `tmux_socket_path`, `tmux_pane_id`, `binding_state` |  |

## `pending_turn_contexts`

| Column | Type | Constraints / default |
|---|---|---|
| `session_id` | TEXT | primary key, NOT NULL, foreign key → `runtime_bindings.session_id` ON DELETE CASCADE |
| `runtime_instance_id` | TEXT | NOT NULL |
| `client_type` | TEXT | NOT NULL |
| `payload` | TEXT | NOT NULL, CHECK `json_valid(payload)` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

## `session_lineage`

| Column | Type | Constraints / default |
|---|---|---|
| `child_session_id` | TEXT | primary key, NOT NULL, foreign key → `sessions.session_id` ON DELETE CASCADE |
| `parent_session_id` | TEXT | NOT NULL, foreign key → `sessions.session_id` ON DELETE CASCADE |
| `relation_type` | TEXT | NOT NULL, CHECK `relation_type IN ('fork')` |
| `forked_from_turn_id` | TEXT | |
| `forked_from_client_node_id` | TEXT | |
| `parent_client_session_key` | TEXT | |
| `child_client_session_key` | TEXT | |
| `metadata` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_session_lineage_parent` | No | `parent_session_id`, `created_at`, `child_session_id` |  |

## `workflows`

| Column | Type | Constraints / default |
|---|---|---|
| `workflow_id` | TEXT | primary key, NOT NULL |
| `title` | TEXT | NOT NULL |
| `cwd` | TEXT | NOT NULL |
| `state` | TEXT | NOT NULL |
| `current_revision` | INTEGER | NOT NULL, default `1`, check `current_revision >= 1` |
| `failure_message` | TEXT | |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `started_at` | TEXT | |
| `completed_at` | TEXT | |
| `activating_node_id` | TEXT | foreign key → `workflow_nodes.node_id`; transient database gate preventing pause from racing node dispatch |
| `active_patch_id` | TEXT | foreign key → `workflow_patches.patch_id` |
| `active_replanner_session_id` | TEXT | foreign key → `sessions.session_id` |

## `workflow_nodes`

| Column | Type | Constraints / default |
|---|---|---|
| `node_id` | TEXT | primary key, NOT NULL |
| `workflow_id` | TEXT | NOT NULL, foreign key → `workflows.workflow_id` |
| `parent_node_id` | TEXT | foreign key → `workflow_nodes.node_id` |
| `title` | TEXT | NOT NULL |
| `instructions` | TEXT | NOT NULL |
| `inputs` | TEXT | NOT NULL, default `'[]'` |
| `output` | TEXT | NOT NULL |
| `execution_profile_id` | TEXT | |
| `execution_profile_version` | TEXT | |
| `introduced_revision` | INTEGER | NOT NULL, default `1`, check `introduced_revision >= 1` |
| `retired_revision` | INTEGER | check `retired_revision IS NULL OR retired_revision > introduced_revision` |
| `session_id` | TEXT | |
| `submitted_at` | TEXT | |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `node_type` | TEXT | NOT NULL, default `'agent'` |
| `phase` | TEXT | NOT NULL, default `''` |
| `submitted_runtime_instance_id` | TEXT | |
| `exit_request_started_at` | TEXT | |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_workflow_nodes_session` | No | `session_id` | `session_id IS NOT NULL` |
| `idx_workflow_nodes_workflow_parent` | No | `workflow_id`, `parent_node_id`, `created_at`, `node_id` |  |
| `idx_workflow_nodes_workflow_revision` | No | `workflow_id`, `introduced_revision`, `retired_revision` |  |

**Triggers**

- `workflows_revision_monotonic` requires the current revision to advance exactly one revision at a time.
- `workflow_nodes_validate_introduction` restricts new Node membership to the current or immediately next revision.
- `workflow_nodes_validate_parent_insert` requires a parent Node to be active in the same Workflow at the child's introduction revision.
- `workflow_nodes_immutable_definition` prevents changes to an accepted Node's Workflow, parent, type, phase, definition, profile, or introduction revision.
- `workflow_nodes_immutable_retirement` permits retirement to be set once and prevents clearing or rewriting it.
- `workflow_nodes_validate_retirement` restricts first retirement to the immediately next Workflow revision.
- `workflow_nodes_history_prevents_delete` prevents accepted Node history from being physically deleted.

## `workflow_patches`

| Column | Type | Constraints / default |
|---|---|---|
| `patch_id` | TEXT | primary key, NOT NULL |
| `workflow_id` | TEXT | NOT NULL, foreign key → `workflows.workflow_id` |
| `requesting_node_id` | TEXT | NOT NULL, foreign key → `workflow_nodes.node_id` |
| `requesting_session_id` | TEXT | NOT NULL, foreign key → `sessions.session_id` |
| `requesting_turn_id` | TEXT | NOT NULL, foreign key → `turns.turn_id` |
| `requesting_runtime_instance_id` | TEXT | NOT NULL |
| `replanner_creation_token` | TEXT | NOT NULL, unique |
| `replanner_session_id` | TEXT | foreign key → `sessions.session_id` |
| `replanner_turn_id` | TEXT | foreign key → `turns.turn_id` |
| `replanner_runtime_instance_id` | TEXT | |
| `base_revision` | INTEGER | NOT NULL, check `base_revision >= 1` |
| `result_revision` | INTEGER | check `result_revision IS NULL OR result_revision >= base_revision` |
| `state` | TEXT | NOT NULL, one of `requested`, `planning`, `applied`, `rejected`, `blocked` |
| `request_document_ref` | TEXT | NOT NULL |
| `request_size_bytes` | INTEGER | NOT NULL, check `request_size_bytes >= 0` |
| `decision_document_ref` | TEXT | |
| `reason_document_ref` | TEXT | |
| `blocked_draft_ref` | TEXT | |
| `interruption_attempted_at` | TEXT | |
| `interruption_requested_at` | TEXT | |
| `replanning_unlocked_at` | TEXT | set only after the recorded requester Turn interruption is confirmed by a persisted Agent Client fact |
| `continuation_message_id` | TEXT | |
| `continuation_queued_at` | TEXT | |
| `replanner_exit_requested_at` | TEXT | durable claim for graceful exit after a Re-planner Turn terminal fact |
| `requested_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `planning_at` | TEXT | |
| `resolved_at` | TEXT | |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
| `updated_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_workflow_patches_one_active` | Yes | `workflow_id` | `state IN ('requested', 'planning')` |
| `idx_workflow_patches_replanner_session` | Yes | `replanner_session_id` | `replanner_session_id IS NOT NULL` |
| `idx_workflow_patches_workflow_requested` | No | `workflow_id`, `requested_at`, `patch_id` |  |

## `workflow_events`

| Column | Type | Constraints / default |
|---|---|---|
| `event_id` | TEXT | primary key, NOT NULL |
| `workflow_id` | TEXT | NOT NULL, foreign key → `workflows.workflow_id` |
| `sequence` | INTEGER | NOT NULL |
| `event_type` | TEXT | NOT NULL |
| `payload` | TEXT | NOT NULL, default `'{}'` |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |

Table constraint: unique constraint `UNIQUE(workflow_id, sequence)`.

**Indexes**

| Name | Unique | Columns | Condition |
|---|---|---|---|
| `idx_workflow_events_workflow_sequence` | No | `workflow_id`, `sequence` |  |
