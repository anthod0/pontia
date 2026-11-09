import type {
  AgentProfileView,
  DashboardStreamEvent,
  EventView,
  InboxMessageView,
  JsonObject,
  SessionOverviewView,
  SessionView,
  TurnTimelineGroup,
  TurnView,
  UpsertAgentProfileInput,
  WorkflowListItemView,
  WorkspaceView,
} from "../src/api/types.ts";
import {
  contextUsage,
  sessionEvent,
  timelineItem,
  type MockData,
  type MockSession,
} from "./data.ts";

/** In-memory state of one mocked device and its open dashboard event streams. */
export interface MockDevice {
  data: MockData;
  streams: Set<(event: DashboardStreamEvent) => void>;
  timers: Map<string, ReturnType<typeof setTimeout>>;
}

export function createMockDevice(data: MockData): MockDevice {
  return { data, streams: new Set(), timers: new Map() };
}

/** A response body sent without the `{ data, meta, error }` envelope. */
class Unwrapped {
  readonly body: unknown;

  constructor(body: unknown) {
    this.body = body;
  }
}

class HttpError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

type Params = string[];
type Handler = (context: {
  device: MockDevice;
  params: Params;
  query: URLSearchParams;
  body: JsonObject;
}) => unknown;

const TURN_DURATION_MS = 2_500;
const encoder = new TextEncoder();

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function now(): string {
  return new Date().toISOString();
}

function notFound(what: string): never {
  throw new HttpError(404, "not_found", `${what} not found`);
}

function findSession(device: MockDevice, sessionId: string): MockSession {
  return (
    device.data.sessions.find((session) => session.view.session_id === sessionId) ??
    notFound("Session")
  );
}

function findWorkspace(device: MockDevice, workspaceId: string): WorkspaceView {
  return (
    device.data.workspaces.find((workspace) => workspace.workspace_id === workspaceId) ??
    notFound("Workspace")
  );
}

function findWorkflow(device: MockDevice, workflowId: string) {
  return (
    device.data.workflows.find((workflow) => workflow.detail.workflow_id === workflowId) ??
    notFound("Workflow")
  );
}

function broadcast(device: MockDevice, event: DashboardStreamEvent): void {
  for (const send of device.streams) send(event);
}

function emit(
  device: MockDevice,
  session: MockSession,
  type: string,
  turnId: string | null,
  payload: JsonObject = {},
): EventView {
  const event = sessionEvent(
    device.data.nextId,
    session.view.session_id,
    turnId,
    type,
    now(),
    payload,
  );
  session.events.push(event);
  broadcast(device, {
    kind: "session_event",
    id: event.event_id,
    occurred_at: event.time,
    event,
  });
  return event;
}

function touch(session: MockSession, state?: SessionView["state"]): void {
  if (state) session.view.state = state;
  session.view.updated_at = now();
}

function cancelTimer(device: MockDevice, sessionId: string): void {
  clearTimeout(device.timers.get(sessionId));
  device.timers.delete(sessionId);
}

function startTurn(device: MockDevice, session: MockSession, input: string, messageId?: string) {
  const startedAt = now();
  const turn: TurnView = {
    turn_id: device.data.nextId("turn"),
    session_id: session.view.session_id,
    parent_turn_id: session.turns.at(-1)?.turn_id ?? null,
    topology_status: session.turns.length ? "linked" : "root",
    state: "running",
    input: { summary: input },
    output: null,
    failure: null,
    created_at: startedAt,
    started_at: startedAt,
    completed_at: null,
    metadata: messageId ? { inbox_message_id: messageId } : {},
  };
  session.turns.push(turn);
  session.items.set(turn.turn_id, [
    timelineItem(device.data.nextId, turn.turn_id, startedAt, {
      kind: "user",
      content_preview: input,
    }),
  ]);
  session.view.current_turn_id = turn.turn_id;
  touch(session, "busy");
  emit(
    device,
    session,
    "turn.started",
    turn.turn_id,
    messageId ? { metadata: { inbox_message_id: messageId } } : {},
  );
  device.timers.set(
    session.view.session_id,
    setTimeout(() => completeTurn(device, session, turn, mockReply(input)), TURN_DURATION_MS),
  );
  return turn;
}

function mockReply(input: string): string {
  return `This is a mock reply. You wrote:\n\n> ${input.split("\n").join("\n> ")}\n\nThe mock agent ran \`bun run check\` and found no errors.`;
}

function completeTurn(
  device: MockDevice,
  session: MockSession,
  turn: TurnView,
  reply: string,
): void {
  cancelTimer(device, session.view.session_id);
  const completedAt = now();
  const callId = device.data.nextId("call");
  const items = session.items.get(turn.turn_id) ?? [];
  items.push(
    timelineItem(device.data.nextId, turn.turn_id, completedAt, {
      kind: "thinking",
      content_preview: "Working out what the user asked for.",
    }),
    timelineItem(device.data.nextId, turn.turn_id, completedAt, {
      kind: "tool_call",
      role: "tool",
      title: "bash",
      status: "started",
      call_id: callId,
      content_preview: 'bash {"command":"bun run check"}',
      managed_tool_use: { tool_name: "bash", input: { type: "bash", command: "bun run check" } },
    }),
    timelineItem(device.data.nextId, turn.turn_id, completedAt, {
      kind: "tool_result",
      role: "tool",
      title: "bash",
      status: "completed",
      call_id: callId,
      content_preview: "svelte-check found 0 errors and 0 warnings",
    }),
    timelineItem(device.data.nextId, turn.turn_id, completedAt, {
      kind: "assistant",
      content_preview: reply,
    }),
  );
  turn.state = "completed";
  turn.completed_at = completedAt;
  turn.output = { summary: reply.slice(0, 200) };
  session.view.context_usage = contextUsage((session.view.context_usage?.used_tokens ?? 0) + 4_000);
  touch(session, "idle");
  emit(device, session, "turn.completed", turn.turn_id);
  dispatchNextInboxMessage(device, session);
}

function dispatchNextInboxMessage(device: MockDevice, session: MockSession): void {
  const next = session.inbox.find((message) => message.state === "pending");
  if (!next) return;
  dispatch(device, session, next);
}

function dispatch(device: MockDevice, session: MockSession, message: InboxMessageView): void {
  const turn = startTurn(device, session, message.input.summary, message.message_id);
  Object.assign(message, {
    state: "dispatched",
    turn_id: turn.turn_id,
    dispatched_at: now(),
    updated_at: now(),
  });
}

function runningTurn(session: MockSession): TurnView | undefined {
  const turn = session.turns.at(-1);
  return turn?.state === "running" ? turn : undefined;
}

function interrupt(device: MockDevice, session: MockSession): void {
  const turn = runningTurn(session);
  cancelTimer(device, session.view.session_id);
  if (!turn) return;
  turn.state = "interrupted";
  turn.completed_at = now();
  touch(session, "interrupted");
  emit(device, session, "turn.interrupted", turn.turn_id);
}

function turnGroup(session: MockSession, turn: TurnView): TurnTimelineGroup {
  return {
    turn_id: turn.turn_id,
    parent_turn_id: turn.parent_turn_id,
    state: turn.state,
    items: session.items.get(turn.turn_id) ?? [],
  };
}

function limitParam(query: URLSearchParams, fallback: number): number {
  const limit = Number(query.get("limit") ?? fallback);
  return Number.isInteger(limit) && limit > 0 ? limit : fallback;
}

function turnIndex(session: MockSession, turnId: string): number {
  const index = session.turns.findIndex((turn) => turn.turn_id === turnId);
  if (index < 0) throw new HttpError(404, "turn_not_found", "Turn not found");
  return index;
}

function sessionOverview(device: MockDevice, query: URLSearchParams): SessionOverviewView {
  const sections = new Set((query.get("sections") ?? "").split(","));
  const workspaceId = query.get("workspace_id");
  const limit = limitParam(query, 50);
  const offset = Number(query.get("cursor") ?? 0);
  const sessions = device.data.sessions
    .map((session) => session.view)
    .filter((view) => !workspaceId || view.workspace_id === workspaceId);
  const unarchived = sessions
    .filter((view) => !view.archived_at)
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  const groups: SessionOverviewView["groups"] = {};
  if (sections.has("pinned")) {
    groups.pinned = {
      sessions: unarchived
        .filter((view) => view.pinned_at)
        .sort((a, b) => b.pinned_at!.localeCompare(a.pinned_at!)),
    };
  }
  if (sections.has("archived")) {
    groups.archived = {
      sessions: sessions
        .filter((view) => view.archived_at)
        .sort((a, b) => b.archived_at!.localeCompare(a.archived_at!)),
    };
  }
  if (sections.has("active")) {
    groups.active = {
      sessions: unarchived.filter((view) => view.state !== "exited" && view.state !== "error"),
    };
  }
  if (sections.has("list")) {
    const page = unarchived.slice(offset, offset + limit);
    groups.list = {
      sessions: page,
      next_cursor: offset + limit < unarchived.length ? String(offset + limit) : null,
    };
  }
  return { groups };
}

function workflowListItem(detail: MockData["workflows"][number]["detail"]): WorkflowListItemView {
  return {
    workflow_id: detail.workflow_id,
    title: detail.title,
    state: detail.state,
    current_revision: detail.current_revision,
    failure_message: detail.failure_message,
    agent_submitted_count: detail.agent_submitted_count,
    agent_total_count: detail.agent_total_count,
    current_phase_name:
      detail.nodes.find((node) => node.node_id === detail.current_node_id)?.phase ?? null,
    started_at: detail.started_at,
    completed_at: detail.completed_at,
    created_at: detail.created_at,
    updated_at: detail.updated_at,
    elapsed_ms: detail.elapsed_ms,
    observation_error: null,
  };
}

function latestProfiles(profiles: AgentProfileView[], includeArchived: boolean) {
  const latest = new Map<string, AgentProfileView>();
  for (const profile of profiles) {
    if (includeArchived || profile.active) latest.set(profile.profile_id, profile);
  }
  return [...latest.values()];
}

function profileFromInput(input: UpsertAgentProfileInput): AgentProfileView {
  const timestamp = now();
  return {
    description: null,
    supported_client_types: [],
    system_prompt_template: null,
    turn_prompt_template: null,
    default_session_role: null,
    default_session_description: null,
    handle_prefix: null,
    expected_output_schema: null,
    artifact_contract: {},
    default_execution_policy: {},
    default_review_policy: {},
    metadata: {},
    ...input,
    active: true,
    archived_at: null,
    archived_reason: null,
    created_at: timestamp,
    updated_at: timestamp,
  };
}

function archiveProfile(profile: AgentProfileView): void {
  profile.active = false;
  profile.archived_at = now();
  profile.archived_reason = "Deleted from the dashboard";
}

function findProfileVersion(device: MockDevice, profileId: string, version: string) {
  return (
    device.data.agentProfiles.find(
      (profile) => profile.profile_id === profileId && profile.version === version,
    ) ?? notFound("Agent profile version")
  );
}

function findAgentClient(device: MockDevice, clientType: string, clientKey: string) {
  return (
    device.data.agentClients.find(
      (client) => client.client_type === clientType && client.client_key === clientKey,
    ) ?? notFound("Agent client")
  );
}

function findInboxMessage(session: MockSession, messageId: string): InboxMessageView {
  return (
    session.inbox.find((message) => message.message_id === messageId) ?? notFound("Inbox message")
  );
}

function createInboxMessage(
  session: MockSession,
  messageId: string,
  input: string,
  deliveryPolicy: string,
  metadata: JsonObject,
): InboxMessageView {
  const timestamp = now();
  const message: InboxMessageView = {
    message_id: messageId,
    session_id: session.view.session_id,
    state: "pending",
    delivery_policy: deliveryPolicy,
    input: { summary: input },
    metadata,
    branch_target_turn_id: null,
    turn_id: null,
    steer_target_turn_id: null,
    retry_of_message_id: null,
    retried_by_message_id: null,
    superseded_by_message_id: null,
    failure_message: null,
    created_at: timestamp,
    updated_at: timestamp,
    dispatched_at: null,
    cancelled_at: null,
  };
  session.inbox.push(message);
  return message;
}

function submitInboxMessage(
  device: MockDevice,
  session: MockSession,
  messageId: string,
  body: JsonObject,
): InboxMessageView {
  const existing = session.inbox.find((message) => message.message_id === messageId);
  if (existing) return existing;
  const policy = typeof body.delivery_policy === "string" ? body.delivery_policy : "after_idle";
  const message = createInboxMessage(
    session,
    messageId,
    String(body.input ?? ""),
    policy,
    (body.metadata as JsonObject | undefined) ?? {},
  );
  if (policy === "interrupt_now") interrupt(device, session);
  if (!runningTurn(session)) dispatch(device, session, message);
  return message;
}

function directoryEntries(device: MockDevice, path: string) {
  const children = device.data.directories[path];
  if (!children) notFound("Directory");
  const root = device.data.roots[0].canonical_path!;
  return {
    root_id: "home",
    path,
    canonical_path: path ? `${root}/${path}` : root,
    parent_path: path ? path.split("/").slice(0, -1).join("/") : null,
    entries: children.map((name) => {
      const childPath = path ? `${path}/${name}` : name;
      return {
        name,
        path: childPath,
        kind: "directory",
        is_workspace: device.data.workspaces.some(
          (workspace) => workspace.canonical_path === `${root}/${childPath}`,
        ),
      };
    }),
    warnings: [],
  };
}

function eventStream(device: MockDevice, signal: AbortSignal): Response {
  let send: ((event: DashboardStreamEvent) => void) | undefined;
  let heartbeat: ReturnType<typeof setInterval> | undefined;
  const close = () => {
    if (send) device.streams.delete(send);
    clearInterval(heartbeat);
  };
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      send = (event) => {
        const id = event.kind === "session_event" ? `id: ${event.id}\n` : "";
        controller.enqueue(encoder.encode(`${id}data: ${JSON.stringify(event)}\n\n`));
      };
      device.streams.add(send);
      controller.enqueue(encoder.encode(": connected\n\n"));
      heartbeat = setInterval(() => controller.enqueue(encoder.encode(": ping\n\n")), 15_000);
      signal.addEventListener(
        "abort",
        () => {
          close();
          controller.close();
        },
        { once: true },
      );
    },
    cancel: close,
  });
  return new Response(body, {
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    },
  });
}

// Routes mirror the pontiad `/api/v1` paths used by src/api/client.ts.
const routes: [method: string, pattern: string, handler: Handler][] = [
  ["GET", "/auth/validate", () => ({ valid: true })],

  ["GET", "/agent-clients", ({ device }) => ({ agent_clients: device.data.agentClients })],
  [
    "PATCH",
    "/agent-clients/:type/:key",
    ({ device, params: [type, key], body }) => {
      const client = findAgentClient(device, type, key);
      if (typeof body.enabled === "boolean") client.enabled = body.enabled;
      if (body.config_overrides) {
        client.config_overrides = body.config_overrides as JsonObject;
        client.effective_config = { ...client.effective_config, ...client.config_overrides };
      }
      client.has_record = true;
      client.updated_at = now();
      client.created_at ??= client.updated_at;
      return { agent_client: client };
    },
  ],
  [
    "DELETE",
    "/agent-clients/:type/:key",
    ({ device, params: [type, key] }) => {
      const client = findAgentClient(device, type, key);
      Object.assign(client, {
        enabled: true,
        has_record: false,
        config_overrides: {},
        created_at: null,
        updated_at: null,
      });
      return { reset: true };
    },
  ],
  ["GET", "/session-creation-options", ({ device }) => ({ options: device.data.creationOptions })],

  [
    "GET",
    "/agent-profiles",
    ({ device, query }) => ({
      agent_profiles: latestProfiles(
        device.data.agentProfiles,
        query.get("include_archived") === "true",
      ),
    }),
  ],
  [
    "POST",
    "/agent-profiles",
    ({ device, body }) => {
      const profile = profileFromInput(body as unknown as UpsertAgentProfileInput);
      device.data.agentProfiles.push(profile);
      return { agent_profile: profile };
    },
  ],
  [
    "DELETE",
    "/agent-profiles/:id",
    ({ device, params: [profileId] }) => {
      const versions = device.data.agentProfiles.filter(
        (profile) => profile.profile_id === profileId && profile.active,
      );
      versions.forEach(archiveProfile);
      return { profile_id: profileId, archived_versions: versions.length };
    },
  ],
  [
    "GET",
    "/agent-profiles/:id/versions",
    ({ device, params: [profileId], query }) => ({
      agent_profile_versions: device.data.agentProfiles.filter(
        (profile) =>
          profile.profile_id === profileId &&
          (profile.active || query.get("include_archived") === "true"),
      ),
    }),
  ],
  [
    "POST",
    "/agent-profiles/:id/versions",
    ({ device, body }) => {
      const profile = profileFromInput(body as unknown as UpsertAgentProfileInput);
      device.data.agentProfiles.push(profile);
      return { agent_profile: profile };
    },
  ],
  [
    "PUT",
    "/agent-profiles/:id/versions/:version",
    ({ device, params: [profileId, version], body }) => {
      const profile = findProfileVersion(device, profileId, version);
      Object.assign(profile, body, { updated_at: now() });
      return { agent_profile: profile };
    },
  ],
  [
    "DELETE",
    "/agent-profiles/:id/versions/:version",
    ({ device, params: [profileId, version] }) => {
      const profile = findProfileVersion(device, profileId, version);
      archiveProfile(profile);
      return { agent_profile: profile };
    },
  ],

  [
    "GET",
    "/workflows",
    ({ device, query }) => ({
      workflows: device.data.workflows
        .slice(0, limitParam(query, 50))
        .map((workflow) => workflowListItem(workflow.detail)),
    }),
  ],
  [
    "GET",
    "/workflows/:id",
    ({ device, params: [id] }) => ({ workflow: findWorkflow(device, id).detail }),
  ],
  [
    "GET",
    "/workflows/:id/revisions/:revision",
    ({ device, params: [id, revision] }) => ({
      revision:
        findWorkflow(device, id).revisions.find((item) => item.revision === Number(revision)) ??
        notFound("Workflow revision"),
    }),
  ],
  [
    "GET",
    "/workflows/:id/patches",
    ({ device, params: [id] }) => ({ patches: findWorkflow(device, id).patches }),
  ],
  [
    "GET",
    "/workflows/:id/documents",
    ({ device, params: [id], query }) => {
      const ref = query.get("ref") ?? "";
      const content = findWorkflow(device, id).documents[ref] ?? notFound("Workflow document");
      return { document: { workflow_id: id, document_ref: ref, content } };
    },
  ],
  [
    "POST",
    "/workflows/:id/pause",
    ({ device, params: [id] }) => {
      const workflow = findWorkflow(device, id);
      Object.assign(workflow.detail, { state: "paused", updated_at: now() });
      return { workflow: workflow.detail };
    },
  ],
  [
    "POST",
    "/workflows/:id/resume",
    ({ device, params: [id] }) => {
      const workflow = findWorkflow(device, id);
      Object.assign(workflow.detail, { state: "running", updated_at: now() });
      return { workflow: workflow.detail };
    },
  ],
  [
    "POST",
    "/workflows/:id/retry",
    ({ device, params: [id] }) => {
      const workflow = findWorkflow(device, id);
      for (const node of workflow.detail.nodes) {
        if (node.status === "failed") node.status = "running";
      }
      Object.assign(workflow.detail, {
        state: "running",
        failure_message: null,
        retry_failure_event_id: null,
        completed_at: null,
        updated_at: now(),
      });
      return { workflow: workflow.detail };
    },
  ],

  ["GET", "/workspaces", ({ device }) => ({ workspaces: device.data.workspaces })],
  [
    "POST",
    "/workspaces",
    ({ device, body }) => {
      const path = String(body.path ?? "");
      const root = device.data.roots[0].canonical_path!;
      const canonicalPath = path ? `${root}/${path}` : root;
      const timestamp = now();
      const workspace: WorkspaceView = {
        workspace_id: device.data.nextId("wks"),
        canonical_path: canonicalPath,
        display_path: canonicalPath.replace(/^\/home\/dev/, "~"),
        name: (body.name as string | null) || canonicalPath.split("/").at(-1) || null,
        state: "active",
        metadata: {},
        created_at: timestamp,
        updated_at: timestamp,
        last_used_at: null,
      };
      device.data.workspaces.push(workspace);
      return { workspace };
    },
  ],
  [
    "PATCH",
    "/workspaces/:id",
    ({ device, params: [id], body }) => {
      const workspace = findWorkspace(device, id);
      Object.assign(workspace, { name: body.name ?? null, updated_at: now() });
      return { workspace };
    },
  ],
  [
    "DELETE",
    "/workspaces/:id",
    ({ device, params: [id] }) => {
      const workspace = findWorkspace(device, id);
      device.data.workspaces = device.data.workspaces.filter((item) => item !== workspace);
      return { workspace };
    },
  ],
  [
    "POST",
    "/workspaces/:id/git-status/refresh",
    ({ device, params: [id] }) => {
      const workspace = findWorkspace(device, id);
      const timestamp = now();
      return {
        git_status: {
          workspace_id: id,
          repo_root: workspace.canonical_path,
          branch: "main",
          upstream: "origin/main",
          ahead: 2,
          behind: 0,
          staged_count: 1,
          unstaged_count: 3,
          untracked_count: 1,
          conflicted_count: 0,
          clean: false,
          state: "observed",
          failure: null,
          observed_at: timestamp,
          updated_at: timestamp,
        },
      };
    },
  ],
  [
    "GET",
    "/workspaces/:id/file-picker",
    ({ device, params: [id], query }) => {
      findWorkspace(device, id);
      const search = (query.get("query") ?? "").toLowerCase();
      const files = device.data.files
        .filter((path) => path.toLowerCase().includes(search))
        .slice(0, limitParam(query, 50))
        .map((path) => ({ path, name: path.split("/").at(-1)!, kind: "file" }));
      return { files, truncated: false, warnings: [] };
    },
  ],
  ["GET", "/workspace-roots", ({ device }) => ({ roots: device.data.roots })],
  [
    "GET",
    "/workspace-roots/:id/entries",
    ({ device, query }) => directoryEntries(device, query.get("path") ?? ""),
  ],

  [
    "GET",
    "/sessions/overview",
    ({ device, query }) => new Unwrapped(sessionOverview(device, query)),
  ],
  [
    "POST",
    "/sessions",
    ({ device, body }) => {
      const workspace = findWorkspace(device, String(body.workspace_id ?? ""));
      const option = device.data.creationOptions.find(
        (item) => item.client_type === body.client_type,
      );
      const timestamp = now();
      const initialTask = body.initial_task as { input: string } | null | undefined;
      const model = device.data.models[0].id;
      const session: MockSession = {
        view: {
          session_id: device.data.nextId("sess"),
          client_type: String(body.client_type),
          title: (body.title as string | null) ?? initialTask?.input.slice(0, 60) ?? null,
          handle: null,
          role: null,
          description: null,
          execution_profile_id: null,
          execution_profile_version: null,
          state: "idle",
          current_turn_id: null,
          workspace_id: workspace.workspace_id,
          workspace: workspace.canonical_path,
          pinned_at: null,
          archived_at: null,
          capabilities: structuredClone(device.data.sessions[0]?.view.capabilities) ?? {
            manual_compaction: true,
            compaction_events: true,
            compaction_history: false,
            timeline: true,
            topology: true,
            interrupt: true,
            list_models: true,
            set_model: true,
          },
          timeline_unavailable_reason: null,
          model,
          model_control_unavailable_reason: null,
          context_usage: contextUsage(6_000),
          lineage: null,
          created_at: timestamp,
          updated_at: timestamp,
          metadata: { ...(option?.metadata ?? {}), model },
        },
        turns: [],
        items: new Map(),
        inbox: [],
        events: [],
        interactions: [],
      };
      device.data.sessions.push(session);
      workspace.last_used_at = timestamp;
      emit(device, session, "session.ready", null);
      const initialTurn = initialTask ? startTurn(device, session, initialTask.input) : null;
      return { session: session.view, initial_turn: initialTurn };
    },
  ],
  [
    "GET",
    "/sessions/:id",
    ({ device, params: [id] }) => ({ session: findSession(device, id).view }),
  ],
  [
    "PATCH",
    "/sessions/:id",
    ({ device, params: [id], body }) => {
      const session = findSession(device, id);
      session.view.title = (body.title as string | null) ?? null;
      touch(session);
      emit(device, session, "session.updated", null);
      return { session: session.view };
    },
  ],
  ...(["pin", "unpin", "archive"] as const).map((action): [string, string, Handler] => [
    "POST",
    `/sessions/:id/${action}`,
    ({ device, params: [id] }) => {
      const session = findSession(device, id);
      if (action === "pin") session.view.pinned_at = now();
      if (action === "unpin") session.view.pinned_at = null;
      if (action === "archive") session.view.archived_at = now();
      touch(session);
      emit(device, session, `session.${action === "archive" ? "archived" : action + "ned"}`, null);
      return { session: session.view };
    },
  ]),
  [
    "GET",
    "/sessions/:id/turns",
    ({ device, params: [id] }) => ({ turns: findSession(device, id).turns }),
  ],
  [
    "GET",
    "/sessions/:id/events",
    ({ device, params: [id] }) => ({ events: findSession(device, id).events }),
  ],
  [
    "GET",
    "/sessions/:id/turns/timeline",
    ({ device, params: [id], query }) => {
      const session = findSession(device, id);
      const direction = query.get("direction") === "forward" ? "forward" : "backward";
      const anchor = query.get("turn_id");
      const anchorIndex = anchor ? turnIndex(session, anchor) : null;
      const directional =
        direction === "forward"
          ? session.turns.slice(anchorIndex ?? 0)
          : session.turns.slice(0, (anchorIndex ?? session.turns.length - 1) + 1).reverse();
      const limit = limitParam(query, 20);
      const selected = directional
        .slice(0, limit)
        .sort((a, b) => a.turn_id.localeCompare(b.turn_id));
      return {
        session_id: id,
        direction,
        items: selected.flatMap((turn) => session.items.get(turn.turn_id) ?? []),
        next_turn_id: directional[limit]?.turn_id ?? null,
      };
    },
  ],
  [
    "GET",
    "/sessions/:id/turns/tree/history",
    ({ device, params: [id], query }) => {
      const session = findSession(device, id);
      const from = query.get("from_turn_id");
      const end = from ? turnIndex(session, from) : session.turns.length - 1;
      const start = Math.max(0, end - limitParam(query, 20) + 1);
      return {
        session_id: id,
        groups: session.turns.slice(start, end + 1).map((turn) => turnGroup(session, turn)),
        next_from_turn_id: start > 0 ? session.turns[start - 1].turn_id : null,
      };
    },
  ],
  [
    "GET",
    "/sessions/:id/turns/tree/updates",
    ({ device, params: [id], query }) => {
      const session = findSession(device, id);
      const from = query.get("from_turn_id");
      const fromIndex = from ? session.turns.findIndex((turn) => turn.turn_id === from) : -1;
      return {
        session_id: id,
        current_turn_id: session.turns.at(-1)?.turn_id ?? null,
        retain_through_turn_id: fromIndex >= 0 ? from : null,
        groups: session.turns.slice(Math.max(fromIndex, 0)).map((turn) => turnGroup(session, turn)),
      };
    },
  ],
  [
    "GET",
    "/sessions/:id/inbox/messages",
    ({ device, params: [id] }) => ({ inbox_messages: findSession(device, id).inbox }),
  ],
  [
    "PUT",
    "/sessions/:id/inbox/messages/:message",
    ({ device, params: [id, messageId], body }) => ({
      inbox_message: submitInboxMessage(device, findSession(device, id), messageId, body),
    }),
  ],
  [
    "GET",
    "/sessions/:id/inbox/messages/:message",
    ({ device, params: [id, messageId] }) => ({
      inbox_message: findInboxMessage(findSession(device, id), messageId),
    }),
  ],
  [
    "POST",
    "/sessions/:id/inbox/messages/:message/retry",
    ({ device, params: [id, originalId], body }) => {
      const session = findSession(device, id);
      const original = findInboxMessage(session, originalId);
      const retry = submitInboxMessage(device, session, String(body.message_id), {
        input: original.input.summary,
        delivery_policy: original.delivery_policy,
        metadata: original.metadata,
      });
      Object.assign(original, { retried_by_message_id: retry.message_id, updated_at: now() });
      retry.retry_of_message_id = original.message_id;
      return { inbox_message: retry };
    },
  ],
  ...(["cancel", "dismiss"] as const).map((action): [string, string, Handler] => [
    "POST",
    `/sessions/:id/inbox/messages/:message/${action}`,
    ({ device, params: [id, messageId] }) => {
      const message = findInboxMessage(findSession(device, id), messageId);
      const timestamp = now();
      Object.assign(message, {
        state: action === "cancel" ? "cancelled" : "dismissed",
        updated_at: timestamp,
        ...(action === "cancel" ? { cancelled_at: timestamp } : {}),
      });
      return { inbox_message: message };
    },
  ]),
  [
    "GET",
    "/sessions/:id/interactions",
    ({ device, params: [id], query }) => ({
      interactions: findSession(device, id).interactions.filter(
        (interaction) => !query.has("state") || interaction.state === query.get("state"),
      ),
      next_cursor: null,
    }),
  ],
  [
    "GET",
    "/sessions/:id/interactions/:interaction",
    ({ device, params: [id, interactionId] }) => ({
      interaction:
        findSession(device, id).interactions.find(
          (interaction) => interaction.interaction_id === interactionId,
        ) ?? notFound("Interaction"),
    }),
  ],
  [
    "PUT",
    "/sessions/:id/interactions/:interaction/response",
    ({ device, params: [id, interactionId], body }) => {
      const session = findSession(device, id);
      const interaction =
        session.interactions.find((item) => item.interaction_id === interactionId) ??
        notFound("Interaction");
      const optionKey = String(body.option_key);
      Object.assign(interaction, {
        state: "resolved",
        response: { option_key: optionKey },
        resolved_at: now(),
      });
      broadcast(device, {
        kind: "interaction_changed",
        session_id: id,
        interaction_id: interactionId,
      });
      const turn = runningTurn(session);
      if (turn) {
        const label = interaction.options.find((option) => option.key === optionKey)?.label;
        device.timers.set(
          id,
          setTimeout(
            () => completeTurn(device, session, turn, `You chose **${label}**. Continuing.`),
            1_000,
          ),
        );
        touch(session, "busy");
      }
      return { interaction };
    },
  ],
  [
    "POST",
    "/sessions/:id/interrupt",
    ({ device, params: [id] }) => {
      interrupt(device, findSession(device, id));
      return { accepted: true };
    },
  ],
  [
    "POST",
    "/sessions/:id/resume",
    ({ device, params: [id] }) => {
      const session = findSession(device, id);
      touch(session, "idle");
      emit(device, session, "session.ready", null);
      return { accepted: true };
    },
  ],
  [
    "POST",
    "/sessions/:id/exit",
    ({ device, params: [id] }) => {
      const session = findSession(device, id);
      interrupt(device, session);
      touch(session, "exited");
      emit(device, session, "session.exited", null);
      return { accepted: true };
    },
  ],
  [
    "POST",
    "/sessions/:id/compact",
    ({ device, params: [id] }) => {
      const session = findSession(device, id);
      const operationId = device.data.nextId("op");
      const tokensBefore = session.view.context_usage?.used_tokens ?? 0;
      emit(device, session, "session.compaction_started", null, { operation_id: operationId });
      setTimeout(() => {
        const tokensAfter = Math.round(tokensBefore / 4);
        session.view.context_usage = contextUsage(tokensAfter);
        touch(session);
        emit(device, session, "session.compaction_completed", null, {
          operation_id: operationId,
          tokens_before: tokensBefore,
          tokens_after: tokensAfter,
        });
      }, 1_500);
      return { accepted: true, operation_id: operationId };
    },
  ],
  [
    "GET",
    "/sessions/:id/models",
    ({ device, params: [id] }) => ({
      models: device.data.models,
      current_model: findSession(device, id).view.model,
      runtime_id: "runtime_mock",
    }),
  ],
  [
    "PATCH",
    "/sessions/:id/model",
    ({ device, params: [id], body }) => {
      const session = findSession(device, id);
      session.view.model = String(body.model);
      session.view.metadata = { ...session.view.metadata, model: session.view.model };
      touch(session);
      emit(device, session, "session.model_updated", null, { model: session.view.model });
      return { accepted: true };
    },
  ],
  [
    "GET",
    "/sessions/:id/files",
    ({ device, params: [id] }) => {
      findSession(device, id);
      return { files: [], next_cursor: null };
    },
  ],
];

const compiledRoutes = routes.map(([method, pattern, handler]) => ({
  method,
  regex: new RegExp(`^${pattern.replace(/:[a-z]+/g, "([^/]+)")}$`),
  handler,
}));

/** Answers one request for `/api/v1{path}` the way pontiad would. */
export async function handleDeviceRequest(
  device: MockDevice,
  request: Request,
  path: string,
): Promise<Response> {
  const url = new URL(request.url);
  if (request.method === "GET" && path === "/dashboard/events/stream") {
    return eventStream(device, request.signal);
  }
  if (path.startsWith("/uploads")) {
    return json(501, {
      data: null,
      meta: {},
      error: { code: "mock_unsupported", message: "Uploads are not available in the mock API." },
    });
  }
  for (const route of compiledRoutes) {
    if (route.method !== request.method) continue;
    const match = route.regex.exec(path);
    if (!match) continue;
    try {
      const text = await request.text();
      const result = route.handler({
        device,
        params: match.slice(1).map(decodeURIComponent),
        query: url.searchParams,
        body: text ? (JSON.parse(text) as JsonObject) : {},
      });
      if (result instanceof Unwrapped) return json(200, result.body);
      return json(200, { data: result, meta: {}, error: null });
    } catch (error) {
      if (!(error instanceof HttpError)) throw error;
      return json(error.status, {
        data: null,
        meta: {},
        error: { code: error.code, message: error.message },
      });
    }
  }
  return json(404, {
    data: null,
    meta: {},
    error: { code: "not_found", message: `No mock route for ${request.method} ${path}` },
  });
}
