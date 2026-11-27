import type {
  AgentProfileView,
  DashboardStreamEvent,
  EventView,
  JsonObject,
  MessageDelivery,
  QueuedMessageView,
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
  authProbed: boolean;
}

export function createMockDevice(data: MockData): MockDevice {
  return { data, streams: new Set(), timers: new Map(), authProbed: false };
}

const messageDeliveries = new WeakMap<MockSession, Map<string, MessageDelivery>>();

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
  headers: Headers;
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

function startTurn(device: MockDevice, session: MockSession, input: string) {
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
    metadata: {},
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
  emit(device, session, "turn.started", turn.turn_id, {});
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
  dispatchNextQueuedMessage(device, session);
}

function dispatchNextQueuedMessage(device: MockDevice, session: MockSession): void {
  const next = session.queue[0];
  if (!next) return;
  dispatch(device, session, next);
}

function dispatch(device: MockDevice, session: MockSession, message: QueuedMessageView): void {
  session.queue = session.queue.filter((item) => item.message_id !== message.message_id);
  startTurn(device, session, message.input);
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

const mockGitChanges = [
  {
    path: "src/lib/store.ts",
    group: "staged",
    status: "M",
    additions: 2,
    deletions: 1,
    binary: false,
  },
  {
    path: "src/app.css",
    group: "unstaged",
    status: "M",
    additions: 8,
    deletions: 3,
    binary: false,
  },
  {
    path: "src/routes/+page.svelte",
    group: "unstaged",
    status: "M",
    additions: 14,
    deletions: 6,
    binary: false,
  },
  {
    path: "src/lib/legacy.ts",
    group: "unstaged",
    status: "D",
    additions: 0,
    deletions: 22,
    binary: false,
  },
  { path: "notes.md", group: "untracked", status: "?", additions: 5, deletions: 0, binary: false },
];

const mockGitBranches = [
  { name: "main", ref: "refs/heads/main", remote: false, current: true },
  { name: "feature/history", ref: "refs/heads/feature/history", remote: false, current: false },
  { name: "origin/main", ref: "refs/remotes/origin/main", remote: true, current: false },
];

const mockGitSubjects = [
  "Show commit history in the Git tab",
  "Read branches without checking them out",
  "Merge branch 'feature/history'",
  "Fix line numbers in long diffs",
  "Group change list by status",
  "Add Git status to the composer",
  "Initial commit",
];

/** A short history per branch; every commit changes the same few files. */
function mockGitCommits(ref: string) {
  const offset = ref.includes("feature") ? 1 : 0;
  return mockGitSubjects.slice(offset).map((subject, index) => {
    const sha = (index + offset + 1).toString(16).padStart(2, "0").repeat(20);
    const parent = (index + offset + 2).toString(16).padStart(2, "0").repeat(20);
    return {
      sha,
      pushed: ref.startsWith("refs/remotes/") || index >= 2,
      parents: subject.startsWith("Merge")
        ? [parent, "ff".repeat(20)]
        : subject === "Initial commit"
          ? []
          : [parent],
      author_name: index % 2 ? "Grace Hopper" : "Ada Lovelace",
      authored_at: new Date(Date.now() - (index + 1) * 5 * 3_600_000).toISOString(),
      subject,
    };
  });
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
    initiator_session_id: detail.initiator_session_id,
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

function profileFromInput(input: UpsertAgentProfileInput): AgentProfileView {
  const timestamp = now();
  return {
    description: null,
    client_type: null,
    client_key: null,
    model: null,
    reasoning_effort: null,
    use_yolo: false,
    system_prompt: null,
    ...input,
    created_at: timestamp,
    updated_at: timestamp,
  };
}

function findProfile(device: MockDevice, profileId: string) {
  return (
    device.data.agentProfiles.find((profile) => profile.profile_id === profileId) ??
    notFound("Agent profile")
  );
}

function findAgentClient(device: MockDevice, clientType: string, clientKey: string) {
  return (
    device.data.agentClients.find(
      (client) => client.client_type === clientType && client.client_key === clientKey,
    ) ?? notFound("Agent client")
  );
}

function mockClientAuth(client: ReturnType<typeof findAgentClient>) {
  const managed = client.client_type === "codex" || client.client_key === "grok";
  return {
    state: client.client_key === "antigravity" ? "unknown" : "unauthenticated",
    checked_at: now(),
    methods: managed
      ? [
          {
            id: "device_code",
            label: `${client.name} Account`,
            kind: "device_code",
            recommended: true,
            remote_compatible: true,
            start_fields: [],
          },
        ]
      : [],
    supports_logout: false,
  };
}

function findQueuedMessage(session: MockSession, messageId: string): QueuedMessageView {
  return (
    session.queue.find((message) => message.message_id === messageId) ?? notFound("Queued message")
  );
}

function createQueuedMessage(
  session: MockSession,
  messageId: string,
  input: string,
  metadata: JsonObject,
): QueuedMessageView {
  const timestamp = now();
  const message: QueuedMessageView = {
    message_id: messageId,
    session_id: session.view.session_id,
    input,
    metadata,
    created_at: timestamp,
  };
  session.queue.push(message);
  return message;
}

function sendMessage(
  device: MockDevice,
  session: MockSession,
  messageId: string,
  body: JsonObject,
): { status: "delivered" } | { status: "queued"; message_id: string } {
  const mode = typeof body.mode === "string" ? body.mode : "immediate";
  const message = createQueuedMessage(
    session,
    messageId,
    String(body.input ?? ""),
    (body.metadata as JsonObject | undefined) ?? {},
  );
  if (mode === "interrupt") interrupt(device, session);
  if (!runningTurn(session) || (mode === "immediate" && session.view.capabilities.steer)) {
    dispatch(device, session, message);
    return { status: "delivered" };
  }
  return { status: "queued", message_id: messageId };
}

function sendMessageOnce(
  device: MockDevice,
  session: MockSession,
  messageId: string,
  body: JsonObject,
): MessageDelivery {
  let deliveries = messageDeliveries.get(session);
  if (!deliveries) {
    deliveries = new Map();
    messageDeliveries.set(session, deliveries);
  }
  const previous = deliveries.get(messageId);
  if (previous) return previous;
  const delivery = sendMessage(device, session, messageId, body);
  deliveries.set(messageId, delivery);
  return delivery;
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
  [
    "GET",
    "/memory/user",
    ({ device }) => ({ user: { ...device.data.memory.user, character_limit: 2000 } }),
  ],
  [
    "PUT",
    "/memory/user",
    ({ device, body }) => {
      const user = device.data.memory.user;
      const text = String(body.text);
      if (text !== user.text) {
        if (body.previous_text !== user.text)
          throw new HttpError(409, "state_conflict", "USER.md changed since it was read");
        if ([...text].length > 2000)
          throw new HttpError(400, "invalid_request", "USER.md exceeds 2000 characters");
        device.data.memory.user = { text, updated_at: new Date().toISOString() };
      }
      return { user: { ...device.data.memory.user, character_limit: 2000 } };
    },
  ],
  [
    "GET",
    "/memories",
    ({ device, query }) => {
      const q = query.get("q")?.trim().toLowerCase() ?? "";
      const memories = device.data.memory.memories
        .filter((memory) => !memory.archived)
        .filter((memory) => !q || memory.content.toLowerCase().includes(q))
        .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
      return { memories, total: memories.length, next_cursor: null };
    },
  ],
  ["GET", "/auth/validate", () => ({ valid: true })],

  ["GET", "/agent-client-setup", ({ device }) => ({ operation: device.data.clientSetup })],
  ["GET", "/agent-client-operations/latest", () => ({ operation: null })],
  [
    "GET",
    "/agent-clients/auth-probe",
    ({ device }) => {
      const firstProbe = !device.authProbed;
      device.authProbed = true;
      return {
        auth_probe: {
          first_probe: firstProbe,
          agent_clients: firstProbe
            ? device.data.agentClients.map((client) => ({
                client_type: client.client_type,
                client_key: client.client_key,
                auth: mockClientAuth(client),
              }))
            : [],
        },
      };
    },
  ],
  [
    "GET",
    "/agent-clients/:type/:key/auth",
    ({ device, params: [type, key] }) => ({
      auth: mockClientAuth(findAgentClient(device, type, key)),
    }),
  ],
  [
    "POST",
    "/agent-clients/:type/:key/setup",
    ({ device, params: [type, key] }) => {
      const client = findAgentClient(device, type, key);
      if (!client.setup_command) throw new Error("Setup is unavailable for this client");
      client.enabled = true;
      client.setup = { status: "initialized" };
      device.data.clientSetup = {
        id: crypto.randomUUID(),
        client_type: type,
        client_key: key,
        status: "succeeded",
        error: null,
        output: "Demo setup completed.",
      };
      return { operation: device.data.clientSetup };
    },
  ],
  ["GET", "/agent-clients", ({ device }) => ({ agent_clients: device.data.agentClients })],
  [
    "PATCH",
    "/agent-clients/:type/:key",
    ({ device, params: [type, key], body }) => {
      const client = findAgentClient(device, type, key);
      if (Object.keys(body).some((key) => key !== "enabled")) {
        throw new HttpError(400, "invalid_request", "Unknown client setting");
      }
      if (typeof body.enabled !== "boolean") {
        throw new HttpError(400, "invalid_request", "enabled must be a boolean");
      }
      client.enabled = body.enabled;
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
        enabled: false,
        version: null,
        version_checked_at: null,
        has_record: false,
        created_at: null,
        updated_at: null,
      });
      return { reset: true };
    },
  ],
  [
    "GET",
    "/agent-clients/:type/:key/models",
    ({ device, params: [type, key] }) => ({
      version:
        device.data.agentClients.find(
          (client) => client.client_type === type && client.client_key === key,
        )?.version ?? null,
      fetched_at: Date.now(),
      models: [...device.data.models, ...(device.data.clientModels[type] ?? [])].map(
        (model, index) => ({
          ...model,
          reasoning_efforts:
            type === "pi"
              ? ["off", "low", "medium", "high"].map((id) => ({ id, name: id, description: "" }))
              : [],
          default_reasoning_effort: type === "pi" && index === 0 ? "medium" : null,
        }),
      ),
      default_model: device.data.models[0]?.id ?? null,
    }),
  ],
  [
    "GET",
    "/session-creation-options",
    ({ device }) => ({
      options: device.data.creationOptions.flatMap((option) => {
        const client = device.data.agentClients.find(
          (entry) =>
            entry.enabled &&
            entry.client_type === option.client_type &&
            (entry.client_type !== "acp" || entry.client_key === option.metadata.acp_agent_id),
        );
        return client ? [{ ...option, version: client.version }] : [];
      }),
    }),
  ],

  ["GET", "/agent-profiles", ({ device }) => ({ agent_profiles: device.data.agentProfiles })],
  [
    "GET",
    "/agent-profiles/:id",
    ({ device, params: [profileId] }) => ({ agent_profile: findProfile(device, profileId) }),
  ],
  [
    "POST",
    "/agent-profiles",
    ({ device, body }) => {
      const input = body as unknown as UpsertAgentProfileInput;
      if (device.data.agentProfiles.some((profile) => profile.profile_id === input.profile_id))
        throw new HttpError(409, "conflict", "Profile already exists");
      const profile = profileFromInput(input);
      device.data.agentProfiles.push(profile);
      return { agent_profile: profile };
    },
  ],
  [
    "PUT",
    "/agent-profiles/:id",
    ({ device, params: [profileId], body }) => {
      const profile = findProfile(device, profileId);
      Object.assign(profile, profileFromInput(body as unknown as UpsertAgentProfileInput), {
        profile_id: profileId,
        created_at: profile.created_at,
      });
      return { agent_profile: profile };
    },
  ],
  [
    "DELETE",
    "/agent-profiles/:id",
    ({ device, params: [profileId] }) => {
      findProfile(device, profileId);
      device.data.agentProfiles = device.data.agentProfiles.filter(
        (profile) => profile.profile_id !== profileId,
      );
      return { profile_id: profileId };
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
        availability: { status: "available", root_id: device.data.roots[0].root_id, path },
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
    "GET",
    "/workspaces/:id/git/changes",
    ({ device, params: [id] }) => ({
      changes: {
        repo_root: findWorkspace(device, id).canonical_path,
        total: mockGitChanges.length,
        truncated: false,
        files: mockGitChanges,
      },
    }),
  ],
  [
    "GET",
    "/workspaces/:id/git/diff",
    ({ query }) => ({
      diff: {
        path: query.get("path"),
        group: query.get("group"),
        binary: false,
        truncated: false,
        hunks: [
          {
            header: "@@ -1,3 +1,4 @@",
            lines: [
              { kind: "context", old: 1, new: 1, text: "import { writable } from 'svelte/store'" },
              { kind: "delete", old: 2, new: null, text: "const open = writable(false)" },
              { kind: "add", old: null, new: 2, text: "const open = writable(true)" },
              { kind: "add", old: null, new: 3, text: "export { open }" },
              { kind: "context", old: 3, new: 4, text: "" },
            ],
          },
        ],
      },
    }),
  ],
  [
    "GET",
    "/workspaces/:id/git/branches",
    () => ({
      branches: {
        head: "refs/heads/main",
        branches: mockGitBranches,
      },
    }),
  ],
  [
    "GET",
    "/workspaces/:id/git/commits",
    ({ query }) => {
      const commits = mockGitCommits(query.get("ref") ?? "HEAD");
      const skip = Number(query.get("skip") ?? 0);
      return { commits: { commits: commits.slice(skip, skip + 50), has_more: false } };
    },
  ],
  [
    "GET",
    "/workspaces/:id/git/commits/:sha",
    ({ params: [, sha] }) => {
      const summary = [
        ...mockGitCommits("refs/heads/main"),
        ...mockGitCommits("refs/heads/feature"),
      ].find((commit) => commit.sha === sha);
      if (!summary) notFound(`commit ${sha}`);
      const files = mockGitChanges
        .filter((file) => file.group !== "untracked")
        .map(({ path, status, additions, deletions, binary }) => ({
          path,
          status,
          additions,
          deletions,
          binary,
        }));
      return {
        commit: {
          ...summary,
          author_email: "dev@example.com",
          committer_name: summary.author_name,
          committed_at: summary.authored_at,
          body: "Lists commits of any branch and opens each commit's file changes.\nNothing is checked out.",
          files,
          total: files.length,
          truncated: false,
        },
      };
    },
  ],
  [
    "GET",
    "/workspaces/:id/git/commits/:sha/diff",
    ({ params: [, sha], query }) => ({
      diff: {
        sha,
        path: query.get("path"),
        binary: false,
        truncated: false,
        hunks: [
          {
            header: "@@ -1,3 +1,4 @@",
            lines: [
              { kind: "context", old: 1, new: 1, text: "import { writable } from 'svelte/store'" },
              { kind: "delete", old: 2, new: null, text: "const open = writable(false)" },
              { kind: "add", old: null, new: 2, text: "const open = writable(true)" },
              { kind: "add", old: null, new: 3, text: "export { open }" },
              { kind: "context", old: 3, new: 4, text: "" },
            ],
          },
        ],
      },
    }),
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
      const model = String(body.model ?? device.data.models[0].id);
      const session: MockSession = {
        view: {
          session_id: device.data.nextId("sess"),
          client_type: String(body.client_type),
          title: (body.title as string | null) ?? initialTask?.input.slice(0, 60) ?? null,
          handle: null,
          role: null,
          description: null,
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
          },
          timeline_unavailable_reason: null,
          model,
          reasoning_effort:
            body.client_type === "pi" ? String(body.reasoning_effort ?? "medium") : null,
          context_usage: contextUsage(6_000),
          lineage: null,
          created_at: timestamp,
          updated_at: timestamp,
          metadata: { ...option?.metadata, model },
        },
        turns: [],
        items: new Map(),
        queue: [],
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
  ...(["pin", "unpin", "archive", "unarchive"] as const).map(
    (action): [string, string, Handler] => [
      "POST",
      `/sessions/:id/${action}`,
      ({ device, params: [id] }) => {
        const session = findSession(device, id);
        if (action === "pin") session.view.pinned_at = now();
        if (action === "unpin") session.view.pinned_at = null;
        if (action === "archive") {
          session.view.archived_at = now();
          session.view.pinned_at = null;
        }
        if (action === "unarchive") session.view.archived_at = null;
        touch(session);
        emit(
          device,
          session,
          `session.${action.endsWith("archive") ? action + "d" : action + "ned"}`,
          null,
        );
        return { session: session.view };
      },
    ],
  ),
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
    "/sessions/:id/queue",
    ({ device, params: [id] }) => ({ messages: findSession(device, id).queue }),
  ],
  [
    "POST",
    "/sessions/:id/messages",
    ({ device, params: [id], body, headers }) => ({
      delivery: sendMessageOnce(
        device,
        findSession(device, id),
        headers.get("Idempotency-Key") ?? device.data.nextId("msg"),
        body,
      ),
    }),
  ],
  [
    "DELETE",
    "/sessions/:id/queue/:message",
    ({ device, params: [id, messageId] }) => {
      const session = findSession(device, id);
      const message = findQueuedMessage(session, messageId);
      session.queue = session.queue.filter((item) => item.message_id !== messageId);
      return { message };
    },
  ],
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
        headers: request.headers,
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
