import { get } from "svelte/store";
import { beforeEach, expect, test, vi } from "vitest";
import type { SessionView } from "../../src/api/types";

const api = vi.hoisted(() => ({
  archiveSession: vi.fn(),
  cancelInboxMessage: vi.fn(),
  createSession: vi.fn(),
  dismissInboxMessage: vi.fn(),
  getSession: vi.fn(),
  getSessionOverview: vi.fn(),
  interruptSession: vi.fn(),
  listEvents: vi.fn(),
  listInboxMessages: vi.fn(),
  listTurns: vi.fn(),
  pinSession: vi.fn(),
  restartSession: vi.fn(),
  resumeSession: vi.fn(),
  submitInboxMessage: vi.fn(),
  terminateSession: vi.fn(),
  unarchiveSession: vi.fn(),
  unpinSession: vi.fn(),
  updateSession: vi.fn(),
}));

vi.mock("../../src/api/client", () => api);

function session(session_id: string): SessionView {
  return {
    session_id,
    client_type: "pi",
    title: session_id,
    handle: null,
    role: null,
    description: null,
    execution_profile_id: null,
    execution_profile_version: null,
    state: "idle",
    current_turn_id: null,
    workspace_id: null,
    workspace: null,
    pinned_at: null,
    archived_at: null,
    capabilities: {},
    model: null,
    context_usage: null,
    lineage: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    metadata: {},
  };
}

beforeEach(() => {
  vi.resetModules();
  Object.values(api).forEach((mock) => mock.mockReset());
});

test("loads the shared overview groups without requesting archived sessions", async () => {
  const pinned = session("pinned");
  const active = session("active");
  const recent = session("recent");
  api.getSessionOverview.mockResolvedValue({
    groups: {
      pinned: { sessions: [pinned] },
      active: { sessions: [active] },
      list: { sessions: [active, recent], next_cursor: "cursor-2" },
    },
  });
  const store = await import("../../src/stores/sessions");

  await store.loadSessionOverview();

  expect(get(store.sessionOverviewPinnedSessions)).toEqual([pinned]);
  expect(get(store.sessionOverviewActiveSessions)).toEqual([active]);
  expect(get(store.sessionOverviewListSessions)).toEqual([active, recent]);
  expect(get(store.sessionOverviewNextCursor)).toBe("cursor-2");
  expect(api.getSessionOverview).toHaveBeenCalledWith({
    sections: ["pinned", "active", "list"],
    limit: 50,
  });
});

test("appends cursor pages without duplicating sessions", async () => {
  const first = session("first");
  const second = session("second");
  api.getSessionOverview
    .mockResolvedValueOnce({
      groups: {
        pinned: { sessions: [] },
        active: { sessions: [] },
        list: { sessions: [first], next_cursor: "cursor-2" },
      },
    })
    .mockResolvedValueOnce({
      groups: { list: { sessions: [first, second], next_cursor: null } },
    });
  const store = await import("../../src/stores/sessions");
  await store.loadSessionOverview();

  await store.loadMoreSessionOverview();

  expect(get(store.sessionOverviewListSessions).map((item) => item.session_id)).toEqual([
    "first",
    "second",
  ]);
  expect(get(store.sessionOverviewNextCursor)).toBeNull();
  expect(api.getSessionOverview).toHaveBeenLastCalledWith({
    sections: ["list"],
    limit: 50,
    cursor: "cursor-2",
  });
});

test("loads all session page overview groups", async () => {
  const active = session("active");
  const recent = session("recent");
  const archived = session("archived");
  const pinned = session("pinned");
  api.getSessionOverview.mockResolvedValue({
    groups: {
      active: { sessions: [active] },
      list: { sessions: [active, recent], next_cursor: "cursor-2" },
      archived: { sessions: [archived] },
      pinned: { sessions: [pinned] },
    },
  });
  const store = await import("../../src/stores/sessions");

  const deactivate = store.activateSessionsPageOverview();
  await vi.waitFor(() => expect(get(store.sessionsPageArchivedSessions)).toEqual([archived]));

  expect(get(store.sessionOverviewActiveSessions)).toEqual([active]);
  expect(get(store.sessionOverviewListSessions)).toEqual([active, recent]);
  expect(get(store.sessionOverviewPinnedSessions)).toEqual([pinned]);
  expect(get(store.sessionOverviewNextCursor)).toBe("cursor-2");
  expect(api.getSessionOverview).toHaveBeenCalledWith({
    sections: ["pinned", "active", "list", "archived"],
    limit: 50,
  });
  deactivate();
});

test("stops requesting archived sessions after the sessions page closes", async () => {
  api.getSessionOverview.mockResolvedValue({
    groups: {
      pinned: { sessions: [] },
      active: { sessions: [] },
      list: { sessions: [], next_cursor: null },
      archived: { sessions: [] },
    },
  });
  const store = await import("../../src/stores/sessions");

  const deactivate = store.activateSessionsPageOverview();
  await vi.waitFor(() => expect(api.getSessionOverview).toHaveBeenCalledTimes(1));
  deactivate();
  await store.loadSessionOverview();

  expect(api.getSessionOverview).toHaveBeenLastCalledWith({
    sections: ["pinned", "active", "list"],
    limit: 50,
  });
});

test("appends session page list pages without duplicates", async () => {
  const first = session("first");
  const second = session("second");
  api.getSessionOverview
    .mockResolvedValueOnce({
      groups: {
        active: { sessions: [] },
        list: { sessions: [first], next_cursor: "cursor-2" },
        archived: { sessions: [] },
        pinned: { sessions: [] },
      },
    })
    .mockResolvedValueOnce({
      groups: { list: { sessions: [first, second], next_cursor: null } },
    });
  const store = await import("../../src/stores/sessions");
  const deactivate = store.activateSessionsPageOverview();
  await vi.waitFor(() => expect(get(store.sessionOverviewListSessions)).toEqual([first]));

  await store.loadMoreSessionOverview();

  expect(get(store.sessionOverviewListSessions).map((item) => item.session_id)).toEqual([
    "first",
    "second",
  ]);
  expect(get(store.sessionOverviewNextCursor)).toBeNull();
  deactivate();
});
