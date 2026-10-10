import { fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { SessionOverviewView, SessionView } from "../../src/api/types";
import { queryClient } from "../../src/lib/queryClient";
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
