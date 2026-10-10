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
    sessions: writableStore<SessionView[]>([]),
    sessionsLoading: writableStore(false),
    sessionsError: writableStore<string | null>(null),
    loadSessions: vi.fn(async () => [] as SessionView[]),
  };
});

vi.mock("$lib/navigation", () => ({ navigate: mocks.navigate }));
vi.mock("../../src/stores/sessions", () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  sessionsError: mocks.sessionsError,
  loadSessions: mocks.loadSessions,
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
  mocks.sessions.set([]);
  mocks.sessionsLoading.set(false);
  mocks.sessionsError.set(null);
  vi.clearAllMocks();
});

test("loads all unarchived sessions, orders them by update time, and opens the selected session", async () => {
  mocks.sessions.set([
    session({ session_id: "older", title: "Older session" }),
    session({
      session_id: "archived",
      title: "Archived session",
      archived_at: "2026-05-15T00:00:00Z",
      updated_at: "2026-05-16T00:00:00Z",
    }),
    session({
      session_id: "newer",
      title: "Newer session",
      state: "exited",
      updated_at: "2026-05-15T00:00:00Z",
    }),
  ]);

  render(SessionsPage);

  expect(mocks.loadSessions).toHaveBeenCalledWith({ includePinned: true, limit: 200 });
  const rows = within(screen.getByTestId("all-session-list")).getAllByRole("button");
  expect(rows.map((row) => row.textContent)).toEqual([
    expect.stringContaining("Newer session"),
    expect.stringContaining("Older session"),
  ]);
  expect(screen.queryByText("Archived session")).not.toBeInTheDocument();

  await fireEvent.click(rows[0]);
  expect(mocks.navigate).toHaveBeenCalledWith("/chat/newer");
});

test("shows the sessions loading failure without an empty state", () => {
  mocks.sessionsError.set("Failed to load");

  render(SessionsPage);

  expect(screen.getByRole("alert")).toHaveTextContent("Failed to load");
  expect(screen.queryByText("No sessions")).not.toBeInTheDocument();
});
