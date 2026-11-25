export type JsonObject = Record<string, unknown>;

export type BotAvatarShape =
  | "classic"
  | "mochi"
  | "cloud"
  | "drop"
  | "blob"
  | "ghost"
  | "onigiri"
  | "egg"
  | "pudding";
export type BotAvatarColor =
  | "blue"
  | "aqua"
  | "green"
  | "yellow"
  | "orange"
  | "purple"
  | "indigo"
  | "brown"
  | "rose";

export interface BotConfiguration {
  name: string;
  description: string | null;
  avatar_shape: BotAvatarShape;
  avatar_color: BotAvatarColor;
  client_type: string;
  client_key: string;
  model: string;
  reasoning_effort: string | null;
}

export interface Bot extends BotConfiguration {
  bot_id: string;
}

export interface BotDeletionBlockers {
  running: boolean;
  helpers: SessionView[];
}

export type BotWorkStatus =
  | { kind: "processing" }
  | { kind: "working"; title: string }
  | { kind: "idle" };

// What the sidebar shows for each Bot, taken from its latest message page.
export type BotSummary = Pick<BotMessagePage, "status" | "open_todo_count">;

export type BotClient = Pick<
  AgentClientView,
  "id" | "name" | "client_type" | "client_key" | "version"
>;

export interface BotMessage {
  id: string;
  kind: "user_input" | "bot_reply";
  todo?:
    | { state: "open"; by: "user" | "bot"; at: string }
    | { state: "done"; by: "user" | "bot"; at: string; message_id?: string }
    | null;
  content: string;
  status: "normal" | "failed";
  created_at: string;
}

export interface BotMessageContext {
  messages: BotMessage[];
  truncated: boolean;
}

export type BotSearchType = "all" | "file" | "image";

/** A file a Bot chat message refers to, read from the message text. */
export interface BotMessageAttachment {
  source: "user_upload" | "attachment_block" | "markdown_link";
  media_kind: "file" | "image";
  path: string;
  name: string;
}

export interface BotSearchHit extends BotMessage {
  /** The matching files of the message, for file and image searches. */
  attachments?: BotMessageAttachment[];
}

export interface BotMessageSearchPage {
  messages: BotSearchHit[];
  next_before: string | null;
}

export interface BotNotice {
  id: string;
  kind: "turn.failed" | "session.error" | "session.exited";
  created_at: string;
}

export type BotEntry = BotMessage | BotNotice;

export interface BotMessagePage {
  open_todo_count: number;
  bot: Bot;
  status: BotWorkStatus;
  generation_notice: BotNotice | null;
  notice_session_id: string | null;
  file_link_session: { session_id: string; workspace: string | null } | null;
  entries: BotEntry[];
  next_before: string | null;
}

export interface AgentClientView {
  id: string;
  client_type: string;
  client_key: string;
  name: string;
  short_id: string;
  enabled: boolean;
  detection: {
    status: "detected" | "not_detected" | "unknown";
    detail: string | null;
  };
  setup: {
    status: "initialized" | "required" | "not_required" | "unknown";
    detail?: string;
  };
  setup_command: string | null;
  version: string | null;
  version_checked_at: string | null;
  has_record: boolean;
  created_at: string | null;
  updated_at: string | null;
}

export interface AgentClientSetupOperation {
  id: string;
  client_type: string;
  client_key: string;
  status: "running" | "succeeded" | "failed";
  error: string | null;
  output: string;
}

export type SessionState =
  | "created"
  | "starting"
  | "idle"
  | "busy"
  | "waiting_for_interaction"
  | "interrupted"
  | "exited"
  | "error";
export type TurnState = "queued" | "running" | "completed" | "failed" | "interrupted";
export type TurnTopologyStatus = "unknown" | "root" | "linked";
export type InboxDeliveryPolicy = "after_idle" | "interrupt_now" | "steer";
export type InboxMessageState =
  | "resuming"
  | "unknown"
  | "pending"
  | "dispatching"
  | "dispatched"
  | "cancelled"
  | "superseded"
  | "failed"
  | "dismissed";
export type WorkflowState =
  | "pending"
  | "running"
  | "paused"
  | "idle"
  | "completed"
  | "failed"
  | "recovering";
export type WorkflowAgentStatus =
  | "pending"
  | "starting"
  | "running"
  | "waiting_for_interaction"
  | "paused"
  | "idle"
  | "exiting"
  | "submitted"
  | "failed"
  | "unknown";

export interface WorkflowListItemView {
  workflow_id: string;
  title: string;
  state: WorkflowState;
  initiator_session_id: string;
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

export interface WorkflowNodeView {
  node_id: string;
  phase: string;
  title: string;
  status: WorkflowAgentStatus;
  session_id: string | null;
  session_state: string | null;
  submitted_at: string | null;
}

export interface WorkflowDetailView {
  retry_failure_event_id: string | null;
  retry_unavailable_reason: string | null;
  recoveries: WorkflowRecovery[];
  workflow_id: string;
  title: string;
  state: WorkflowState;
  initiator_session_id: string;
  failure_message: string | null;
  cwd: string;
  termination_reason: string | null;
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

export interface WorkflowRecovery {
  original_failure_message: string | null;
  recovery_id: string;
  failure_event_id: string;
  node_id: string;
  session_id: string;
  message_id: string;
  state: "requested" | "preparing" | "dispatching" | "completed" | "failed";
  failure_message: string | null;
  created_at: string;
}

export type ContextUsageCapability = "unsupported" | "estimated" | "exact";

export interface SessionCompactionAccepted {
  accepted: boolean;
  operation_id: string;
}

export interface SessionCapabilities {
  steer?: boolean;
  accept_task?: boolean;
  interrupt?: boolean;
  stream_output?: boolean;
  heartbeat?: boolean;
  timeline?: boolean;
  topology?: boolean;
  branch_control?: boolean;
  fork_session?: boolean;
  manual_compaction: boolean;
  compaction_events: boolean;
  compaction_history: boolean;
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
  confidence: "exact" | "estimated" | "unknown";
  observed_at: string;
}

export interface AgentProfileView {
  profile_id: string;
  name: string;
  description: string | null;
  client_type: string | null;
  client_key: string | null;
  model: string | null;
  reasoning_effort: string | null;
  use_yolo: boolean;
  system_prompt: string | null;
  created_at: string;
  updated_at: string;
}

export interface UpsertAgentProfileInput {
  profile_id: string;
  name: string;
  description?: string | null;
  client_type?: string | null;
  client_key?: string | null;
  model?: string | null;
  reasoning_effort?: string | null;
  use_yolo?: boolean;
  system_prompt?: string | null;
}

export interface SessionLineageView {
  relation_type: "fork" | string;
  parent_session_id: string;
  forked_from_turn_id: string | null;
  forked_from_client_node_id: string | null;
  parent_client_session_key: string | null;
  child_client_session_key: string | null;
  created_at: string;
}

export interface SessionCreationOption {
  model_catalog_key?: string | null;
  id: string;
  name: string;
  client_type: string;
  metadata: JsonObject;
  version: string | null;
}

export interface SessionView {
  [clientDetail: string]: unknown;
  session_id: string;
  client_type: string;
  title: string | null;
  handle: string | null;
  role: string | null;
  description: string | null;
  state: SessionState | string;
  current_turn_id: string | null;
  workspace_id: string | null;
  workspace: string | null;
  pinned_at: string | null;
  archived_at: string | null;
  capabilities: SessionCapabilities;
  control_connected?: boolean;
  control_disconnected_reason?: "unsupported_protocol_version";
  timeline_unavailable_reason?: string | null;
  model: string | null;
  reasoning_effort?: string | null;
  context_usage: ContextUsageView | null;
  lineage: SessionLineageView | null;
  created_at: string;
  updated_at: string;
  metadata: JsonObject;
}

export type SessionOverviewSection = "pinned" | "archived" | "active" | "list";

export interface SessionOverviewGroupView {
  sessions: SessionView[];
}

export interface SessionOverviewListGroupView extends SessionOverviewGroupView {
  next_cursor: string | null;
}

export interface SessionOverviewView {
  groups: {
    pinned?: SessionOverviewGroupView;
    archived?: SessionOverviewGroupView;
    active?: SessionOverviewGroupView;
    list?: SessionOverviewListGroupView;
  };
}

export type WorkspaceAvailability =
  | { status: "available"; root_id: string; path: string }
  | {
      status: "unavailable";
      reason: "outside_root" | "missing_directory" | "not_directory" | "permission_denied";
    }
  | { status: "unknown"; reason: "root_unavailable" | "check_failed" };

export interface WorkspaceView {
  availability: WorkspaceAvailability;
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
  state: "unknown" | "observed" | "error" | string;
  failure: string | null;
  observed_at: string | null;
  updated_at: string | null;
}

export type GitChangeGroup = "conflicted" | "staged" | "unstaged" | "untracked";

export interface GitChangedFile {
  /** Relative to the repository root; untracked directories end with "/". */
  path: string;
  orig_path?: string;
  group: GitChangeGroup;
  status: "M" | "A" | "D" | "R" | "C" | "T" | "U" | "?" | string;
  additions: number | null;
  deletions: number | null;
  binary: boolean;
}

export interface GitChangesView {
  repo_root: string | null;
  files: GitChangedFile[];
  total: number;
  truncated: boolean;
}

export interface GitDiffLine {
  kind: "context" | "add" | "delete";
  old: number | null;
  new: number | null;
  text: string;
}

export interface GitDiffView {
  path: string;
  group: GitChangeGroup;
  binary: boolean;
  hunks: { header: string; lines: GitDiffLine[] }[];
  truncated: boolean;
}

export interface GitBranch {
  /** Short name, such as `main` or `origin/main`. */
  name: string;
  /** Full ref name, such as `refs/heads/main`. */
  ref: string;
  remote: boolean;
  current: boolean;
}

export interface GitBranchesView {
  /** The checked-out branch ref, `HEAD` when detached, or null before the first commit. */
  head: string | null;
  branches: GitBranch[];
}

export interface GitCommitSummary {
  pushed?: boolean;
  sha: string;
  parents: string[];
  author_name: string;
  authored_at: string;
  subject: string;
}

export interface GitCommitsPage {
  commits: GitCommitSummary[];
  has_more: boolean;
}

export interface GitCommitFile {
  path: string;
  orig_path?: string;
  status: "M" | "A" | "D" | "R" | "C" | "T" | string;
  additions: number | null;
  deletions: number | null;
  binary: boolean;
}

export interface GitCommitView {
  sha: string;
  parents: string[];
  author_name: string;
  author_email: string;
  authored_at: string;
  committer_name: string;
  committed_at: string;
  subject: string;
  body: string;
  files: GitCommitFile[];
  total: number;
  truncated: boolean;
}

export interface GitCommitDiffView {
  sha: string;
  path: string;
  binary: boolean;
  hunks: { header: string; lines: GitDiffLine[] }[];
  truncated: boolean;
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
  kind: "directory" | string;
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
  kind: "directory" | "file" | string;
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

/** A file of a message's attachments block. */
export interface AttachedFile {
  path: string;
  name: string;
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
  | { type: "read"; path: string; start_line?: number | null; end_line?: number | null }
  | { type: "edit"; path: string; edits_count: number }
  | { type: "write"; path: string }
  | { type: "bash"; command: string; timeout?: number | null }
  | { type: "file_changes"; changes: ManagedFileChange[] };

export interface ManagedFileChange {
  path: string;
  operation: "add" | "update" | "delete";
  move_path?: string | null;
  diff: string;
}

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
  call_id?: string | null;
  managed_tool_use?: ManagedToolUse | null;
}

export type TurnTimelineDirection = "forward" | "backward";

export type TurnTimelineItem = TimelineItem & { turn_id: string | null };

export type HistorySource = "native" | "file";

export interface TurnTimelinePage {
  history_source?: HistorySource | null;
  session_id: string;
  direction: TurnTimelineDirection;
  items: TurnTimelineItem[];
  next_turn_id: string | null;
}

export interface TurnTimelineGroup {
  history_issue?: "topology_unknown" | "range_unavailable" | "range_invalid";
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
  | { kind: "session_event"; id: string; occurred_at: string; event: EventView }
  | { kind: "history_changed"; session_id: string }
  | { kind: "interaction_changed"; session_id: string; interaction_id: string }
  | { kind: "interactions_resync" };

export interface Interaction {
  interaction_id: string;
  session_id: string;
  turn_id: string | null;
  type: "choice";
  state: "pending" | "resolving" | "resolved" | "expired";
  title: string;
  description: string | null;
  options: { key: string; label: string; intent: "positive" | "negative" }[];
  response: { option_key: string } | null;
  requested_at: string;
  resolved_at: string | null;
}

export interface CreateSessionInput {
  reasoning_effort?: string;
  model?: string;
  client_type: string;
  workspace?: string | null;
  workspace_id?: string | null;
  title?: string | null;
  handle?: string | null;
  role?: string | null;
  description?: string | null;
  system_instructions?: string | null;
  include_standalone_session_instructions?: boolean;
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

export interface ReasoningEffortOption {
  id: string;
  name: string;
  description: string;
}

export interface SessionModel {
  reasoning_efforts?: ReasoningEffortOption[];
  default_reasoning_effort?: string | null;
  id: string;
  name: string;
  description: string;
}

export interface AgentClientModels {
  version: string | null;
  fetched_at: number;
  models: SessionModel[];
  default_model: string | null;
}

export interface ForkSessionOutcome {
  stage: "fork_unconfirmed" | "fork_failed" | "session_pending" | "session_created";
  session_id: string | null;
  error: string | null;
}

export type MemoryKind = "fact" | "decision" | "event" | "lesson" | "observation";
export type MemoryImportance = "low" | "normal" | "high";

export interface MemoryView {
  id: string;
  kind: MemoryKind;
  content: string;
  keywords: string[];
  workspace_id: string | null;
  session_id: string | null;
  archived: boolean;
  organized: boolean;
  importance: MemoryImportance;
  occurred_at: string | null;
  sources: string | null;
  created_by: "bot" | "dreaming";
  created_at: string;
  updated_at: string;
  last_read_at: string | null;
}

export interface MemoryListQuery {
  q?: string;
  cursor?: string;
}

export interface MemoryPage {
  memories: MemoryView[];
  total: number;
  next_cursor: string | null;
}

export interface UserFileView {
  text: string;
  character_limit: number;
  updated_at: string | null;
}

export interface ReplaceUserFileInput {
  text: string;
  /** The text the edit started from; the save fails if USER.md has changed since. */
  previous_text: string;
}
