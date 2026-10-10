import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { tick } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";
import AppSidebarHost from "./components/layout/AppSidebarHost.svelte";
import AppShellHost from "./components/layout/AppShellHost.svelte";
import TopBarHost from "./components/layout/TopBarHost.svelte";
import SettingsShellHost from "./components/settings/SettingsShellHost.svelte";

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

vi.mock("$lib/navigation", () => ({ navigate: mocks.navigate }));
vi.mock("../src/services/eventStream", () => ({
  startEventStream: mocks.startEventStream,
  stopEventStream: mocks.stopEventStream,
}));
vi.mock("../src/stores/sessions", () => ({
  sidebarPinnedSessions: mocks.sidebarPinnedSessions,
  sidebarActiveSessions: mocks.sidebarActiveSessions,
  sidebarRecentSessions: mocks.sidebarRecentSessions,
  sidebarSessionsLoading: mocks.sidebarSessionsLoading,
  sidebarSessionsLoadingMore: mocks.sidebarSessionsLoadingMore,
  sidebarSessionsError: mocks.sidebarSessionsError,
  sidebarSessionsNextCursor: mocks.sidebarSessionsNextCursor,
  sessionDetail: mocks.sessionDetail,
  sessionDetailError: mocks.sessionDetailError,
  loadMoreSidebarSessions: mocks.loadMoreSidebarSessions,
  updateSessionTitle: mocks.updateSessionTitle,
  pinSession: mocks.pinSession,
  unpinSession: mocks.unpinSession,
  archiveSession: mocks.archiveSession,
  terminateSession: mocks.terminateSession,
}));
beforeEach(() => {
  window.history.pushState({}, "", "/dashboard");
  mocks.sessions.set([]);
  mocks.sidebarPinnedSessions.set([]);
  mocks.sidebarRecentSessions.set([]);
  mocks.sidebarSessionsNextCursor.set(null);
  mocks.sidebarSessionsLoadingMore.set(false);
  mocks.sidebarSessionsLoading.set(false);
  mocks.sidebarSessionsError.set(null);
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
    execution_profile_id: null,
    execution_profile_version: null,
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

test("opens the all sessions page from the Recent Sessions header", async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole("button", { name: "Open all sessions" }));

  expect(mocks.navigate).toHaveBeenCalledWith("/sessions");
});

test("sidebar renders active sessions before paged recent sessions without duplicating active sessions", () => {
  const active = chatSession("session-active", "idle", "2026-05-14T04:00:00Z");
  const duplicate = chatSession("session-active", "idle", "2026-05-14T04:00:00Z");
  const older = chatSession("session-older", "exited", "2026-05-14T01:00:00Z");
  mocks.sessions.set([active]);
  mocks.sidebarRecentSessions.set([duplicate, older]);

  render(AppSidebarHost);

  const activeButton = screen.getByText("session-active").closest("button");
  const olderButton = screen.getByText("session-older").closest("button");
  expect(screen.getAllByText("session-active")).toHaveLength(1);
  expect(
    activeButton?.compareDocumentPosition(olderButton as Node) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
});

test("sidebar collapses the pinned sessions group", async () => {
  mocks.sidebarPinnedSessions.set([
    chatSession("session-pinned", "idle", "2026-05-14T01:00:00Z", "2026-05-14T02:00:00Z"),
  ]);
  render(AppSidebarHost);

  const toggle = screen.getByRole("button", { name: /pinned sessions/i });
  expect(screen.getByText("session-pinned")).toBeInTheDocument();
  await fireEvent.click(toggle);
  expect(screen.queryByText("session-pinned")).not.toBeInTheDocument();
});

test("sidebar loads the next page when its session list reaches the bottom", async () => {
  mocks.sessions.set([chatSession("session-active", "idle", "2026-05-14T04:00:00Z")]);
  mocks.sidebarSessionsNextCursor.set("next-page");
  render(AppSidebarHost);

  const scroller = screen.getByText("Recent Sessions").closest(".overflow-y-auto") as HTMLElement;
  Object.defineProperties(scroller, {
    scrollHeight: { configurable: true, value: 500 },
    scrollTop: { configurable: true, value: 420 },
    clientHeight: { configurable: true, value: 80 },
  });
  await fireEvent.scroll(scroller);

  expect(mocks.loadMoreSidebarSessions).toHaveBeenCalledTimes(1);
});

test("sidebar shows semantic status dots except for terminal sessions, and opens chat for the selected session", async () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      handle: "main",
      role: "coder",
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
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
      execution_profile_id: null,
      execution_profile_version: null,
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
      execution_profile_id: null,
      execution_profile_version: null,
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

  expect(screen.getByText("Recent Sessions")).toBeInTheDocument();
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
      execution_profile_id: null,
      execution_profile_version: null,
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
      execution_profile_id: null,
      execution_profile_version: null,
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
      execution_profile_id: null,
      execution_profile_version: null,
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
      execution_profile_id: null,
      execution_profile_version: null,
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
  mocks.sessions.set([
    {
      session_id: "session-pinned",
      client_type: "pi",
      title: "Pinned title",
      handle: "main",
      role: "coder",
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
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

test("sidebar only marks new chat active on the default route", () => {
  window.history.pushState({}, "", "/dashboard");

  render(AppSidebarHost);

  const chat = screen.getByText("New Chat").closest("button");

  expect(chat).not.toBeNull();

  expect(chat).toHaveAttribute("data-active", "true");
});

test("sidebar New Chat notifies mounted route components about the route change", async () => {
  window.history.pushState({}, "", "/dashboard/chat/session-active");
  const popstateListener = vi.fn();
  window.addEventListener("popstate", popstateListener);

  render(AppSidebarHost);
  await fireEvent.click(screen.getByText("New Chat"));

  expect(mocks.navigate).toHaveBeenCalledWith("/");
  expect(popstateListener).toHaveBeenCalledTimes(1);
  window.removeEventListener("popstate", popstateListener);
});

test("sidebar highlights the matching recent session on chat routes", () => {
  mocks.sessions.set([
    {
      session_id: "session-active",
      client_type: "pi",
      handle: "main",
      role: "coder",
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
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
      execution_profile_id: null,
      execution_profile_version: null,
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

test("top bar trigger closes and reopens the sidebar", async () => {
  const onOpenChange = vi.fn();
  render(TopBarHost, { props: { onOpenChange } });

  const trigger = within(screen.getByRole("banner")).getByRole("button", {
    name: /toggle sidebar/i,
  });
  await fireEvent.click(trigger);
  await fireEvent.click(trigger);

  expect(onOpenChange.mock.calls).toEqual([[false], [true]]);
});

test("sidebar settings button navigates directly to common settings without document reload", async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole("button", { name: /settings/i }));
  expect(mocks.navigate).toHaveBeenCalledWith("/settings/common");
});

test("chat shortcuts switch among active sessions on chat routes", async () => {
  window.history.pushState({}, "", "/dashboard/chat/session-older");
  mocks.sessions.set([
    chatSession("session-busy", "busy", "2026-05-14T04:00:00Z"),
    chatSession("session-error", "error", "2026-05-14T03:00:00Z"),
    chatSession("session-older", "idle", "2026-05-14T02:00:00Z"),
    chatSession("session-exited", "exited", "2026-05-14T01:00:00Z"),
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "j", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-busy");

  window.history.pushState({}, "", "/dashboard/chat/session-busy");
  await fireEvent.keyDown(window, { key: "k", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-older");
});

test("chat help shortcut opens a kbd shortcut reference dialog", async () => {
  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "?", altKey: true, shiftKey: true });

  const dialog = screen.getByRole("dialog", { name: /keyboard shortcuts/i });
  expect(within(dialog).getByText(/next active chat/i)).toBeInTheDocument();
  expect(within(dialog).getByText(/focus chat input/i)).toBeInTheDocument();
});

test("chat header help button opens the shortcuts dialog", async () => {
  render(AppShellHost);

  const helpButton = screen.getByRole("button", { name: /keyboard shortcuts/i });

  await fireEvent.click(helpButton);

  expect(screen.getByRole("dialog", { name: /keyboard shortcuts/i })).toBeInTheDocument();
});

test("chat numeric shortcuts open active sessions by sidebar order and skip inactive sessions", async () => {
  mocks.sessions.set([
    chatSession("session-recent", "idle", "2026-05-14T04:00:00Z"),
    chatSession("session-pinned", "idle", "2026-05-14T01:00:00Z", "2026-05-14T05:00:00Z"),
    chatSession("session-error", "error", "2026-05-14T06:00:00Z"),
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: "1", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-pinned");

  await fireEvent.keyDown(window, { key: "2", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/chat/session-recent");

  await fireEvent.keyDown(window, { key: "3", altKey: true });
  expect(mocks.navigate).not.toHaveBeenLastCalledWith("/chat/session-error");
});

test("chat shortcuts are scoped to chat routes and do not interrupt typing", async () => {
  mocks.sessions.set([chatSession("session-recent", "idle", "2026-05-14T04:00:00Z")]);

  render(AppShellHost);

  window.history.pushState({}, "", "/dashboard/settings/common");
  await fireEvent.keyDown(window, { key: "1", altKey: true });
  expect(mocks.navigate).not.toHaveBeenCalled();

  window.history.pushState({}, "", "/dashboard");
  const input = document.createElement("textarea");
  document.body.appendChild(input);
  input.focus();
  await fireEvent.keyDown(window, { key: "1", altKey: true });
  expect(mocks.navigate).not.toHaveBeenCalled();
  input.remove();
});

test("chat new and focus shortcuts work on chat routes", async () => {
  render(AppShellHost);
  const input = document.createElement("textarea");
  input.setAttribute("data-chat-shortcut-focus-target", "true");
  document.body.appendChild(input);

  await fireEvent.keyDown(window, { key: "n", altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith("/");

  await fireEvent.keyDown(window, { key: "l", altKey: true });
  expect(document.activeElement).toBe(input);
  input.remove();
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

  await fireEvent.keyDown(window, { key: "n", altKey: true });

  expect(mocks.navigate).toHaveBeenLastCalledWith("/", { workspace: "workspace-current" });
});

test("settings shell renders a persistent vertical side switcher around page content", () => {
  window.history.pushState({}, "", "/dashboard/settings/workspaces");

  render(SettingsShellHost);

  const nav = screen.getByRole("navigation", { name: /settings sections/i });
  expect(within(nav).getByRole("link", { name: /^common$/i })).toHaveAttribute(
    "href",
    "/dashboard/settings/common",
  );
  const activeLink = within(nav).getByRole("link", { name: /^workspaces$/i });
  expect(activeLink).toHaveAttribute("aria-current", "page");

  const content = screen.getByText("Current settings page content");
  expect(content).toBeInTheDocument();
});

test("settings shell section switcher uses router navigation instead of a document reload", async () => {
  window.history.pushState({}, "", "/dashboard/settings/common");
  render(SettingsShellHost);

  await fireEvent.click(screen.getByRole("link", { name: /^workspaces$/i }));

  expect(mocks.navigate).toHaveBeenCalledWith("/settings/workspaces");
});

test("shows a sidebar loading failure instead of claiming there are no sessions", async () => {
  mocks.sidebarSessionsError.set("Failed to fetch");
  render(AppSidebarHost);
  expect(screen.getByRole("alert")).toHaveTextContent("Sidebar refresh failed");
  expect(screen.queryByText("No active sessions")).not.toBeInTheDocument();
  mocks.sidebarSessionsError.set(null);
  await tick();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
