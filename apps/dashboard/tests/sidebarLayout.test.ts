import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import AppSidebarHost from "./components/layout/AppSidebarHost.svelte";
import AppShellHost from "./components/layout/AppShellHost.svelte";
import TopBar from "../src/components/layout/TopBar.svelte";
import SettingsShellHost from "./components/settings/SettingsShellHost.svelte";
import { downloads } from "../src/lib/downloads/manager";
import { testDevice } from "./deviceFixture";

import type { BotMessagePage, BotWorkStatus } from "../src/api/types";
import type { BotUpdate } from "../src/services/botChatStream";

const botStreams = vi.hoisted(() => new Map<string, (update: BotUpdate) => void>());
vi.mock("../src/services/botChatStream", () => ({
  openBotChatStream: (_device: unknown, id: string, onUpdate: (update: BotUpdate) => void) => {
    botStreams.set(id, onUpdate);
    return () => botStreams.delete(id);
  },
}));

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
      get value() {
        return value;
      },
    };
  }

  const sessionItems = writableStore<unknown[]>([]);
  const loading = writableStore(false);
  const error = writableStore<string | null>(null);
  return {
    navigate: vi.fn(),
    startEventStream: vi.fn(),
    stopEventStream: vi.fn(),
    sessions: sessionItems,
    sidebarPinnedSessions: writableStore<unknown[]>([]),
    sidebarActiveSessions: sessionItems,
    sidebarRecentSessions: writableStore<unknown[]>([]),
    sidebarSessionsLoading: loading,
    sidebarSessionsLoadingMore: writableStore(false),
    sidebarSessionsError: error,
    sidebarSessionsNextCursor: writableStore<string | null>(null),
    sessionDetail: writableStore(null),
    sessionDetailError: writableStore<string | null>(null),
    loadMoreSidebarSessions: vi.fn(async () => []),
    updateSessionTitle: vi.fn(async () => undefined),
    pinSession: vi.fn(async () => undefined),
    unpinSession: vi.fn(async () => undefined),
    archiveSession: vi.fn(async () => undefined),
    terminateSession: vi.fn(async () => undefined),
  };
});

vi.mock("$lib/navigation", async (importOriginal) => ({
  ...(await importOriginal()),
  navigate: mocks.navigate,
}));
vi.mock("../src/services/eventStream", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    startEventStream: mocks.startEventStream,
    stopEventStream: mocks.stopEventStream,
  };
  return {
    ...original,
    ...mocked,
    createEvents: (device) => ({ ...original.createEvents(device), ...mocked }),
  };
});
vi.mock("../src/stores/sessions", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    sessionDetail: mocks.sessionDetail,
    sessionDetailError: mocks.sessionDetailError,
    updateSessionTitle: mocks.updateSessionTitle,
    pinSession: mocks.pinSession,
    unpinSession: mocks.unpinSession,
    archiveSession: mocks.archiveSession,
    terminateSession: mocks.terminateSession,
  };
  return {
    ...original,
    ...mocked,
    createSessions: (device) => ({ ...original.createSessions(device), ...mocked }),
  };
});
vi.mock("../src/queries/workspaces", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    createWorkspacesQuery: () => ({
      data: [
        {
          workspace_id: "workspace-1",
          name: "First workspace",
          display_path: "/first",
          canonical_path: "/first",
        },
        {
          workspace_id: "workspace-2",
          name: "Second workspace",
          display_path: "/second",
          canonical_path: "/second",
        },
      ],
      isPending: false,
      error: null,
    }),
  };
  return {
    ...original,
    ...mocked,
    createWorkspaceQueries: (device) => ({ ...original.createWorkspaceQueries(device), ...mocked }),
  };
});
vi.mock("../src/queries/sessionOverview", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    createSessionOverviewQuery: (_includeArchived = false, workspaceId = () => undefined) => ({
      get data() {
        return {
          pinned: mocks.sidebarPinnedSessions.value,
          active: mocks.sidebarActiveSessions.value,
          list: mocks.sidebarRecentSessions.value.filter(
            (session) =>
              !workspaceId() ||
              (session as { workspace_id: string }).workspace_id === workspaceId(),
          ),
          archived: [],
          nextCursor: mocks.sidebarSessionsNextCursor.value,
        };
      },
      get isPending() {
        return mocks.sidebarSessionsLoading.value;
      },
      get isFetchingNextPage() {
        return mocks.sidebarSessionsLoadingMore.value;
      },
      get error() {
        const message = mocks.sidebarSessionsError.value;
        return message ? new Error(message) : null;
      },
      fetchNextPage: mocks.loadMoreSidebarSessions,
    }),
    snapshotSessionOverview: (data: unknown) => data,
  };
  return {
    ...original,
    ...mocked,
    createSessionOverview: (device) => ({ ...original.createSessionOverview(device), ...mocked }),
  };
});
beforeEach(() => {
  window.history.pushState({}, "", "/dashboard");
  mocks.sessions.set([]);
  mocks.sidebarPinnedSessions.set([]);
  mocks.sidebarRecentSessions.set([]);
  mocks.sidebarSessionsNextCursor.set(null);
  mocks.sidebarSessionsLoadingMore.set(false);
  mocks.sidebarSessionsLoading.set(false);
  mocks.sidebarSessionsError.set(null);
  testDevice.sidebarSessionFilter.selectedWorkspaceId.set("");
  testDevice.sidebarSessionFilter.sessionScope.set("active");
  vi.clearAllMocks();
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

function chatSession(
  session_id: string,
  state: string,
  updated_at: string,
  pinned_at: string | null = null,
) {
  return {
    session_id,
    client_type: "pi",
    handle: session_id,
    role: null,
    description: null,
    state,
    current_turn_id: null,
    workspace_id: "workspace-1",
    workspace: null,
    capabilities: {},
    created_at: "2026-05-14T00:00:00Z",
    updated_at,
    pinned_at,
    metadata: {},
  };
}

test("opens the all sessions page from the Sessions header", async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole("button", { name: "Open all sessions" }));

  expect(mocks.navigate).toHaveBeenCalledWith("/sessions");
});

test("workspace selection filters recent sessions but keeps global pins and does not navigate", async () => {
  const first = chatSession("first-active", "idle", "2026-05-14T04:00:00Z");
  const second = {
    ...chatSession("second-active", "idle", "2026-05-14T03:00:00Z"),
    workspace_id: "workspace-2",
  };
  const older = chatSession("first-older", "exited", "2026-05-14T01:00:00Z");
  const pinned = {
    ...chatSession("second-pinned", "exited", "2026-05-14T01:00:00Z", "2026-05-14T02:00:00Z"),
    workspace_id: "workspace-2",
  };
  mocks.sessions.set([first, second]);
  mocks.sidebarRecentSessions.set([first, second, older, pinned]);
  mocks.sidebarPinnedSessions.set([pinned]);
  testDevice.sidebarSessionFilter.sessionScope.set("all");
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole("button", { name: "Filter sessions" }));
  await fireEvent.click(screen.getByRole("menuitemradio", { name: /First workspace/ }));

  expect(screen.getAllByText("first-active")).toHaveLength(1);
  expect(screen.getByText("first-older")).toBeInTheDocument();
  expect(screen.queryByText("second-active")).not.toBeInTheDocument();
  expect(screen.getAllByText("second-pinned")).toHaveLength(1);
  expect(mocks.navigate).not.toHaveBeenCalled();

  await fireEvent.click(screen.getByRole("button", { name: "Filter sessions" }));
  expect(screen.getByRole("menuitemradio", { name: /First workspace/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await fireEvent.click(screen.getByRole("menuitemradio", { name: "All workspaces" }));

  expect(screen.getByText("second-active")).toBeInTheDocument();
  expect(screen.getAllByText("second-pinned")).toHaveLength(1);
});

test("sidebar shows only active sessions by default and switches to all sessions from the filter or the list", async () => {
  TestIntersectionObserver.instances = [];
  vi.stubGlobal("IntersectionObserver", TestIntersectionObserver);
  const active = chatSession("session-active", "idle", "2026-05-14T04:00:00Z");
  const otherWorkspace = {
    ...chatSession("session-other", "idle", "2026-05-14T03:00:00Z"),
    workspace_id: "workspace-2",
  };
  const exited = chatSession("session-exited", "exited", "2026-05-14T02:00:00Z");
  const failed = chatSession("session-failed", "error", "2026-05-14T01:00:00Z");
  const pinnedExited = chatSession(
    "session-pinned",
    "exited",
    "2026-05-14T01:00:00Z",
    "2026-05-14T05:00:00Z",
  );
  mocks.sessions.set([active, otherWorkspace]);
  mocks.sidebarRecentSessions.set([active, otherWorkspace, exited, failed, pinnedExited]);
  mocks.sidebarPinnedSessions.set([pinnedExited]);
  mocks.sidebarSessionsNextCursor.set("next-page");
  render(AppSidebarHost);

  expect(screen.getByText("session-active")).toBeInTheDocument();
  expect(screen.getByText("session-other")).toBeInTheDocument();
  expect(screen.getByText("session-pinned")).toBeInTheDocument();
  expect(screen.queryByText("session-exited")).not.toBeInTheDocument();
  expect(screen.queryByText("session-failed")).not.toBeInTheDocument();
  TestIntersectionObserver.instances[0].trigger(true);
  expect(mocks.loadMoreSidebarSessions).not.toHaveBeenCalled();

  await fireEvent.click(screen.getByRole("button", { name: "Filter sessions" }));
  expect(screen.getByRole("menuitemradio", { name: "Active sessions" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await fireEvent.click(screen.getByRole("menuitemradio", { name: /First workspace/ }));
  expect(screen.queryByText("session-other")).not.toBeInTheDocument();
  expect(screen.queryByText("session-exited")).not.toBeInTheDocument();

  document.body.style.pointerEvents = "";
  await fireEvent.click(screen.getByRole("button", { name: "Show closed sessions" }));
  expect(screen.getByText("session-exited")).toBeInTheDocument();
  expect(screen.getByText("session-failed")).toBeInTheDocument();
  expect(screen.queryByText("session-other")).not.toBeInTheDocument();
  expect(screen.getAllByText("session-pinned")).toHaveLength(1);
  expect(screen.queryByRole("button", { name: "Show closed sessions" })).not.toBeInTheDocument();
  TestIntersectionObserver.instances[0].trigger(true);
  await waitFor(() => expect(mocks.loadMoreSidebarSessions).toHaveBeenCalledTimes(1));

  await fireEvent.click(screen.getByRole("button", { name: "Filter sessions" }));
  expect(screen.getByRole("menuitemradio", { name: "All sessions" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await fireEvent.click(screen.getByRole("menuitemradio", { name: "Active sessions" }));
  expect(screen.queryByText("session-exited")).not.toBeInTheDocument();
  expect(screen.getByText("session-active")).toBeInTheDocument();
  vi.unstubAllGlobals();
});

test("sidebar hides the closed-session link when there are no closed sessions", () => {
  mocks.sessions.set([chatSession("session-active", "idle", "2026-05-14T04:00:00Z")]);
  mocks.sidebarRecentSessions.set([chatSession("session-active", "idle", "2026-05-14T04:00:00Z")]);
  render(AppSidebarHost);

  expect(screen.getByText("session-active")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Show closed sessions" })).not.toBeInTheDocument();
});

test("sidebar says when no session is active", () => {
  mocks.sidebarRecentSessions.set([
    chatSession("session-exited", "exited", "2026-05-14T02:00:00Z"),
  ]);
  render(AppSidebarHost);

  expect(screen.getByText("No active sessions")).toBeInTheDocument();
  expect(screen.queryByText("session-exited")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Show closed sessions" })).toBeInTheDocument();
});

test("sidebar renders active sessions before paged recent sessions without duplicating active sessions", () => {
  const active = chatSession("session-active", "idle", "2026-05-14T04:00:00Z");
  const duplicate = chatSession("session-active", "idle", "2026-05-14T04:00:00Z");
  const older = chatSession("session-older", "exited", "2026-05-14T01:00:00Z");
  mocks.sessions.set([active]);
  mocks.sidebarRecentSessions.set([duplicate, older]);
  testDevice.sidebarSessionFilter.sessionScope.set("all");

  render(AppSidebarHost);

  const activeButton = screen.getByText("session-active").closest("button");
  const olderButton = screen.getByText("session-older").closest("button");
  expect(screen.getAllByText("session-active")).toHaveLength(1);
  expect(
    activeButton?.compareDocumentPosition(olderButton as Node) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
});

test("sidebar lists pinned sessions first under one collapsible Sessions group", async () => {
  const pinned = chatSession(
    "session-pinned",
    "idle",
    "2026-05-14T01:00:00Z",
    "2026-05-14T02:00:00Z",
  );
  const active = chatSession("session-active", "idle", "2026-05-14T04:00:00Z");
  mocks.sidebarPinnedSessions.set([pinned]);
  mocks.sessions.set([active]);
  render(AppSidebarHost);

  expect(screen.queryByText(/pinned sessions/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/recent sessions/i)).not.toBeInTheDocument();
  const pinnedButton = screen.getByText("session-pinned").closest("button")!;
  const activeButton = screen.getByText("session-active").closest("button")!;
  expect(within(pinnedButton).getByLabelText("Pinned session")).toBeInTheDocument();
  expect(within(activeButton).queryByLabelText("Pinned session")).not.toBeInTheDocument();
  expect(
    pinnedButton.compareDocumentPosition(activeButton) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();

  const toggle = screen.getByRole("button", { name: /^sessions$/i });
  expect(toggle).toHaveAttribute("aria-expanded", "true");
  await fireEvent.click(toggle);
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText("session-pinned")).not.toBeInTheDocument();
  expect(screen.queryByText("session-active")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "New Chat" })).toBeInTheDocument();
});

test("sidebar keeps the no-active-sessions hint below pinned sessions", () => {
  mocks.sidebarPinnedSessions.set([
    chatSession("session-pinned", "idle", "2026-05-14T01:00:00Z", "2026-05-14T02:00:00Z"),
  ]);
  render(AppSidebarHost);

  expect(screen.getByText("session-pinned")).toBeInTheDocument();
  expect(screen.getByText("No active sessions")).toBeInTheDocument();
});

class TestIntersectionObserver {
  static instances: TestIntersectionObserver[] = [];
  readonly target: Element[] = [];

  constructor(
    private readonly callback: IntersectionObserverCallback,
    readonly options: IntersectionObserverInit = {},
  ) {
    TestIntersectionObserver.instances.push(this);
  }

  observe(element: Element) {
    this.target.push(element);
  }

  unobserve() {}
  disconnect() {}

  trigger(isIntersecting: boolean) {
    this.callback(
      this.target.map((target) => ({ isIntersecting, target }) as IntersectionObserverEntry),
      this as unknown as IntersectionObserver,
    );
  }
}

test("sidebar loads the next page when the end of its session list scrolls into view", async () => {
  TestIntersectionObserver.instances = [];
  vi.stubGlobal("IntersectionObserver", TestIntersectionObserver);
  mocks.sessions.set([chatSession("session-active", "idle", "2026-05-14T04:00:00Z")]);
  mocks.sidebarSessionsNextCursor.set("next-page");
  testDevice.sidebarSessionFilter.sessionScope.set("all");
  render(AppSidebarHost);

  const [observer] = TestIntersectionObserver.instances;
  expect(observer.options.root).toBe(document.querySelector('[data-sidebar="content"]'));
  expect(mocks.loadMoreSidebarSessions).not.toHaveBeenCalled();

  observer.trigger(true);
  await waitFor(() => expect(mocks.loadMoreSidebarSessions).toHaveBeenCalledTimes(1));
  vi.unstubAllGlobals();
});

test("sidebar shows each Bot's work status and open todos", async () => {
  const bots = ["Release Captain", "Reviewer", "Ops", "Writer"].map((name, index) => ({
    bot_id: `bot-${index}`,
    name,
    description: null,
    avatar_shape: "classic" as const,
    avatar_color: "blue" as const,
    client_type: "pi",
    client_key: "default",
    model: "provider/model",
    reasoning_effort: null,
  }));
  const pages: Record<string, Pick<BotMessagePage, "status" | "open_todo_count">> = {
    "bot-0": { status: { kind: "working", title: "Fix upload retry" }, open_todo_count: 2 },
    "bot-1": { status: { kind: "processing" }, open_todo_count: 0 },
    "bot-2": { status: { kind: "idle" }, open_todo_count: 0 },
    "bot-3": { status: { kind: "busy" }, open_todo_count: 0 },
  };
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue(bots);
  vi.spyOn(testDevice.api, "getBotMessages").mockImplementation(
    async (id) =>
      ({
        bot: bots.find((bot) => bot.bot_id === id)!,
        ...pages[id],
        entries: [],
        next_before: null,
        generation_notice: null,
        notice_session_id: null,
        file_link_session: null,
      }) satisfies BotMessagePage,
  );
  render(AppSidebarHost);

  const row = (name: string) => screen.getByText(name).closest("a")!;
  expect(
    await within(await waitFor(() => row("Release Captain"))).findByText("Fix upload retry"),
  ).toBeInTheDocument();
  expect(
    within(row("Release Captain")).getByRole("img", { name: "2 open todos" }),
  ).toBeInTheDocument();
  expect(await within(row("Reviewer")).findByText("Working on your message")).toBeInTheDocument();
  expect(row("Ops")).toHaveAccessibleName("Ops");
  expect(await within(row("Writer")).findByText("Still working")).toBeInTheDocument();
  expect(within(row("Reviewer")).queryByLabelText(/open todo/)).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^bots$/i })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Create Bot" })).toBeInTheDocument();
  vi.restoreAllMocks();
});

test("sidebar offers a setup card when the device has no Bot", async () => {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([]);
  render(AppSidebarHost);

  expect(await screen.findByText("Set up your first Bot")).toBeInTheDocument();
  const buttons = screen.getAllByRole("button", { name: "Create Bot" });
  expect(buttons).toHaveLength(2);
  await fireEvent.click(buttons[1]);
  expect(await screen.findByRole("dialog", { name: "Create Bot" })).toBeInTheDocument();
  vi.restoreAllMocks();
});

test("sidebar shows semantic status dots except for terminal sessions, and opens chat for the selected session", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
    {
      session_id: "session-closed",
      client_type: "pi",
      handle: "closed",
      role: null,
      description: null,
      state: "exited",
      current_turn_id: null,
      workspace_id: "workspace-2",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T02:00:00Z",
      metadata: {},
    },
    {
      session_id: "session-error",
      client_type: "pi",
      handle: "failed",
      role: null,
      description: null,
      state: "error",
      current_turn_id: null,
      workspace_id: "workspace-3",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T03:00:00Z",
      metadata: {},
    },
  ]);

  render(AppSidebarHost);

  const activeSessionButton = screen.getByText("main · coder").closest("button");
  const closedSessionButton = screen.getByText("closed").closest("button");
  expect(activeSessionButton).not.toBeNull();
  expect(closedSessionButton).not.toBeNull();
  expect(screen.getByLabelText("idle session")).toBeInTheDocument();
  expect(screen.queryByLabelText("exited session")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("error session")).not.toBeInTheDocument();
  expect(screen.queryByText("idle")).not.toBeInTheDocument();
  expect(screen.queryByText("exited")).not.toBeInTheDocument();

  await fireEvent.click(screen.getByText("main · coder"));

  expect(mocks.navigate).toHaveBeenCalledWith("/chat/session-active");
});

test("sidebar renames a recent session from the hover edit action without opening it", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      title: "Original title",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(
    screen.getByRole("button", { name: /open session actions for original title/i }),
  );
  await fireEvent.click(screen.getByRole("menuitem", { name: /rename/i }));

  const dialog = screen.getByRole("dialog", { name: "Rename session" });
  const titleInput = within(dialog).getByLabelText("Session title");
  await fireEvent.input(titleInput, { target: { value: "Renamed session" } });
  await fireEvent.click(within(dialog).getByRole("button", { name: "Rename session" }));

  expect(mocks.updateSessionTitle).toHaveBeenCalledWith("session-active", "Renamed session");
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("sidebar session actions menu pins unpinned sessions without opening them", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      title: "Original title",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(
    screen.getByRole("button", { name: /open session actions for original title/i }),
  );
  await fireEvent.click(screen.getByRole("menuitem", { name: /pin/i }));
  expect(mocks.pinSession).toHaveBeenCalledWith("session-active");
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("sidebar session actions menu exits sessions without opening them", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      title: "Original title",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(
    screen.getByRole("button", { name: /open session actions for original title/i }),
  );
  const menu = screen.getByRole("menu");
  await fireEvent.click(within(menu).getByRole("menuitem", { name: /^exit$/i }));
  expect(mocks.terminateSession).toHaveBeenCalledWith("session-active");
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("sidebar session actions menu archives sessions without opening them", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      title: "Original title",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(
    screen.getByRole("button", { name: /open session actions for original title/i }),
  );
  await fireEvent.click(screen.getByRole("menuitem", { name: /archive/i }));
  expect(mocks.archiveSession).toHaveBeenCalledWith("session-active");
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("sidebar session actions menu unpins pinned sessions", async () => {
  mocks.sidebarPinnedSessions.set([
    {
      session_id: "session-pinned",
      client_type: "pi",
      title: "Pinned title",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      pinned_at: "2026-05-14T01:00:00Z",
      archived_at: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(
    screen.getByRole("button", { name: /open session actions for pinned title/i }),
  );
  await fireEvent.click(screen.getByRole("menuitem", { name: /unpin/i }));
  expect(mocks.unpinSession).toHaveBeenCalledWith("session-pinned");
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("right-clicking a sidebar session opens its actions menu without opening the session", async () => {
  mocks.sessions.set([{ ...chatSession("session-active", "idle", "2026-05-14T01:00:00Z") }]);
  render(AppSidebarHost);

  const row = screen.getByText("session-active").closest("button")!;
  expect(await fireEvent.contextMenu(row, { clientX: 40, clientY: 60 })).toBe(false);
  const menu = screen.getByRole("menu");
  expect(
    within(menu)
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.trim()),
  ).toEqual(["Rename", "Pin", "Archive", "Exit"]);
  await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pin" }));

  expect(mocks.pinSession).toHaveBeenCalledWith("session-active");
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("shift right-click on a sidebar session keeps the browser menu", async () => {
  mocks.sessions.set([{ ...chatSession("session-active", "idle", "2026-05-14T01:00:00Z") }]);
  render(AppSidebarHost);

  const row = screen.getByText("session-active").closest("button")!;
  expect(await fireEvent.contextMenu(row, { shiftKey: true })).toBe(true);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

function mockSidebarBot() {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([
    {
      bot_id: "bot-1",
      name: "Release Captain",
      description: null,
      avatar_shape: "classic",
      avatar_color: "blue",
      client_type: "pi",
      client_key: "default",
      model: "provider/model",
      reasoning_effort: null,
    },
  ]);
}

async function openBotContextMenu() {
  render(AppSidebarHost);
  const link = (await screen.findByText("Release Captain")).closest("a")!;
  expect(await fireEvent.contextMenu(link, { clientX: 40, clientY: 60 })).toBe(false);
  return screen.getByRole("menu", { name: "Bot actions" });
}

test("right-clicking a sidebar Bot offers edit, reload, and delete", async () => {
  mockSidebarBot();
  const menu = await openBotContextMenu();

  expect(
    within(menu)
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.trim()),
  ).toEqual(["Edit Bot", "Reload Bot session", "Delete Bot"]);
  await fireEvent.click(within(menu).getByRole("menuitem", { name: "Edit Bot" }));

  expect(await screen.findByRole("dialog", { name: "Edit Bot" })).toBeInTheDocument();
  expect(mocks.navigate).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});

test("the sidebar Bot menu reloads the Bot session", async () => {
  mockSidebarBot();
  const reload = vi.spyOn(testDevice.api, "reloadBot").mockResolvedValue();
  const menu = await openBotContextMenu();

  await fireEvent.click(within(menu).getByRole("menuitem", { name: "Reload Bot session" }));

  expect(reload).toHaveBeenCalledWith("bot-1");
  expect(mocks.navigate).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});

test("the sidebar Bot menu deletes only after the delete confirmation", async () => {
  mockSidebarBot();
  vi.spyOn(testDevice.api, "getBotDeletionBlockers").mockResolvedValue({
    running: false,
    helpers: [],
  });
  const remove = vi.spyOn(testDevice.api, "deleteBot").mockResolvedValue();
  const menu = await openBotContextMenu();

  await fireEvent.click(within(menu).getByRole("menuitem", { name: "Delete Bot" }));
  const dialog = await screen.findByRole("alertdialog", { name: "Delete Release Captain?" });
  expect(remove).not.toHaveBeenCalled();
  const confirm = within(dialog).getByRole("button", { name: "Delete Bot" });
  await waitFor(() => expect(confirm).toBeEnabled());
  await fireEvent.click(confirm);

  await waitFor(() => expect(remove).toHaveBeenCalledWith("bot-1"));
  vi.restoreAllMocks();
});

test("the Sessions header opens New Chat and shows its shortcut on hover", async () => {
  render(AppSidebarHost);

  expect(screen.queryByText("New Chat")).not.toBeInTheDocument();
  const button = screen.getByRole("button", { name: "New Chat" });
  await fireEvent.pointerMove(button);
  await fireEvent.focus(button);
  const tooltip = await screen.findByText("New Chat", { selector: "[data-slot=tooltip-content]" });
  expect(within(tooltip).getByText("Alt+N")).toBeInTheDocument();
  await fireEvent.click(button);

  expect(mocks.navigate).toHaveBeenCalledWith("/");
});

test("the collapsed icon rail keeps New Chat and marks it active on the default route", async () => {
  window.history.pushState({}, "", "/dashboard");
  render(AppSidebarHost);
  const header = document.querySelector<HTMLElement>('[data-slot="sidebar-header"]')!;
  expect(document.querySelector('[data-sidebar="menu-button"][data-active]')).toBeNull();

  await fireEvent.click(within(header).getByRole("button", { name: /toggle sidebar/i }));
  const chat = screen
    .getAllByRole("button", { name: "New Chat" })
    .find((button) => button.dataset.sidebar === "menu-button")!;
  expect(chat).toHaveAttribute("data-active", "true");
  await fireEvent.click(chat);
  expect(mocks.navigate).toHaveBeenCalledWith("/");
});

test("sidebar highlights the matching recent session on chat routes", () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
    {
      session_id: "session-other",
      client_type: "pi",
      handle: "other",
      role: null,
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-2",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T00:30:00Z",
      metadata: {},
    },
  ]);

  window.history.pushState({}, "", "/dashboard/chat/session-active");
  render(AppSidebarHost);

  expect(screen.getByText("main · coder").closest("button")).toHaveAttribute("data-active", "true");
  expect(screen.getByText("other").closest("button")).not.toHaveAttribute("data-active");
});

test("top bar session title opens the rename dialog", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      title: "Original title",
      handle: "main",
      role: "coder",
      description: null,
      state: "idle",
      current_turn_id: null,
      workspace_id: "workspace-1",
      workspace: null,
      capabilities: {},
      created_at: "2026-05-14T00:00:00Z",
      updated_at: "2026-05-14T01:00:00Z",
      metadata: {},
    },
  ]);
  window.history.pushState({}, "", "/dashboard/chat/session-active");
  render(TopBar);

  await fireEvent.click(
    within(screen.getByRole("banner")).getByRole("button", { name: "Original title" }),
  );

  const dialog = screen.getByRole("dialog", { name: "Rename session" });
  const titleInput = within(dialog).getByLabelText("Session title");
  expect(titleInput).toHaveValue("Original title");
  await fireEvent.input(titleInput, { target: { value: "Renamed session" } });
  await fireEvent.click(within(dialog).getByRole("button", { name: "Rename session" }));

  expect(mocks.updateSessionTitle).toHaveBeenCalledWith("session-active", "Renamed session");
});

test("sidebar header trigger collapses and expands the sidebar", async () => {
  render(AppSidebarHost);

  const header = document.querySelector<HTMLElement>('[data-slot="sidebar-header"]')!;
  const trigger = within(header).getByRole("button", { name: /toggle sidebar/i });
  const sidebar = header.closest('[data-slot="sidebar"]');
  expect(sidebar).toHaveAttribute("data-state", "expanded");
  await fireEvent.click(trigger);
  expect(sidebar).toHaveAttribute("data-state", "collapsed");
  await fireEvent.click(trigger);
  expect(sidebar).toHaveAttribute("data-state", "expanded");
});

test("sidebar footer opens the keyboard shortcuts reference", async () => {
  const listener = vi.fn();
  document.addEventListener("pontia:open-chat-shortcuts", listener);
  try {
    render(AppSidebarHost);
    await fireEvent.click(screen.getByRole("button", { name: "Keyboard shortcuts" }));
    expect(listener).toHaveBeenCalledOnce();
  } finally {
    document.removeEventListener("pontia:open-chat-shortcuts", listener);
  }
});

test("sidebar settings button navigates directly to common settings without document reload", async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole("button", { name: /settings/i }));
  expect(mocks.navigate).toHaveBeenCalledWith("/settings/common");
});

test("chat shortcuts move down and up the active sessions in sidebar order", async () => {
  window.history.pushState({}, "", "/dashboard/chat/session-middle");
  // The overview API returns active sessions already ordered by most recent activity.
  mocks.sessions.set([
    chatSession("session-top", "busy", "2026-05-14T04:00:00Z"),
    chatSession("session-middle", "idle", "2026-05-14T03:00:00Z"),
    chatSession("session-bottom", "idle", "2026-05-14T02:00:00Z"),
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "j", code: "KeyJ", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-bottom");

  await fireEvent.keyDown(window, { key: "k", code: "KeyK", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-top");
});

test.each(["/dashboard", "/dashboard/bots/test-bot", "/dashboard/settings/common"])(
  "chat help shortcut opens a kbd shortcut reference dialog on %s",
  async (path) => {
    window.history.pushState({}, "", path);
    render(AppShellHost);

    await fireEvent.keyDown(window, { key: "?", code: "Slash", altKey: true, shiftKey: true });

    const dialog = screen.getByRole("dialog", { name: /keyboard shortcuts/i });
    expect(within(dialog).getByText(/next active chat/i)).toBeInTheDocument();
    expect(within(dialog).getByText(/focus chat input/i)).toBeInTheDocument();
  },
);

test("chat header help button opens the shortcuts dialog", async () => {
  render(AppShellHost);

  const helpButton = screen.getByRole("button", { name: /keyboard shortcuts/i });

  await fireEvent.click(helpButton);

  expect(screen.getByRole("dialog", { name: /keyboard shortcuts/i })).toBeInTheDocument();
});

test("chat numeric shortcuts open active sessions by sidebar order within the selected workspace", async () => {
  mocks.sessions.set([
    chatSession("session-first", "idle", "2026-05-14T04:00:00Z"),
    {
      ...chatSession("session-other", "idle", "2026-05-14T03:00:00Z"),
      workspace_id: "workspace-2",
    },
    chatSession("session-second", "idle", "2026-05-14T02:00:00Z"),
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "2", code: "Digit2", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-other");

  await fireEvent.click(screen.getByRole("button", { name: "Filter sessions" }));
  await fireEvent.click(screen.getByRole("menuitemradio", { name: /First workspace/ }));

  await fireEvent.keyDown(window, { key: "2", code: "Digit2", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-second");
});

test("chat shortcuts are scoped to chat routes and work while typing", async () => {
  mocks.sessions.set([chatSession("session-recent", "idle", "2026-05-14T04:00:00Z")]);

  render(AppShellHost);

  window.history.pushState({}, "", "/dashboard/settings/common");
  await fireEvent.keyDown(window, { key: "1", code: "Digit1", altKey: true });
  expect(mocks.navigate).not.toHaveBeenCalled();

  window.history.pushState({}, "", "/dashboard");
  const input = document.createElement("textarea");
  document.body.appendChild(input);
  input.focus();
  await fireEvent.keyDown(input, { key: "1", code: "Digit1", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-recent");
  input.remove();
});

test.each(["/dashboard", "/dashboard/bots/test-bot"])(
  "Alt+L moves focus from another control to the chat input on %s",
  async (path) => {
    window.history.pushState({}, "", path);
    const { container } = render(AppShellHost);
    const input = document.createElement("textarea");
    input.setAttribute("data-chat-shortcut-focus-target", "true");
    container.appendChild(input);
    const button = screen.getByRole("button", { name: /keyboard shortcuts/i });
    button.focus();

    await fireEvent.keyDown(button, { key: "l", code: "KeyL", altKey: true });
    expect(input).toHaveFocus();
  },
);

test("bot chat leaves session navigation shortcuts unhandled", async () => {
  window.history.pushState({}, "", "/dashboard/bots/test-bot");
  mocks.sessions.set([chatSession("session-recent", "idle", "2026-05-14T04:00:00Z")]);
  render(AppShellHost);

  for (const code of ["KeyJ", "KeyK", "Digit1", "Digit9", "KeyN"]) {
    const event = new KeyboardEvent("keydown", { code, altKey: true, cancelable: true });
    await fireEvent(window, event);
    expect(event.defaultPrevented).toBe(false);
  }
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test("chat new shortcut opens a new chat", async () => {
  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "n", code: "KeyN", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/");
});

test("chat new shortcut on a session route preserves the current session workspace", async () => {
  window.history.pushState({}, "", "/dashboard/chat/session-current");
  mocks.sessions.set([
    {
      ...chatSession("session-current", "idle", "2026-05-14T04:00:00Z"),
      workspace_id: "workspace-current",
    },
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "n", code: "KeyN", altKey: true });

  expect(mocks.navigate).toHaveBeenLastCalledWith("/", { workspace: "workspace-current" });
});

test("the sidebar lists settings sections instead of sessions on settings pages", async () => {
  mocks.sessions.set([chatSession("session-recent", "idle", "2026-05-14T04:00:00Z")]);
  window.history.pushState({}, "", "/dashboard/settings/clients");

  render(AppSidebarHost);

  const nav = screen.getByRole("list", { name: /settings sections/i });
  expect(within(nav).getByRole("link", { name: "Agent Clients" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  expect(within(nav).getByRole("link", { name: "Workspaces" })).toHaveAttribute(
    "href",
    "/dashboard/settings/workspaces",
  );
  expect(screen.queryByRole("button", { name: /New Chat/ })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^sessions$/i })).not.toBeInTheDocument();

  await fireEvent.click(within(nav).getByRole("link", { name: "Workspaces" }));
  expect(mocks.navigate).toHaveBeenLastCalledWith("/settings/workspaces");

  await fireEvent.click(screen.getByRole("button", { name: /Back to chat/ }));
  expect(mocks.navigate).toHaveBeenLastCalledWith("/");
});

test("the top bar titles the current settings section", () => {
  window.history.pushState({}, "", "/dashboard/settings/clients");

  render(TopBar);

  expect(
    within(screen.getByRole("banner")).getByRole("heading", { name: "Agent Clients" }),
  ).toBeInTheDocument();
});

test("settings shell renders the current settings page content", () => {
  window.history.pushState({}, "", "/dashboard/settings/workspaces");

  render(SettingsShellHost);

  expect(screen.getByText("Current settings page content")).toBeInTheDocument();
});

test("shows a sidebar loading failure instead of claiming there are no sessions", async () => {
  mocks.sidebarSessionsError.set("Failed to fetch");
  render(AppSidebarHost);
  expect(screen.getByRole("alert")).toHaveTextContent("Sidebar refresh failed");
  expect(screen.queryByText("No active sessions")).not.toBeInTheDocument();
});

function mockCurrentBot() {
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue([
    {
      bot_id: "bot-1",
      name: "Release Captain",
      description: null,
      avatar_shape: "classic",
      avatar_color: "blue",
      client_type: "pi",
      client_key: "default",
      model: "provider/model",
      reasoning_effort: null,
    },
  ]);
  window.history.pushState({}, "", "/dashboard/bots/bot-1");
}

test("Bot top bar opens the Bot edit dialog from its name and has no more menu", async () => {
  mockCurrentBot();
  render(TopBar);

  const banner = screen.getByRole("banner");
  await fireEvent.click(await within(banner).findByRole("button", { name: "Release Captain" }));

  const dialog = screen.getByRole("dialog", { name: "Edit Bot" });
  expect(within(dialog).getByRole("button", { name: "Delete Bot" })).toBeInTheDocument();
  expect(within(banner).queryByRole("button", { name: "More" })).not.toBeInTheDocument();
  vi.restoreAllMocks();
});

test("Desktop download center shows a button only while downloads exist", async () => {
  mockCurrentBot();
  render(AppShellHost);

  const banner = screen.getByRole("banner");
  await within(banner).findByRole("button", { name: "Release Captain" });
  expect(screen.queryByRole("button", { name: "Downloads" })).not.toBeInTheDocument();

  const task = downloads.start(
    "missing-device",
    "session",
    { path: "/tmp/report.txt" },
    "report.txt",
  );
  const button = await screen.findByRole("button", { name: "Downloads" });
  expect(within(button).getByTestId("downloads-count")).toHaveTextContent("1");
  await fireEvent.click(button);
  expect(screen.getByRole("dialog", { name: "Downloads" })).toBeInTheDocument();
  downloads.cancel(task.id);
  vi.restoreAllMocks();
});

test("sidebar refreshes every Bot status and todo count from live updates without opening its chat", async () => {
  const bots = ["first", "second"].map((id) => ({
    bot_id: id,
    name: id,
    description: null,
    avatar_shape: "classic" as const,
    avatar_color: "blue" as const,
    client_type: "pi",
    client_key: "default",
    model: "test",
    reasoning_effort: null,
  }));
  vi.spyOn(testDevice.api, "listBots").mockResolvedValue(bots);
  let status: BotWorkStatus = { kind: "idle" };
  let todos = 0;
  vi.spyOn(testDevice.api, "getBotMessages").mockImplementation(
    async (id) =>
      ({
        bot: bots.find((bot) => bot.bot_id === id)!,
        status,
        entries: [],
        next_before: null,
        generation_notice: null,
        open_todo_count: todos,
        notice_session_id: null,
        file_link_session: null,
      }) satisfies BotMessagePage,
  );
  const view = render(AppSidebarHost);
  const row = (await screen.findByText("second")).closest("a")!;
  await waitFor(() => expect(botStreams.has("second")).toBe(true));
  for (const next of [
    { kind: "processing" },
    { kind: "working", title: "Review" },
    { kind: "busy" },
    { kind: "idle" },
  ] satisfies BotWorkStatus[]) {
    status = next;
    todos += 1;
    botStreams.get("second")!({ type: "changed" });
    await waitFor(() =>
      expect(testDevice.queryClient.getQueryData(["bot-summary", "second"])).toEqual({
        status: next,
        open_todo_count: todos,
      }),
    );
    await waitFor(() =>
      expect(
        within(row).getByLabelText(todos === 1 ? "1 open todo" : `${todos} open todos`),
      ).toBeInTheDocument(),
    );
  }
  expect(row).toHaveAccessibleName("4 open todos second");
  view.unmount();
  expect(botStreams.has("second")).toBe(false);
  vi.restoreAllMocks();
});

test.each([
  ["/dashboard", "KeyG"],
  ["/dashboard/chat/current", "KeyC"],
  ["/dashboard/chat/current", "KeyW"],
])("consumes unused chat control keys on %s for %s", async (path, code) => {
  window.history.pushState({}, "", path);
  render(AppShellHost);
  expect(await fireEvent.keyDown(window, { code, altKey: true })).toBe(false);
});
