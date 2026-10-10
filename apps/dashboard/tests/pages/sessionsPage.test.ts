import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import SessionsPage from "../../src/pages/SessionsPage.svelte";
import type { SessionView } from "../../src/api/types";

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

  return {
    navigate: vi.fn(),
    active: writableStore<SessionView[]>([]),
    archived: writableStore<SessionView[]>([]),
    list: writableStore<SessionView[]>([]),
    pinned: writableStore<SessionView[]>([]),
    loading: writableStore(false),
    loadingMore: writableStore(false),
    error: writableStore<string | null>(null),
    nextCursor: writableStore<string | null>(null),
    activateOverview: vi.fn(() => () => undefined),
    loadMore: vi.fn(async () => [] as SessionView[]),
  };
});

vi.mock("$lib/navigation", () => ({ navigate: mocks.navigate }));
vi.mock("../../src/stores/sessions", () => ({
  activateSessionsPageOverview: mocks.activateOverview,
  sessionOverviewActiveSessions: mocks.active,
  sessionOverviewListSessions: mocks.list,
  sessionOverviewPinnedSessions: mocks.pinned,
  sessionOverviewLoading: mocks.loading,
  sessionOverviewLoadingMore: mocks.loadingMore,
  sessionOverviewError: mocks.error,
  sessionOverviewNextCursor: mocks.nextCursor,
  sessionsPageArchivedSessions: mocks.archived,
  loadMoreSessionOverview: mocks.loadMore,
}));

const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: "session-1",
  client_type: "pi",
  title: "Session one",
  handle: null,
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  state: "idle",
  current_turn_id: null,
  workspace_id: "workspace-1",
  workspace: null,
  pinned_at: null,
  archived_at: null,
  capabilities: { context_usage: "unsupported" },
  model: null,
  context_usage: null,
  lineage: null,
  created_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
  metadata: {},
  ...overrides,
});

beforeEach(() => {
  mocks.active.set([]);
  mocks.archived.set([]);
  mocks.list.set([]);
  mocks.pinned.set([]);
  mocks.loading.set(false);
  mocks.loadingMore.set(false);
  mocks.error.set(null);
  mocks.nextCursor.set(null);
  vi.clearAllMocks();
});

test("shows active sessions before the list and removes active duplicates", async () => {
  const active = session({ session_id: "active", title: "Active session", state: "busy" });
  const recent = session({ session_id: "recent", title: "Recent session", state: "exited" });
  mocks.active.set([active]);
  mocks.list.set([active, recent]);

  render(SessionsPage);

  expect(mocks.activateOverview).toHaveBeenCalledOnce();
  expect(within(screen.getByTestId("active-session-list")).getByRole("button")).toHaveTextContent(
    "Active session",
  );
  const listRows = within(screen.getByTestId("all-session-list")).getAllByRole("button");
  expect(listRows).toHaveLength(1);
  expect(listRows[0]).toHaveTextContent("Recent session");
  expect(listRows[0]).not.toHaveTextContent("exited");

  await fireEvent.click(listRows[0]);
  expect(mocks.navigate).toHaveBeenCalledWith("/chat/recent");
});

test("loads another list page when more sessions are available", async () => {
  mocks.list.set([session()]);
  mocks.nextCursor.set("next-page");

  render(SessionsPage);
  await fireEvent.click(screen.getByRole("button", { name: "Load more" }));

  expect(mocks.loadMore).toHaveBeenCalledOnce();
});

test("allows archived and pinned sessions to be viewed separately", async () => {
  mocks.archived.set([
    session({
      session_id: "archived",
      title: "Archived session",
      archived_at: "2026-05-15T00:00:00Z",
    }),
  ]);
  mocks.pinned.set([
    session({ session_id: "pinned", title: "Pinned session", pinned_at: "2026-05-15T00:00:00Z" }),
  ]);

  render(SessionsPage);

  await fireEvent.click(screen.getByRole("tab", { name: "Archived" }));
  expect(within(screen.getByTestId("archived-session-list")).getByRole("button")).toHaveTextContent(
    "Archived session",
  );

  await fireEvent.click(screen.getByRole("tab", { name: "Pinned" }));
  expect(within(screen.getByTestId("pinned-session-list")).getByRole("button")).toHaveTextContent(
    "Pinned session",
  );
});

test("shows the sessions loading failure without an empty state", () => {
  mocks.error.set("Failed to load");

  render(SessionsPage);

  expect(screen.getByRole("alert")).toHaveTextContent("Failed to load");
  expect(screen.queryByText("No sessions")).not.toBeInTheDocument();
});
