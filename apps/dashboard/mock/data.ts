import type {
  AgentClientView,
  AgentProfileView,
  EventView,
  InboxMessageView,
  Interaction,
  ManagedToolUseInput,
  SessionCreationOption,
  SessionModel,
  SessionView,
  TurnState,
  TurnTimelineItem,
  TurnView,
  WorkflowDetailView,
  WorkflowGraphRevisionView,
  WorkflowPatchHistoryView,
  WorkspaceRootView,
  WorkspaceView,
} from "../src/api/types.ts";

export interface MockSession {
  view: SessionView;
  turns: TurnView[];
  items: Map<string, TurnTimelineItem[]>;
  inbox: InboxMessageView[];
  events: EventView[];
  interactions: Interaction[];
}

export interface MockWorkflow {
  detail: WorkflowDetailView;
  revisions: WorkflowGraphRevisionView[];
  patches: WorkflowPatchHistoryView[];
  documents: Record<string, string>;
}

export interface MockData {
  roots: WorkspaceRootView[];
  directories: Record<string, string[]>;
  workspaces: WorkspaceView[];
  sessions: MockSession[];
  workflows: MockWorkflow[];
  agentProfiles: AgentProfileView[];
  agentClients: AgentClientView[];
  creationOptions: SessionCreationOption[];
  models: SessionModel[];
  files: string[];
  nextId: (prefix: string) => string;
}

type Step = { thinking: string } | { tool: ManagedToolUseInput; result: string; failed?: boolean };

interface TurnSpec {
  user: string;
  steps?: Step[];
  reply?: string;
  state?: TurnState;
}

interface SessionSpec {
  title: string | null;
  client: "pi" | "codex" | "claude-agent";
  workspace: WorkspaceView;
  state: SessionView["state"];
  updatedMinutesAgo: number;
  turns: TurnSpec[];
  pinned?: boolean;
  archived?: boolean;
  interaction?: { title: string; description: string };
}

const models: SessionModel[] = [
  { id: "anthropic/claude-sonnet-4-5", name: "Claude Sonnet 4.5", description: "anthropic" },
  { id: "anthropic/claude-opus-4-1", name: "Claude Opus 4.1", description: "anthropic" },
  { id: "openai/gpt-5-codex", name: "GPT-5 Codex", description: "openai" },
];

const creationOptions: SessionCreationOption[] = [
  { id: "pi", name: "Pi", client_type: "pi", metadata: {} },
  { id: "codex", name: "Codex", client_type: "codex", metadata: {} },
  { id: "claude-agent", name: "Claude Agent", client_type: "claude-agent", metadata: {} },
  {
    id: '["acp","grok"]',
    name: "Grok Build",
    client_type: "acp",
    metadata: { acp_agent_id: "grok" },
  },
];

function ago(minutes: number): string {
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

function createIdSource(): (prefix: string) => string {
  let counter = 0;
  // Zero-padded counters keep IDs ordered by creation, like the UUIDv7 IDs pontiad issues.
  return (prefix) => `${prefix}_mock${String(++counter).padStart(6, "0")}`;
}

function workspace(path: string, minutesAgo: number): WorkspaceView {
  const name = path.split("/").at(-1) ?? path;
  return {
    workspace_id: `wks_mock_${name}`,
    canonical_path: path,
    display_path: path.replace(/^\/home\/dev/, "~"),
    name,
    state: "active",
    metadata: {},
    created_at: ago(60 * 24 * 30),
    updated_at: ago(minutesAgo),
    last_used_at: ago(minutesAgo),
  };
}

function capabilities(): SessionView["capabilities"] {
  return {
    accept_task: true,
    interrupt: true,
    stream_output: false,
    heartbeat: false,
    timeline: true,
    topology: true,
    branch_control: false,
    list_models: true,
    set_model: true,
    manual_compaction: true,
    compaction_events: true,
    compaction_history: false,
    context_usage: "estimated",
  };
}

export function contextUsage(usedTokens: number): SessionView["context_usage"] {
  const maxTokens = 200_000;
  return {
    used_tokens: usedTokens,
    max_tokens: maxTokens,
    remaining_tokens: maxTokens - usedTokens,
    usage_ratio: usedTokens / maxTokens,
    input_tokens: Math.round(usedTokens * 0.95),
    output_tokens: Math.round(usedTokens * 0.05),
    cache_tokens: 0,
    confidence: "estimated",
    observed_at: new Date().toISOString(),
  };
}

export function timelineItem(
  nextId: (prefix: string) => string,
  turnId: string,
  occurredAt: string,
  item: Pick<TurnTimelineItem, "kind" | "content_preview"> & Partial<TurnTimelineItem>,
): TurnTimelineItem {
  return {
    item_id: nextId("item"),
    raw_kind: item.kind,
    role: item.kind === "user" ? "user" : "assistant",
    title: null,
    status: null,
    occurred_at: occurredAt,
    turn_id: turnId,
    ...item,
  };
}

function stepItems(
  nextId: (prefix: string) => string,
  turnId: string,
  occurredAt: string,
  step: Step,
): TurnTimelineItem[] {
  if ("thinking" in step) {
    return [
      timelineItem(nextId, turnId, occurredAt, {
        kind: "thinking",
        content_preview: step.thinking,
      }),
    ];
  }
  const callId = nextId("call");
  const { type: toolName, ...args } = step.tool;
  return [
    timelineItem(nextId, turnId, occurredAt, {
      kind: "tool_call",
      role: "tool",
      title: toolName,
      status: "started",
      call_id: callId,
      content_preview: `${toolName} ${JSON.stringify(args)}`,
      managed_tool_use: { tool_name: toolName, input: step.tool },
    }),
    timelineItem(nextId, turnId, occurredAt, {
      kind: "tool_result",
      role: "tool",
      title: toolName,
      status: step.failed ? "failed" : "completed",
      call_id: callId,
      content_preview: step.result,
    }),
  ];
}

function buildSession(nextId: (prefix: string) => string, spec: SessionSpec): MockSession {
  const sessionId = nextId("sess");
  const createdMinutesAgo = spec.updatedMinutesAgo + spec.turns.length * 6 + 5;
  const turns: TurnView[] = [];
  const items = new Map<string, TurnTimelineItem[]>();
  const events: EventView[] = [];

  spec.turns.forEach((turnSpec, index) => {
    const turnId = nextId("turn");
    const startedAt = ago(createdMinutesAgo - index * 6);
    const state = turnSpec.state ?? "completed";
    const finished = state !== "running" && state !== "queued";
    const completedAt = finished ? ago(createdMinutesAgo - index * 6 - 2) : null;
    turns.push({
      turn_id: turnId,
      session_id: sessionId,
      parent_turn_id: turns.at(-1)?.turn_id ?? null,
      topology_status: turns.length ? "linked" : "root",
      state,
      input: { summary: turnSpec.user },
      output: turnSpec.reply ? { summary: turnSpec.reply.slice(0, 200) } : null,
      failure: state === "failed" ? { message: "Agent process exited with status 1." } : null,
      created_at: startedAt,
      started_at: startedAt,
      completed_at: completedAt,
      metadata: {},
    });
    const turnItems = [
      timelineItem(nextId, turnId, startedAt, { kind: "user", content_preview: turnSpec.user }),
      ...(turnSpec.steps ?? []).flatMap((step) => stepItems(nextId, turnId, startedAt, step)),
    ];
    if (turnSpec.reply) {
      turnItems.push(
        timelineItem(nextId, turnId, completedAt ?? startedAt, {
          kind: "assistant",
          content_preview: turnSpec.reply,
        }),
      );
    }
    items.set(turnId, turnItems);
    events.push(sessionEvent(nextId, sessionId, turnId, "turn.started", startedAt, {}));
    if (completedAt) {
      events.push(sessionEvent(nextId, sessionId, turnId, `turn.${state}`, completedAt, {}));
    }
  });

  const currentTurn = turns.at(-1);
  const model = spec.client === "codex" ? models[2].id : models[0].id;
  const view: SessionView = {
    session_id: sessionId,
    client_type: spec.client,
    title: spec.title,
    handle: null,
    role: null,
    description: null,
    execution_profile_id: null,
    execution_profile_version: null,
    state: spec.state,
    current_turn_id: currentTurn?.turn_id ?? null,
    workspace_id: spec.workspace.workspace_id,
    workspace: spec.workspace.canonical_path,
    pinned_at: spec.pinned ? ago(spec.updatedMinutesAgo) : null,
    archived_at: spec.archived ? ago(spec.updatedMinutesAgo) : null,
    capabilities: capabilities(),
    timeline_unavailable_reason: null,
    model,
    model_control_unavailable_reason: null,
    context_usage: contextUsage(8_000 + spec.turns.length * 9_000),
    lineage: null,
    created_at: ago(createdMinutesAgo),
    updated_at: ago(spec.updatedMinutesAgo),
    metadata: { model },
  };

  const interactions: Interaction[] = spec.interaction
    ? [
        {
          interaction_id: nextId("int"),
          session_id: sessionId,
          turn_id: currentTurn?.turn_id ?? null,
          type: "choice",
          state: "pending",
          title: spec.interaction.title,
          description: spec.interaction.description,
          options: [
            { key: "approve", label: "Approve", intent: "positive" },
            { key: "reject", label: "Reject", intent: "negative" },
          ],
          response: null,
          requested_at: ago(spec.updatedMinutesAgo),
          resolved_at: null,
        },
      ]
    : [];

  return { view, turns, items, inbox: [], events, interactions };
}

export function sessionEvent(
  nextId: (prefix: string) => string,
  sessionId: string,
  turnId: string | null,
  type: string,
  time: string,
  payload: EventView["payload"],
): EventView {
  return {
    event_id: nextId("evt"),
    session_id: sessionId,
    turn_id: turnId,
    source: "agent_client",
    type,
    time,
    payload,
  };
}

function agentClient(
  clientType: string,
  clientKey: string,
  name: string,
  command: string,
): AgentClientView {
  return {
    id: btoa(JSON.stringify([clientType, clientKey])).replace(/=+$/, ""),
    client_type: clientType,
    client_key: clientKey,
    name,
    short_id: clientKey,
    enabled: true,
    has_record: false,
    config_overrides: {},
    effective_config: { name, command, args: [], env: {} },
    created_at: null,
    updated_at: null,
  };
}

function agentProfile(
  profileId: string,
  version: string,
  name: string,
  description: string,
  metadata: AgentProfileView["metadata"] = {},
): AgentProfileView {
  return {
    profile_id: profileId,
    version,
    name,
    description,
    supported_client_types: ["pi", "codex", "claude-agent"],
    agent_kind: "executor",
    system_prompt_template: `You are ${name}. ${description}`,
    turn_prompt_template: "{{input}}",
    default_session_role: null,
    default_session_description: null,
    handle_prefix: null,
    expected_output_schema: null,
    artifact_contract: {},
    default_execution_policy: {},
    default_review_policy: {},
    metadata,
    active: true,
    archived_at: null,
    archived_reason: null,
    created_at: ago(60 * 24 * 10),
    updated_at: ago(60 * 24 * 2),
  };
}

function workflow(
  nextId: (prefix: string) => string,
  spec: {
    title: string;
    state: WorkflowDetailView["state"];
    cwd: string;
    startedMinutesAgo: number;
    phases: { name: string; nodes: { title: string; status: string }[] }[];
    failure?: string;
  },
): MockWorkflow {
  const workflowId = nextId("wf");
  const graphNodes: WorkflowGraphRevisionView["nodes"] = [];
  const nodes: WorkflowDetailView["nodes"] = [];
  for (const phase of spec.phases) {
    for (const node of phase.nodes) {
      const nodeId = nextId("node");
      const submitted = node.status === "submitted";
      nodes.push({
        node_id: nodeId,
        phase: phase.name,
        title: node.title,
        status: node.status as WorkflowDetailView["nodes"][number]["status"],
        session_id: node.status === "pending" ? null : nextId("sess"),
        session_state: submitted ? "exited" : node.status === "running" ? "busy" : null,
        submitted_at: submitted ? ago(spec.startedMinutesAgo - 10) : null,
      });
      graphNodes.push({
        node_id: nodeId,
        parent_node_id: null,
        node_type: "agent",
        session_id: null,
        turn_ids: [],
        phase: phase.name,
        title: node.title,
        instructions: `Complete "${node.title}" and submit the result.`,
        inputs: [],
        output: "report.md",
        execution_profile_id: "executor",
        execution_profile_version: "1",
        introduced_revision: 1,
        retired_revision: null,
      });
    }
  }
  const finished = spec.state === "completed" || spec.state === "failed";
  const failureEventId = spec.state === "failed" ? nextId("evt") : null;
  return {
    detail: {
      retry_failure_event_id: failureEventId,
      retry_unavailable_reason: null,
      recoveries: [],
      workflow_id: workflowId,
      title: spec.title,
      state: spec.state,
      current_revision: 1,
      failure_message: spec.failure ?? null,
      cwd: spec.cwd,
      active_patch: null,
      agent_submitted_count: nodes.filter((node) => node.status === "submitted").length,
      agent_total_count: nodes.length,
      current_node_id:
        nodes.find((node) => node.status === "running" || node.status === "failed")?.node_id ??
        null,
      started_at: ago(spec.startedMinutesAgo),
      completed_at: finished ? ago(spec.startedMinutesAgo - 40) : null,
      created_at: ago(spec.startedMinutesAgo + 1),
      updated_at: ago(finished ? spec.startedMinutesAgo - 40 : 1),
      elapsed_ms: (finished ? 40 : spec.startedMinutesAgo) * 60_000,
      nodes,
    },
    revisions: [{ workflow_id: workflowId, revision: 1, current: true, nodes: graphNodes }],
    patches: [],
    documents: {},
  };
}

const sidebarReply = `I tightened the sidebar spacing and aligned the session rows with the device header.

**Changes**
- \`AppSidebar.svelte\`: reduced group padding from \`px-3\` to \`px-2\`.
- \`DeviceSessionList.svelte\`: rows now use a fixed \`h-8\` height so long titles truncate instead of wrapping.

\`\`\`svelte
<Sidebar.MenuButton class="h-8 truncate">{label}</Sidebar.MenuButton>
\`\`\`

\`bun run check\` passes.`;

const releaseNotes = `## Pontia 0.8.0

| Area | Change |
| --- | --- |
| Dashboard | Multi-device sidebar with per-device session lists |
| CLI | \`pontia doctor\` checks the local runtime and client hooks |
| Edge | Faster startup after updates |

### Upgrade notes
1. Restart \`pontiad\` after updating.
2. Re-run \`pontia client install\` for each agent client.

> The public dashboard now requires signing in again after the update.`;

export function createSampleData(): MockData {
  const nextId = createIdSource();
  const pontia = workspace("/home/dev/projects/pontia", 3);
  const website = workspace("/home/dev/projects/website", 90);
  const notes = workspace("/home/dev/notes", 60 * 24 * 3);

  const longTurns: TurnSpec[] = Array.from({ length: 14 }, (_, index) => ({
    user: `Step ${index + 1}: continue the refactor of the timeline store.`,
    steps: [{ thinking: `Looking at what remains after step ${index}.` }],
    reply: `Finished step ${index + 1}. Moved one more helper out of \`timeline.ts\` and kept its tests passing.`,
  }));

  const sessions = [
    buildSession(nextId, {
      title: "Refine sidebar spacing",
      client: "pi",
      workspace: pontia,
      state: "idle",
      updatedMinutesAgo: 3,
      pinned: true,
      turns: [
        {
          user: "The sidebar feels cramped on small screens. Can you tighten the spacing?",
          steps: [
            { thinking: "I should read the sidebar layout before changing spacing." },
            {
              tool: {
                type: "read",
                path: "apps/dashboard/src/components/layout/AppSidebar.svelte",
              },
              result:
                "<script lang=\"ts\">\n  import * as Sidebar from '$lib/components/ui/sidebar'\n</script>",
            },
            {
              tool: {
                type: "edit",
                path: "apps/dashboard/src/components/layout/AppSidebar.svelte",
                edits_count: 2,
              },
              result: "Successfully replaced 2 block(s).",
            },
            {
              tool: { type: "bash", command: "bun run check" },
              result: "svelte-check found 0 errors and 0 warnings",
            },
          ],
          reply: sidebarReply,
        },
        {
          user: "Looks good. Also make the device status label a bit more muted.",
          steps: [
            {
              tool: {
                type: "edit",
                path: "apps/dashboard/src/modes/public/DashboardModeLayout.svelte",
                edits_count: 1,
              },
              result: "Successfully replaced 1 block(s).",
            },
          ],
          reply: "Done. The status label now uses `text-muted-foreground` at `text-[10px]`.",
        },
      ],
    }),
    buildSession(nextId, {
      title: "Investigate flaky upload test",
      client: "codex",
      workspace: pontia,
      state: "busy",
      updatedMinutesAgo: 1,
      turns: [
        {
          user: "uploads.test.ts fails about one run in ten. Find out why.",
          steps: [
            { thinking: "The failure looks timing related. I'll run the test in a loop." },
            {
              tool: {
                type: "bash",
                command: "for i in $(seq 20); do bun run test tests/uploads.test.ts || break; done",
              },
              result:
                "FAIL tests/uploads.test.ts > resumes an interrupted upload\nAssertionError: expected 'pending' to be 'completed'",
              failed: true,
            },
          ],
          state: "running",
        },
      ],
    }),
    buildSession(nextId, {
      title: "Apply database migration",
      client: "claude-agent",
      workspace: pontia,
      state: "waiting_for_interaction",
      updatedMinutesAgo: 8,
      interaction: {
        title: "Run migration 0042_add_device_labels.sql?",
        description:
          "This adds a `label` column to the `devices` table in the local control-plane database.",
      },
      turns: [
        {
          user: "Add a label column to devices and apply the migration locally.",
          steps: [
            {
              tool: {
                type: "write",
                path: "crates/pontia-storage-sqlite/migrations/0042_add_device_labels.sql",
              },
              result: "Wrote 3 lines.",
            },
          ],
          state: "running",
        },
      ],
    }),
    buildSession(nextId, {
      title: "Draft release notes",
      client: "pi",
      workspace: website,
      state: "idle",
      updatedMinutesAgo: 45,
      turns: [
        {
          user: "Draft release notes for 0.8.0 from the changelog.",
          steps: [
            {
              tool: { type: "read", path: "CHANGELOG.md", start_line: 1, end_line: 80 },
              result: "## [0.8.0]\n- dashboard: multi-device sidebar\n- cli: doctor command",
            },
          ],
          reply: releaseNotes,
        },
      ],
    }),
    buildSession(nextId, {
      title: "Long refactor conversation",
      client: "pi",
      workspace: pontia,
      state: "idle",
      updatedMinutesAgo: 120,
      turns: longTurns,
    }),
    buildSession(nextId, {
      title: "Fix edge health check",
      client: "codex",
      workspace: pontia,
      state: "error",
      updatedMinutesAgo: 60 * 5,
      turns: [
        {
          user: "The edge health check times out after updates. Fix it.",
          steps: [{ thinking: "Checking the health endpoint handler." }],
          state: "failed",
        },
      ],
    }),
    buildSession(nextId, {
      title: null,
      client: "claude-agent",
      workspace: notes,
      state: "idle",
      updatedMinutesAgo: 60 * 20,
      turns: [],
    }),
    buildSession(nextId, {
      title: "Summarize meeting notes",
      client: "pi",
      workspace: notes,
      state: "exited",
      updatedMinutesAgo: 60 * 26,
      turns: [
        {
          user: "Summarize notes/2026-11-03.md in five bullets.",
          reply:
            "- Ship the multi-device sidebar\n- Move docs to the new site\n- Fix edge startup\n- Plan 0.9 workflows\n- Review pricing page",
        },
      ],
    }),
    buildSession(nextId, {
      title: "Old landing page experiment",
      client: "pi",
      workspace: website,
      state: "exited",
      updatedMinutesAgo: 60 * 24 * 7,
      archived: true,
      turns: [{ user: "Try a darker hero section.", reply: "Added a dark variant behind a flag." }],
    }),
  ];

  const workflows = [
    workflow(nextId, {
      title: "Add device labels end to end",
      state: "running",
      cwd: pontia.canonical_path,
      startedMinutesAgo: 25,
      phases: [
        {
          name: "Design",
          nodes: [{ title: "Write the spec", status: "submitted" }],
        },
        {
          name: "Implement",
          nodes: [
            { title: "Storage and migration", status: "submitted" },
            { title: "HTTP API", status: "running" },
            { title: "Dashboard UI", status: "pending" },
          ],
        },
        { name: "Review", nodes: [{ title: "Code review", status: "pending" }] },
      ],
    }),
    workflow(nextId, {
      title: "Translate docs to Chinese",
      state: "completed",
      cwd: website.canonical_path,
      startedMinutesAgo: 60 * 30,
      phases: [
        {
          name: "Translate",
          nodes: [
            { title: "Quick start", status: "submitted" },
            { title: "Configuration", status: "submitted" },
          ],
        },
      ],
    }),
    workflow(nextId, {
      title: "Upgrade Svelte dependencies",
      state: "failed",
      cwd: pontia.canonical_path,
      startedMinutesAgo: 60 * 6,
      failure: 'Agent for "Run tests" exited before submitting a result.',
      phases: [
        {
          name: "Upgrade",
          nodes: [
            { title: "Bump packages", status: "submitted" },
            { title: "Run tests", status: "failed" },
          ],
        },
      ],
    }),
  ];

  return {
    roots: [{ root_id: "home", label: "Home", canonical_path: "/home/dev", state: "available" }],
    directories: {
      "": ["projects", "notes", "downloads"],
      projects: ["pontia", "website", "playground"],
      "projects/playground": ["src", "tests"],
      "projects/pontia": ["apps", "crates", "docs"],
      "projects/website": ["src", "static"],
      notes: [],
    },
    workspaces: [pontia, website, notes],
    sessions,
    workflows,
    agentProfiles: [
      agentProfile("executor", "1", "Executor", "Implements a single task.", { builtin: true }),
      agentProfile("reviewer", "1", "Reviewer", "Reviews a change for bugs and style."),
      agentProfile("reviewer", "2", "Reviewer", "Reviews a change and checks it against the spec."),
    ],
    agentClients: [
      agentClient("pi", "pi", "Pi", "pi"),
      agentClient("codex", "codex", "Codex", "codex"),
      agentClient("claude-agent", "claude-agent", "Claude Agent", "claude"),
      agentClient("acp", "grok", "Grok Build", "grok"),
    ],
    creationOptions,
    models,
    files: [
      "README.md",
      "justfile",
      "apps/dashboard/src/app.css",
      "apps/dashboard/src/components/layout/AppSidebar.svelte",
      "apps/dashboard/src/pages/SessionChatPage.svelte",
      "crates/pontiad/src/main.rs",
    ],
    nextId,
  };
}

/** A device that has just been set up: no workspaces, sessions or workflows. */
export function createEmptyData(): MockData {
  const data = createSampleData();
  return { ...data, workspaces: [], sessions: [], workflows: [] };
}
