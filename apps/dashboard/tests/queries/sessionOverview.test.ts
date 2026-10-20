import { fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { SessionOverviewView, SessionView } from "../../src/api/types";
import { queryClient } from "../../src/queries/queryClient";
import {
  invalidateSessionOverview,
  type SessionOverviewSnapshot,
} from "../../src/queries/sessionOverview";
import SessionOverviewQueryHarness from "../components/SessionOverviewQueryHarness.svelte";

function session(session_id: string, title = session_id): SessionView {
  return {
    session_id,
    client_type: "pi",
    title,
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

function overview(groups: SessionOverviewView["groups"]): Response {
  return new Response(JSON.stringify({ groups }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

beforeEach(() => {
  queryClient.clear();
});

afterEach(() => {
  queryClient.clear();
  vi.unstubAllGlobals();
});

test("loads another overview page and combines sessions without duplicates", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input), window.location.origin);
      if (url.searchParams.get("cursor") === "cursor-2") {
        return overview({
          list: {
            sessions: [session("first"), session("second")],
            next_cursor: null,
          },
        });
      }
      return overview({
        pinned: { sessions: [] },
        active: { sessions: [] },
        list: { sessions: [session("first")], next_cursor: "cursor-2" },
      });
    }),
  );
  let latest: SessionOverviewSnapshot | undefined;
  render(SessionOverviewQueryHarness, {
    onSnapshot: (snapshot) => {
      latest = snapshot;
    },
  });

  await vi.waitFor(() => expect(latest?.nextCursor).toBe("cursor-2"));
  await fireEvent.click(screen.getByTestId("load-more-sessions"));

  await vi.waitFor(() => {
    expect(latest?.list.map((item) => item.session_id)).toEqual(["first", "second"]);
    expect(latest?.nextCursor).toBeNull();
  });
});

test("workspace switches isolate pages and cursors without filtering the Sessions page", async () => {
  const requests: URL[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input), window.location.origin);
      requests.push(url);
      const workspace = url.searchParams.get("workspace_id") ?? "all";
      const cursor = url.searchParams.get("cursor");
      return overview({
        list: {
          sessions: [session(`${workspace}-${cursor ? "second" : "first"}`)],
          next_cursor: cursor ? null : `${workspace}-cursor`,
        },
      });
    }),
  );
  let sidebar: SessionOverviewSnapshot | undefined;
  let sessionsPage: SessionOverviewSnapshot | undefined;
  const sidebarView = render(SessionOverviewQueryHarness, {
    workspaceId: "workspace-1",
    onSnapshot: (snapshot) => {
      sidebar = snapshot;
    },
  });
  render(SessionOverviewQueryHarness, {
    includeArchived: true,
    onSnapshot: (snapshot) => {
      sessionsPage = snapshot;
    },
  });
  await vi.waitFor(() => {
    expect(sidebar?.list.map((item) => item.session_id)).toEqual(["workspace-1-first"]);
    expect(sessionsPage?.list.map((item) => item.session_id)).toEqual(["all-first"]);
  });
  await fireEvent.click(screen.getAllByTestId("load-more-sessions")[0]);
  await vi.waitFor(() =>
    expect(sidebar?.list.map((item) => item.session_id)).toEqual([
      "workspace-1-first",
      "workspace-1-second",
    ]),
  );

  await sidebarView.rerender({ workspaceId: "workspace-2" });
  await vi.waitFor(() =>
    expect(sidebar?.list.map((item) => item.session_id)).toEqual(["workspace-2-first"]),
  );
  await fireEvent.click(screen.getAllByTestId("load-more-sessions")[0]);
  await vi.waitFor(() =>
    expect(sidebar?.list.map((item) => item.session_id)).toEqual([
      "workspace-2-first",
      "workspace-2-second",
    ]),
  );
  expect(
    requests
      .filter((url) => url.searchParams.has("cursor"))
      .map((url) => [url.searchParams.get("workspace_id"), url.searchParams.get("cursor")]),
  ).toEqual([
    ["workspace-1", "workspace-1-cursor"],
    ["workspace-2", "workspace-2-cursor"],
  ]);
  expect(sessionsPage?.list.map((item) => item.session_id)).toEqual(["all-first"]);
  expect(sessionsPage?.nextCursor).toBe("all-cursor");

  await sidebarView.rerender({ workspaceId: undefined });
  await vi.waitFor(() =>
    expect(sidebar?.list.map((item) => item.session_id)).toEqual(["all-first"]),
  );
  expect(sidebar?.nextCursor).toBe("all-cursor");
});

test("a late workspace response cannot replace the newly selected workspace", async () => {
  let resolveFirst!: (response: Response) => void;
  const firstResponse = new Promise<Response>((resolve) => {
    resolveFirst = resolve;
  });
  const fetch = vi.fn(async (input: RequestInfo | URL) => {
    const url = new URL(String(input), window.location.origin);
    if (url.searchParams.get("workspace_id") === "workspace-1") return firstResponse;
    return overview({ list: { sessions: [session("second-workspace")], next_cursor: null } });
  });
  vi.stubGlobal("fetch", fetch);
  let latest: SessionOverviewSnapshot | undefined;
  const view = render(SessionOverviewQueryHarness, {
    workspaceId: "workspace-1",
    onSnapshot: (snapshot) => {
      latest = snapshot;
    },
  });
  await vi.waitFor(() => expect(fetch).toHaveBeenCalled());
  await view.rerender({ workspaceId: "workspace-2" });
  await vi.waitFor(() =>
    expect(latest?.list.map((item) => item.session_id)).toEqual(["second-workspace"]),
  );

  resolveFirst(
    overview({ list: { sessions: [session("first-workspace")], next_cursor: "first-cursor" } }),
  );
  await firstResponse;
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(latest?.list.map((item) => item.session_id)).toEqual(["second-workspace"]);
  expect(latest?.nextCursor).toBeNull();
});

test("invalidating the overview refreshes active archived and non-archived queries", async () => {
  const calls = { archived: 0, current: 0 };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input), window.location.origin);
      const archived = url.searchParams.get("sections")?.includes("archived") === true;
      const kind = archived ? "archived" : "current";
      calls[kind] += 1;
      const current = session(kind, `${kind}-${calls[kind]}`);
      return overview({
        pinned: { sessions: [] },
        active: { sessions: [] },
        list: { sessions: [current], next_cursor: null },
        ...(archived ? { archived: { sessions: [current] } } : {}),
      });
    }),
  );
  let currentSnapshot: SessionOverviewSnapshot | undefined;
  let archivedSnapshot: SessionOverviewSnapshot | undefined;
  render(SessionOverviewQueryHarness, {
    onSnapshot: (snapshot) => {
      currentSnapshot = snapshot;
    },
  });
  render(SessionOverviewQueryHarness, {
    includeArchived: true,
    onSnapshot: (snapshot) => {
      archivedSnapshot = snapshot;
    },
  });
  await vi.waitFor(() => {
    expect(currentSnapshot?.list[0]?.title).toBe("current-1");
    expect(archivedSnapshot?.archived[0]?.title).toBe("archived-1");
  });

  await invalidateSessionOverview();

  await vi.waitFor(() => {
    expect(currentSnapshot?.list[0]?.title).toBe("current-2");
    expect(archivedSnapshot?.archived[0]?.title).toBe("archived-2");
  });
});
