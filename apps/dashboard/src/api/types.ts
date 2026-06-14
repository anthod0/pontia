export type JsonObject = Record<string, unknown>;

export type SessionState = 'created' | 'starting' | 'idle' | 'busy' | 'interrupted' | 'exited' | 'error';
export type TaskState = 'created' | 'routing' | 'needs_confirmation' | 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
export type TurnState = 'queued' | 'running' | 'completed' | 'failed' | 'interrupted' | 'cancelled';
export type InboxDeliveryPolicy = 'after_idle' | 'interrupt_now';
export type InboxMessageState = 'pending' | 'dispatching' | 'dispatched' | 'cancelled' | 'superseded' | 'failed' | 'dismissed';

export type ContextUsageCapability = 'unsupported' | 'estimated' | 'exact';

export interface SessionCapabilities {
  accept_task?: boolean;
  interrupt?: boolean;
  stream_output?: boolean;
  heartbeat?: boolean;
  artifact_sources?: boolean;
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

export type AgentKind = 'planner' | 'executor';

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

export interface SessionView {
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
  capabilities: SessionCapabilities;
  model: string | null;
  context_usage: ContextUsageView | null;
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

export interface DagProposalView {
  proposal_id: string;
  task_id: string;
  mode: string;
  state: string;
  summary: string;
  proposal_json: JsonObject;
  validation_json: JsonObject;
  created_by_session_id: string | null;
  revision: number;
  supersedes_proposal_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface WorkItemRuntimeView {
  current_run_id: string | null;
  current_state: string;
  current_attempt: number;
  ready_at: string | null;
  blocked_reason: string | null;
  outcome_state: string | null;
  outcome_reason: string | null;
  replanned_from_state: string | null;
  retry_count: number;
  max_retries: number;
  priority: number;
  optional: boolean;
  parallelizable: boolean;
  session_id: string | null;
  turn_id: string | null;
  updated_at: string;
}

export interface WorkItemView {
  work_item_id: string;
  task_id: string;
  title: string;
  description: string;
  kind: string;
  action: string;
  execution_profile_id: string;
  execution_profile_version: string | null;
  active: boolean;
  priority: number;
  optional: boolean;
  parallelizable: boolean;
  acceptance_criteria: unknown;
  metadata: JsonObject;
  created_at: string;
  updated_at: string;
  runtime: WorkItemRuntimeView | null;
}

export interface WorkItemEdgeView {
  edge_id: string;
  task_id: string;
  from_work_item_id: string;
  to_work_item_id: string;
  edge_type: string;
  created_at: string;
}

export interface WorkItemRunView {
  run_id: string;
  work_item_id: string;
  task_id: string;
  attempt: number;
  state: string;
  session_id: string | null;
  turn_id: string | null;
  client_type: string | null;
  execution_profile_id: string;
  execution_profile_version: string | null;
  rendered_prompt_ref: string | null;
  output_summary: string | null;
  failure: unknown | null;
  created_at: string;
  updated_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface DagSignalView {
  signal_id: string;
  task_id: string;
  work_item_id: string | null;
  run_id: string | null;
  source_session_id: string | null;
  source: 'agent' | 'human' | 'system' | string;
  kind: string;
  summary: string;
  detail: string | null;
  severity: string;
  related_refs: unknown;
  state: string;
  created_at: string;
  updated_at: string;
}

export interface DagSummaryView {
  total_work_items: number;
  ready_work_items: number;
  running_work_items: number;
  completed_work_items: number;
  blocked_work_items: number;
  failed_work_items: number;
  open_signals: number;
  total_runs: number;
}

export interface TaskDagView {
  task_id: string;
  summary: DagSummaryView;
  work_items: WorkItemView[];
  edges: WorkItemEdgeView[];
  runs: WorkItemRunView[];
  signals: DagSignalView[];
}

export interface TurnView {
  turn_id: string;
  session_id: string;
  state: TurnState | string;
  input: { summary?: string; artifact_id?: string | null; [key: string]: unknown } | null;
  output: { summary?: string; artifact_ids?: string[]; [key: string]: unknown } | null;
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
  turn_id: string | null;
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

export interface TimelineItem {
  item_id: string;
  kind: string;
  raw_kind?: string | null;
  role: string | null;
  title: string | null;
  status: string | null;
  occurred_at: string | null;
  content_preview: string | null;
  content_ref: string;
  turn_id?: string | null;
}

export interface TimelinePage {
  session_id: string;
  binding_id: string;
  items: TimelineItem[];
  head_cursor: string | null;
  tail_cursor: string | null;
  has_more: boolean;
  source_id: string;
}

export interface TimelineItemDetail {
  binding_id: string;
  content_ref: string;
  content_type: string;
  text: string;
  size_bytes: number;
}

export type DashboardStreamEvent =
  | { kind: 'session_event'; id: string; occurred_at: string; event: EventView }
  | { kind: 'task_event'; id: string; occurred_at: string; event: TaskEventView };

export interface ArtifactView {
  artifact_id: string;
  session_id: string;
  turn_id: string | null;
  kind: string;
  name: string;
  size_bytes: number | null;
  preview: string | null;
  created_at: string;
  metadata: JsonObject;
}

export interface CreateDagTaskInput {
  input: string;
  workspace?: string | null;
  client_type: string;
  metadata?: JsonObject;
}

export interface DagPlanningTurnView {
  task_id: string;
  session_id: string;
  turn_id: string;
  profile_id: string;
}

export interface CreateDagTaskResult {
  task: TaskView;
  planning_turn: DagPlanningTurnView;
}

export interface HumanSignalInput {
  kind: string;
  summary: string;
  detail?: string | null;
  severity?: 'low' | 'medium' | 'high';
}

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
}

export interface ArtifactContent {
  artifactId: string;
  contentType: string;
  text: string;
  bytes: ArrayBuffer;
}

export interface ApiEnvelope<T> {
  data: T | null;
  meta?: JsonObject;
  error?: { code: string; message: string } | null;
}
