export type JsonObject = Record<string, unknown>;

export type SessionState = 'created' | 'starting' | 'idle' | 'busy' | 'interrupted' | 'exited' | 'error';
export type TaskState = 'created' | 'routing' | 'needs_confirmation' | 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
export type TurnState = 'queued' | 'running' | 'completed' | 'failed' | 'interrupted' | 'abandoned';
export type TurnTopologyStatus = 'unknown' | 'root' | 'linked';
export type InboxDeliveryPolicy = 'after_idle' | 'interrupt_now' | 'steer';
export type InboxMessageState = 'resuming' | 'unknown' | 'pending' | 'dispatching' | 'dispatched' | 'cancelled' | 'superseded' | 'failed' | 'dismissed';
export type WorkflowState = 'pending' | 'running' | 'paused' | 'replanning' | 'blocked' | 'idle' | 'completed' | 'failed';
export type WorkflowAgentStatus = 'pending' | 'starting' | 'running' | 'paused' | 'idle' | 'exiting' | 'submitted' | 'failed' | 'unknown';

export interface WorkflowListItemView {
  workflow_id: string;
  title: string;
  state: WorkflowState;
  current_revision: number;
  failure_message: string | null;
  agent_submitted_count: number;
  agent_total_count: number;
  current_phase_name: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
  elapsed_ms: number;
  observation_error: string | null;
}

export interface WorkflowGraphRevisionView {
  workflow_id: string;
  revision: number;
  current: boolean;
  nodes: WorkflowGraphNodeView[];
}

export interface WorkflowGraphNodeView {
  node_id: string;
  parent_node_id: string | null;
  node_type: string;
  session_id: string | null;
  turn_ids: string[];
  phase: string;
  title: string;
  instructions: string;
  inputs: string[];
  output: string;
  execution_profile_id: string | null;
  execution_profile_version: string | null;
  introduced_revision: number;
  retired_revision: number | null;
}

export interface WorkflowNodeView {
  node_id: string;
  phase: string;
  title: string;
  status: WorkflowAgentStatus;
  session_id: string | null;
  session_state: string | null;
  submitted_at: string | null;
}

export interface WorkflowActivePatchView {
  patch_id: string;
  state: string;
  base_revision: number;
  request_document_ref: string;
  requesting_node_id: string;
  requesting_session_id: string;
  requesting_turn_id: string;
  replanner_session_id: string | null;
  replanner_turn_id: string | null;
}

export interface WorkflowPatchHistoryView extends WorkflowActivePatchView {
  outcome: string | null;
  result_revision: number | null;
  requesting_runtime_instance_id: string;
  replanner_runtime_instance_id: string | null;
  added_node_ids: string[];
  retired_node_ids: string[];
  decision_document_ref: string | null;
  reason_document_ref: string | null;
  blocked_draft_ref: string | null;
  requested_at: string;
  planning_at: string | null;
  resolved_at: string | null;
}

export interface WorkflowDocumentView {
  workflow_id: string;
  document_ref: string;
  content: string;
}

export interface WorkflowDetailView {
  workflow_id: string;
  title: string;
  state: WorkflowState;
  current_revision: number;
  failure_message: string | null;
  cwd: string;
  active_patch: WorkflowActivePatchView | null;
  agent_submitted_count: number;
  agent_total_count: number;
  current_node_id: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
  elapsed_ms: number;
  nodes: WorkflowNodeView[];
}

export type ContextUsageCapability = 'unsupported' | 'estimated' | 'exact';

export interface SessionCapabilities {
  accept_task?: boolean;
  interrupt?: boolean;
  stream_output?: boolean;
  heartbeat?: boolean;
  timeline?: boolean;
  topology?: boolean;
  branch_control?: boolean;
  list_models?: boolean;
  set_model?: boolean;
  context_usage?: ContextUsageCapability;
  [key: string]: unknown;
}

export interface ContextUsageView {
  used_tokens: number | null;
  max_tokens: number | null;
  remaining_tokens: number | null;
  usage_ratio: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cache_tokens: number | null;
  confidence: 'exact' | 'estimated' | 'unknown';
  observed_at: string;
}

export type AgentKind = 'executor';

export interface AgentProfileView {
  profile_id: string;
  version: string;
  name: string;
  description: string | null;
  supported_client_types: string[];
  agent_kind: AgentKind;
  system_prompt_template: string | null;
  turn_prompt_template: string | null;
  default_session_role: string | null;
  default_session_description: string | null;
  handle_prefix: string | null;
  expected_output_schema: string | null;
  artifact_contract: JsonObject;
  default_execution_policy: JsonObject;
  default_review_policy: JsonObject;
  metadata: JsonObject;
  active: boolean;
  archived_at: string | null;
  archived_reason: string | null;
  created_at: string;
  updated_at: string;
}

export interface UpsertAgentProfileInput {
  profile_id: string;
  version: string;
  name: string;
  description?: string | null;
  supported_client_types?: string[];
  agent_kind: AgentKind;
  system_prompt_template?: string | null;
  turn_prompt_template?: string | null;
  default_session_role?: string | null;
  default_session_description?: string | null;
  handle_prefix?: string | null;
  expected_output_schema?: string | null;
  artifact_contract?: JsonObject;
  default_execution_policy?: JsonObject;
  default_review_policy?: JsonObject;
  metadata?: JsonObject;
}

export interface SessionLineageView {
  relation_type: 'fork' | string;
  parent_session_id: string;
  forked_from_turn_id: string | null;
  forked_from_client_node_id: string | null;
  parent_client_session_key: string | null;
  child_client_session_key: string | null;
  created_at: string;
}

export interface CodexTuiView {
  owner_session_id: string;
  target_session_id: string;
  connected: boolean;
  socket_path: string | null;
  pane_id: string | null;
}

export interface SessionView {
  codex?: {
    connection: 'awaiting_input' | 'available' | 'reconciling' | 'unavailable' | 'archived';
    thread_id?: string;
    tui?: CodexTuiView;
    owned_tui?: CodexTuiView;
  };
  session_id: string;
  client_type: string;
  title: string | null;
  handle: string | null;
  role: string | null;
  description: string | null;
  execution_profile_id: string | null;
  execution_profile_version: string | null;
  state: SessionState | string;
  current_turn_id: string | null;
  workspace_id: string | null;
  workspace: string | null;
  pinned_at: string | null;
  archived_at: string | null;
  capabilities: SessionCapabilities;
  timeline_unavailable_reason?: string | null;
  model: string | null;
  model_control_unavailable_reason?: string | null;
  context_usage: ContextUsageView | null;
  lineage: SessionLineageView | null;
  created_at: string;
  updated_at: string;
  metadata: JsonObject;
}

export interface WorkspaceView {
  workspace_id: string;
  canonical_path: string;
  display_path: string;
  name: string | null;
  state: string;
  metadata: JsonObject;
  created_at: string;
  updated_at: string;
  last_used_at: string | null;
}

export interface WorkspaceGitStatusView {
  workspace_id: string;
  repo_root: string | null;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  staged_count: number;
  unstaged_count: number;
  untracked_count: number;
  conflicted_count: number;
  clean: boolean;
  state: 'unknown' | 'observed' | 'error' | string;
  failure: string | null;
  observed_at: string | null;
  updated_at: string | null;
}

export interface WorkspaceRootView {
  root_id: string;
  label: string;
  canonical_path: string | null;
  state: string;
}

export interface WorkspaceDirectoryEntryView {
  name: string;
  path: string;
  kind: 'directory' | string;
  is_workspace: boolean;
}

export interface WorkspaceDirectoryListingView {
  root_id: string;
  path: string;
  canonical_path: string;
  parent_path: string | null;
  entries: WorkspaceDirectoryEntryView[];
  warnings: string[];
}

export interface FilePickerFileView {
  path: string;
  name: string;
  kind: 'directory' | 'file' | string;
}

export interface FilePickerResultView {
  files: FilePickerFileView[];
  truncated: boolean;
  warnings: string[];
}

export interface RegisterWorkspaceInput {
  root_id: string;
  path: string;
  name?: string | null;
}

export interface RenameWorkspaceInput {
  name?: string | null;
}

export interface TaskView {
  task_id: string;
  state: TaskState | string;
  input: string;
  workspace_id: string | null;
  session_id: string | null;
  turn_id: string | null;
  routing_state: string;
  routing_reason: string | null;
  routing_confidence: number | null;
  metadata: JsonObject;
  created_at: string;
  updated_at: string;
}

export interface TaskEventView {
  event_id: string;
  task_id: string;
  event_type: string;
  payload: JsonObject;
  created_at: string;
}

export interface TurnView {
  turn_id: string;
  session_id: string;
  parent_turn_id: string | null;
  topology_status: TurnTopologyStatus;
  state: TurnState | string;
  input: { summary?: string; [key: string]: unknown } | null;
  output: { summary?: string; [key: string]: unknown } | null;
  failure: unknown | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  metadata: JsonObject;
}

export interface InboxMessageView {
  message_id: string;
  session_id: string;
  state: InboxMessageState | string;
  delivery_policy: InboxDeliveryPolicy | string;
  input: { summary: string; [key: string]: unknown };
  metadata: JsonObject;
  branch_target_turn_id: string | null;
  turn_id: string | null;
  steer_target_turn_id: string | null;
  retry_of_message_id: string | null;
  retried_by_message_id: string | null;
  superseded_by_message_id: string | null;
  failure_message: string | null;
  created_at: string;
  updated_at: string;
  dispatched_at: string | null;
  cancelled_at: string | null;
}

export interface EventView {
  event_id: string;
  session_id: string;
  turn_id: string | null;
  source: string;
  type: string;
  time: string;
  payload: JsonObject;
}

export type ManagedToolUseInput =
  | { type: 'read'; path: string; start_line?: number | null; end_line?: number | null }
  | { type: 'edit'; path: string; edits_count: number }
  | { type: 'write'; path: string }
  | { type: 'bash'; command: string; timeout?: number | null };

export interface ManagedToolUse {
  tool_name: string;
  input: ManagedToolUseInput;
}

export interface TimelineItem {
  item_id: string;
  kind: string;
  raw_kind?: string | null;
  role: string | null;
  title: string | null;
  status: string | null;
  occurred_at: string | null;
  content_preview: string | null;
  turn_id?: string | null;
  managed_tool_use?: ManagedToolUse | null;
}

export type TurnTimelineDirection = 'forward' | 'backward';

export type TurnTimelineItem = TimelineItem & { turn_id: string };

export interface TurnTimelinePage {
  session_id: string;
  direction: TurnTimelineDirection;
  items: TurnTimelineItem[];
  next_turn_id: string | null;
}

export interface TurnTimelineGroup {
  turn_id: string;
  parent_turn_id: string | null;
  state: TurnState | string;
  items: TurnTimelineItem[];
}

export interface TurnTreeHistoryPage {
  session_id: string;
  groups: TurnTimelineGroup[];
  next_from_turn_id: string | null;
}

export interface TurnTreeUpdatesPage {
  session_id: string;
  current_turn_id: string | null;
  retain_through_turn_id: string | null;
  groups: TurnTimelineGroup[];
}

export type DashboardStreamEvent =
  | { kind: 'session_event'; id: string; occurred_at: string; event: EventView }
  | { kind: 'task_event'; id: string; occurred_at: string; event: TaskEventView };

export interface CreateSessionInput {
  client_type: string;
  workspace?: string | null;
  workspace_id?: string | null;
  title?: string | null;
  handle?: string | null;
  role?: string | null;
  description?: string | null;
  execution_profile_id?: string | null;
  execution_profile_version?: string | null;
  metadata?: JsonObject;
  initial_task?: { input: string; metadata?: JsonObject } | null;
}

export interface UpdateSessionInput {
  title?: string | null;
}

export interface CreateSessionResult {
  session: SessionView;
  initial_turn: TurnView | null;
}

export interface SubmitTurnInput {
  input: string;
  metadata?: JsonObject;
}

export interface SubmitInboxMessageInput {
  input: string;
  delivery_policy?: InboxDeliveryPolicy;
  metadata?: JsonObject;
  branch_target_turn_id?: string | null;
}

export interface ApiEnvelope<T> {
  data: T | null;
  meta?: JsonObject;
  error?: { code: string; message: string } | null;
}


export interface SessionModel {
  id: string;
  name: string;
  description: string;
}

export interface SessionModels {
  models: SessionModel[];
  current_model: string | null;
  runtime_instance_id: string;
}
