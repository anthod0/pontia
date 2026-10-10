import { QueryObserver } from "@tanstack/svelte-query";
import { beforeEach, expect, test, vi } from "vitest";
import type { SessionOverviewView, SessionView } from "../../src/api/types";
import { queryClient } from "../../src/lib/queryClient";
import {
  invalidateSessionOverview,
  snapshotSessionOverview,
} from "../../src/queries/sessionOverview";

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
  queryClient.clear();
});

test("combines paged overview results without duplicating sessions", () => {
  const first = session("first");
  const second = session("second");
  const snapshot = snapshotSessionOverview({
    pages: [
      {
        groups: {
          pinned: { sessions: [] },
          active: { sessions: [] },
          list: { sessions: [first], next_cursor: "cursor-2" },
        },
      },
      {
        groups: {
          list: { sessions: [first, second], next_cursor: null },
        },
      },
    ],
    pageParams: [null, "cursor-2"],
  });

  expect(snapshot.list.map((item) => item.session_id)).toEqual(["first", "second"]);
  expect(snapshot.nextCursor).toBeNull();
});

test("invalidating session overview refetches active overview queries", async () => {
  const overview: SessionOverviewView = {
    groups: {
      pinned: { sessions: [] },
      active: { sessions: [] },
      list: { sessions: [], next_cursor: null },
    },
  };
  const queryFn = vi.fn(async () => overview);
  const observer = new QueryObserver(queryClient, {
    queryKey: ["sessions", "overview", { includeArchived: false }],
    queryFn,
  });
  const unsubscribe = observer.subscribe(() => undefined);
  await observer.refetch();
  queryFn.mockClear();

  await invalidateSessionOverview();

  expect(queryFn).toHaveBeenCalledOnce();
  unsubscribe();
});
