export type JsonObject = Record<string, unknown>;

export type SessionState = 'created' | 'starting' | 'idle' | 'busy' | 'interrupted' | 'exited' | 'error';
export type TaskState = 'created' | 'routing' | 'needs_confirmation' | 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
export type TurnState = 'queued' | 'running' | 'completed' | 'failed' | 'interrupted' | 'cancelled';
export type InboxDeliveryPolicy = 'after_idle' | 'interrupt_now';
export type InboxMessageState = 'pending' | 'dispatching' | 'dispatched' | 'cancelled' | 'superseded' | 'failed';

export interface SessionCapabilities {
  accept_task?: boolean;
  interrupt?: boolean;
  stream_output?: boolean;
  heartbeat?: boolean;
  artifact_sources?: boolean;
  [key: string]: unknown;
}

export interface AgentProfileView {
  profile_id: string;
  version: string;
  name: string;
  description: string | null;
  supported_client_types: string[];
  system_prompt_template: string | null;
  turn_prompt_template: string | null;
  default_session_role: string | null;
  default_session_description: string | null;
  handle_prefix: string | null;
  session_reuse_policy: string;
  expected_output_schema: string | null;
  artifact_contract: JsonObject;
  default_execution_policy: JsonObject;
  default_review_policy: JsonObject;
  metadata: JsonObject;
  created_at: string;
  updated_at: string;
}

export interface SessionView {
  session_id: string;
  client_type: string;
  handle: string | null;
  state: SessionState | string;
  current_turn_id: string | null;
  workspace_id: string | null;
  workspace: string | null;
  capabilities: SessionCapabilities;
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

export interface CreateTaskInput {
  input: string;
  workspace?: string | null;
  client_type: string;
  metadata?: JsonObject;
}

export interface ConfirmTaskWorkspaceInput {
  workspace: string;
  client_type: string;
}

export interface SubmitPlannerInput {
  message: string;
  client_type: string;
}

export interface CreateSessionInput {
  client_type: string;
  workspace?: string | null;
  workspace_id?: string | null;
  handle?: string | null;
  role?: string | null;
  description?: string | null;
  metadata?: JsonObject;
  initial_task?: { input: string; metadata?: JsonObject } | null;
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
