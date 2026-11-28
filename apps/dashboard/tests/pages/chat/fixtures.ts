import { testDevice } from "../../deviceFixture";
import { cleanup } from "@testing-library/svelte";
import { afterEach, beforeEach, vi } from "vitest";
import type { SessionConsoleDetail } from "../../../src/stores/sessions";
import type { TimelineState } from "../../../src/stores/timeline";
import type {
  CreateSessionResult,
  QueuedMessageView,
  SessionView,
  TimelineItem,
  TurnView,
  WorkspaceDirectoryListingView,
  WorkspaceRootView,
  WorkspaceView,
} from "../../../src/api/types";
const { chatDraft } = testDevice.chatDraft;
const { queryClient } = testDevice;
const { sessionKeys } = testDevice.sessionQueries;
import { resetChatEntryAutofocus } from "../../../src/lib/chatEntryAutofocus";

const mocks = vi.hoisted(() => {
  function writableStore<T>(initial: T) {
    let value = initial;
    const subscribers = new Set<(value: T) => void>();
    return {
      subscribe(run: (value: T) => void) {
        subscribers.add(run);
        run(value);
        return () => subscribers.delete(run);
      },
      set(next: T) {
        value = next;
        for (const run of subscribers) run(value);
      },
      get() {
        return value;
      },
    };
  }

  const timelineStateValue = (overrides: Partial<TimelineState> = {}): TimelineState => ({
    sessionId: "",
    mode: "linear",
    groups: [],
    items: [],
    nextOlderTurnId: null,
    latestTurnId: null,
    hasMore: false,
    loading: false,
    refreshing: false,
    refreshKind: null,
    status: "idle",
    errorCode: null,
    error: null,
    ...overrides,
  });

  const sessions = writableStore<SessionView[]>([]);
  const sessionDetail = writableStore<SessionConsoleDetail | null>(null);
  const sessionDetailLoading = writableStore(false);
  const sessionDetailError = writableStore<string | null>(null);
  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesInitialized = writableStore(true);
  const workspacesError = writableStore<string | null>(null);
  const workspaceRoots = writableStore<WorkspaceRootView[]>([]);
  const workspaceGitStatuses = writableStore({});
  const workspaceGitStatusErrors = writableStore({});
  const timelineState = writableStore<TimelineState>(timelineStateValue());
  const dashboardEventListeners = new Set<(event: unknown) => void>();
  const liveOutputListeners = new Map<
    string,
    {
      onEvent: (event: unknown) => void;
      onDisconnected: () => void;
    }
  >();

  return {
    sessions,
    sessionDetail,
    sessionDetailLoading,
    sessionDetailError,
    sessionDetailErrorKind: writableStore<string | null>(null),
    workspaces,
    workspacesLoading,
    workspacesInitialized,
    workspacesError,
    workspaceRoots,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    timelineState,
    timelineStateValue,
    dashboardEventListeners,
    liveOutputListeners,
    loadedSessions: [] as SessionView[],
    loadSessionDetail: vi.fn(async () => null),
    sendMessage: vi.fn(),
    submitBranchReplay: vi.fn(async () => ({ replay_id: "replay_test" })),
    cancelQueuedMessage: vi.fn(),
    resumeSession: vi.fn(),
    interruptSession: vi.fn(),
    terminateSession: vi.fn(),
    pinSession: vi.fn(),
    unpinSession: vi.fn(),
    archiveSession: vi.fn(),
    unarchiveSession: vi.fn(),
    updateSessionTitle: vi.fn(),
    setSessionReasoningEffort: vi.fn(),
    createSession: vi.fn(),
    loadSessionTimeline: vi.fn(async (_sessionId: string) => null),
    refreshSessionTimeline: vi.fn(async () => true),
    restoreSessionTimeline: vi.fn(async () => false),
    resetTimelineState: vi.fn((sessionId = "") => {
      mocks.timelineState.set(mocks.timelineStateValue({ sessionId }));
    }),
    loadWorkspaces: vi.fn(async () => undefined),
    listWorkspaces: vi.fn(async () => mocks.workspaces.get()),
    listSessionCreationOptions:
      vi.fn<() => Promise<import("../../../src/api/types").SessionCreationOption[]>>(),
    loadWorkspaceRoots: vi.fn(async () => mocks.workspaceRoots.get()),
    browseWorkspaceRoot: vi.fn(async (): Promise<WorkspaceDirectoryListingView> => ({
      root_id: "root-1",
      path: "",
      canonical_path: "/repo",
      parent_path: null,
      entries: [],
      warnings: [],
    })),
    registerWorkspace: vi.fn(async () => workspace()),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    toastError: vi.fn(),
    navigate: vi.fn(),
    pathParams: {} as Record<string, string>,
  };
});

vi.mock("../../../src/api/client", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    listWorkspaces: mocks.listWorkspaces,
    listSessionCreationOptions: mocks.listSessionCreationOptions,
    listPendingInteractions: vi.fn(async () => []),
    listWorkspaceRoots: vi.fn(async () => mocks.workspaceRoots.get()),
    listWorkspaceRootEntries: vi.fn(async (rootId: string, path = "") =>
      mocks.browseWorkspaceRoot(rootId, path),
    ),
    refreshWorkspaceGitStatus: vi.fn(async (workspaceId: string) => {
      await mocks.refreshWorkspaceGitStatus(workspaceId);
      return (
        mocks.workspaceGitStatuses.get()[workspaceId] ?? {
          workspace_id: workspaceId,
          state: "unknown",
          observed_at: null,
        }
      );
    }),
    registerWorkspace: (input: unknown) => mocks.registerWorkspace(input),
    setSessionReasoningEffort: mocks.setSessionReasoningEffort,
    submitBranchReplay: mocks.submitBranchReplay,
  };
  return {
    ...original,
    ...mocked,
    createApi: (device) => ({ ...original.createApi(device), ...mocked }),
  };
});

vi.mock("../../../src/stores/sessions", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    sessionDetail: mocks.sessionDetail,
    sessionDetailLoading: mocks.sessionDetailLoading,
    sessionDetailError: mocks.sessionDetailError,
    sessionDetailErrorKind: mocks.sessionDetailErrorKind,
    selectSession: vi.fn(),
    loadSessionDetail: mocks.loadSessionDetail,
    sendMessage: async (
      sessionId: string,
      input: import("../../../src/api/types").SendMessageInput,
    ) => {
      const delivery = await mocks.sendMessage(sessionId, input);
      if (delivery?.status === "delivered") {
        queryClient.setQueryData<
          Record<string, import("../../../src/lib/session-chat/sessionChat").SessionChatMessage[]>
        >(sessionKeys.optimisticChat(), (current) => ({
          ...current,
          [sessionId]: [
            ...(current?.[sessionId] ?? []),
            {
              id: `optimistic:${crypto.randomUUID()}:user`,
              turnId: `local_${crypto.randomUUID()}`,
              role: "user",
              content: input.input,
              status: "pending",
              createdAt: new Date().toISOString(),
            },
          ],
        }));
      }
      return delivery;
    },
    cancelQueuedMessage: mocks.cancelQueuedMessage,
    resumeSession: mocks.resumeSession,
    interruptSession: mocks.interruptSession,
    terminateSession: mocks.terminateSession,
    pinSession: mocks.pinSession,
    unpinSession: mocks.unpinSession,
    archiveSession: mocks.archiveSession,
    unarchiveSession: mocks.unarchiveSession,
    updateSessionTitle: mocks.updateSessionTitle,
    createSession: async (input: import("../../../src/api/types").CreateSessionInput) => {
      const result = await mocks.createSession(input);
      if (input.initial_task?.input) {
        const identity = result.initial_turn?.turn_id ?? `test_${crypto.randomUUID()}`;
        queryClient.setQueryData<
          Record<string, import("../../../src/lib/session-chat/sessionChat").SessionChatMessage[]>
        >(sessionKeys.optimisticChat(), (messages) => ({
          ...messages,
          [result.session.session_id]: [
            ...(messages?.[result.session.session_id] ?? []),
            {
              id: `optimistic:${identity}:user`,
              turnId: identity,
              role: "user",
              content: input.initial_task.input.trim(),
              status: "pending",
              createdAt: result.initial_turn?.created_at ?? new Date().toISOString(),
            },
          ],
        }));
      }
      return result;
    },
  };
  return {
    ...original,
    ...mocked,
    createSessions: (device) => ({ ...original.createSessions(device), ...mocked }),
  };
});

vi.mock("../../../src/stores/timeline", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    timelineState: mocks.timelineState,
    loadSessionTimeline: mocks.loadSessionTimeline,
    refreshSessionTimeline: mocks.refreshSessionTimeline,
    restoreSessionTimeline: mocks.restoreSessionTimeline,
    hasTimelineSnapshot: (state: TimelineState, sessionId: string) =>
      state.sessionId === sessionId && (state.status === "ready" || state.status === "empty"),
    resetTimelineState: mocks.resetTimelineState,
  };
  return {
    ...original,
    ...mocked,
    createTimeline: (device) => ({ ...original.createTimeline(device), ...mocked }),
  };
});

vi.mock("../../../src/services/eventStream", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    subscribeDashboardEvents: (
      listener: (event: import("../../../src/api/types").DashboardStreamEvent) => void,
    ) => {
      mocks.dashboardEventListeners.add(listener);
      return () => mocks.dashboardEventListeners.delete(listener);
    },
  };
  return {
    ...original,
    ...mocked,
    createEvents: (device) => ({ ...original.createEvents(device), ...mocked }),
  };
});

vi.mock("../../../src/services/liveOutputStream", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    openLiveOutputStream: (
      sessionId: string,
      handlers: {
        onEvent: (event: unknown) => void;
        onDisconnected: () => void;
      },
    ) => {
      mocks.liveOutputListeners.set(sessionId, handlers);
      return () => mocks.liveOutputListeners.delete(sessionId);
    },
  };
  return {
    ...original,
    ...mocked,
    createLiveOutput: (device) => ({ ...original.createLiveOutput(device), ...mocked }),
  };
});

vi.mock("$lib/navigation", () => ({ navigate: mocks.navigate }));

vi.mock("svelte-sonner", () => ({
  toast: { error: mocks.toastError, dismiss: vi.fn() },
}));

export const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: "session-1",
  client_type: "pi",
  title: null,
  handle: "main",
  role: null,
  description: null,
  state: "idle",
  current_turn_id: null,
  workspace_id: "workspace-1",
  workspace: null,
  capabilities: { accept_task: true, timeline: true },
  model: null,
  context_usage: null,
  created_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
  metadata: {},
  ...overrides,
});

export const turn = (overrides: Partial<TurnView> = {}): TurnView => ({
  turn_id: "turn-1",
  session_id: "session-1",
  parent_turn_id: null,
  topology_status: "unknown",
  state: "completed",
  input: { summary: "hello" },
  output: { summary: "hi there" },
  failure: null,
  created_at: "2026-05-14T00:00:00Z",
  started_at: "2026-05-14T00:00:01Z",
  completed_at: "2026-05-14T00:00:02Z",
  metadata: {},
  ...overrides,
});

export const queuedMessage = (overrides: Partial<QueuedMessageView> = {}): QueuedMessageView => ({
  message_id: "message-1",
  session_id: "session-1",
  input: "queued follow-up",
  metadata: {},
  created_at: "2026-05-14T00:00:03Z",
  ...overrides,
});

export function timelineItemsFromTurns(turns: TurnView[]): TimelineItem[] {
  return turns.flatMap((item): TimelineItem[] => [
    {
      item_id: `${item.turn_id}:user`,
      kind: "user",
      raw_kind: "user",
      role: "user",
      title: null,
      status: null,
      occurred_at: item.created_at,
      content_preview: typeof item.input?.summary === "string" ? item.input.summary : null,
      turn_id: item.turn_id,
    },
    {
      item_id: `${item.turn_id}:assistant`,
      kind: "assistant",
      raw_kind: "text",
      role: "assistant",
      title: null,
      status: item.failure ? "error" : null,
      occurred_at: item.completed_at ?? item.created_at,
      content_preview:
        item.output?.summary ??
        (typeof item.failure?.message === "string" ? item.failure.message : null),
      turn_id: item.turn_id,
    },
  ]);
}

export const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  availability: { status: "available", root_id: "root-1", path: "pontia" },
  workspace_id: "workspace-1",
  canonical_path: "/repo/pontia",
  display_path: "~/repo/pontia",
  name: "pontia",
  state: "active",
  metadata: {},
  created_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
  last_used_at: null,
  ...overrides,
});

export { mocks };
export const timelineStateValue = mocks.timelineStateValue;

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  queryClient.clear();
  resetChatEntryAutofocus();
  if (!Element.prototype.hasPointerCapture) Element.prototype.hasPointerCapture = () => false;
  if (!Element.prototype.releasePointerCapture)
    Element.prototype.releasePointerCapture = () => undefined;
  window.history.pushState({}, "", "/dashboard");
  const activeSession = session();
  mocks.loadedSessions = [activeSession];
  mocks.sessions.set([activeSession]);
  mocks.sessionDetail.set(null);
  mocks.sessionDetailLoading.set(false);
  mocks.sessionDetailError.set(null);
  mocks.sessionDetailErrorKind.set(null);
  mocks.workspaces.set([workspace()]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesInitialized.set(true);
  mocks.workspacesError.set(null);
  mocks.workspaceRoots.set([
    { root_id: "root-1", label: "Projects", canonical_path: "/repo", state: "available" },
  ]);
  mocks.workspaceGitStatuses.set({});
  mocks.workspaceGitStatusErrors.set({});
  mocks.timelineState.set(mocks.timelineStateValue());
  chatDraft.set("");
  mocks.dashboardEventListeners.clear();
  mocks.liveOutputListeners.clear();
  mocks.pathParams = {};
  mocks.loadSessionDetail.mockReset().mockResolvedValue(null);
  mocks.listWorkspaces.mockReset().mockImplementation(async () => mocks.workspaces.get());
  mocks.listSessionCreationOptions.mockReset().mockResolvedValue([
    {
      id: "pi",
      name: "Pi",
      client_type: "pi",
      client_key: "default",
      model_catalog_key: "default",
      version: null,
      metadata: {},
    },
    {
      id: "codex",
      name: "codex",
      client_type: "codex",
      client_key: "default",
      version: null,
      metadata: {},
    },
  ]);
  mocks.loadSessionTimeline.mockReset();
  mocks.refreshSessionTimeline.mockReset().mockResolvedValue(true);
  mocks.restoreSessionTimeline.mockReset().mockResolvedValue(false);
  mocks.createSession.mockResolvedValue({
    session: activeSession,
    initial_turn: null,
  } satisfies CreateSessionResult);
  mocks.loadWorkspaceRoots.mockReset().mockImplementation(async () => mocks.workspaceRoots.get());
  mocks.browseWorkspaceRoot.mockReset().mockResolvedValue({
    root_id: "root-1",
    path: "",
    canonical_path: "/repo",
    parent_path: null,
    entries: [],
    warnings: [],
  });
  mocks.registerWorkspace.mockReset().mockResolvedValue(workspace());
  mocks.setSessionReasoningEffort.mockReset().mockResolvedValue({ accepted: true });
  mocks.submitBranchReplay.mockReset().mockResolvedValue({ replay_id: "replay_test" });
  mocks.sendMessage.mockReset().mockResolvedValue({ status: "delivered" });
  mocks.cancelQueuedMessage.mockReset();
  queryClient.setQueryData(sessionKeys.optimisticChat(), {});
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    const detail = mocks.sessionDetail.get();
    const turns = detail?.turns ?? [];
    const page = {
      session_id: sessionId,
      items: timelineItemsFromTurns(turns),
      direction: "backward" as const,
      next_turn_id: null,
    };
    mocks.timelineState.set(
      mocks.timelineStateValue({
        sessionId,
        items: page.items,
        nextOlderTurnId: page.next_turn_id,
        latestTurnId: page.items.at(-1)?.turn_id ?? null,
        hasMore: page.next_turn_id !== null,
        loading: false,
        refreshing: false,
        refreshKind: null,
        status: page.items.length ? "ready" : "empty",
      }),
    );
    return page;
  });
  window.localStorage.clear();
  document.body.style.pointerEvents = "";
  vi.clearAllMocks();
});
