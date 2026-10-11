import { render } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { token } from "../src/stores/auth";
import { dashboardStreamCursor, sseStatus } from "../src/stores/connection";
import { loadSessionDetail, selectSession, sessionDetail } from "../src/stores/sessions";
import { fetchWorkspaces } from "../src/queries/workspaces";
import { startEventStream, stopEventStream } from "../src/services/eventStream";
import { refreshDashboardSnapshot } from "../src/services/dashboardSnapshotRefresh";
import { selectedWorkflowId } from "../src/stores/workflows";
import { workflowKeys } from "../src/queries/workflows";
import type { WorkflowDetailView } from "../src/api/types";
import * as timeline from "../src/stores/timeline";
import { queryClient } from "../src/queries/queryClient";
import type { SessionOverviewSnapshot } from "../src/queries/sessionOverview";
import SessionOverviewQueryHarness from "./components/SessionOverviewQueryHarness.svelte";
import WorkspacesQueryHarness from "./components/WorkspacesQueryHarness.svelte";

let online = false;
let state = "busy";
let clientType = "pi";
let supportsTimeline = false;
let streams: ReadableStreamDefaultController<Uint8Array>[];
let requests: string[];

function envelope(data: unknown) {
  return new Response(JSON.stringify({ data, error: null }), { status: 200 });
}

beforeEach(() => {
  vi.useFakeTimers();
  online = false;
  state = "busy";
  clientType = "pi";
  supportsTimeline = false;
  streams = [];
  requests = [];
  queryClient.clear();
  token.set("test-token");
  selectSession("current");
  dashboardStreamCursor.set(null);
  timeline.resetTimelineState();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      expect(init?.method ?? "GET").toBe("GET");
      const path = String(input).split("?")[0].replace("/api/v1", "");
      requests.push(path);
      if (!online) throw new Error("Offline");
      if (path === "/dashboard/events/stream") {
        return new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              streams.push(controller);
              init?.signal?.addEventListener(
                "abort",
                () => {
                  try {
                    controller.error(new DOMException("Aborted", "AbortError"));
                  } catch {
                    /* already closed */
                  }
                },
                { once: true },
              );
            },
          }),
        );
      }
      const session = {
        session_id: "current",
        client_type: clientType,
        state,
        capabilities: { timeline: supportsTimeline, topology: supportsTimeline },
      };
      if (path === "/sessions/overview") {
        return new Response(
          JSON.stringify({
            groups: {
              pinned: { sessions: [] },
              active: { sessions: [session] },
              list: { sessions: [session], next_cursor: null },
            },
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        );
      }
      const data: Record<string, unknown> = {
        "/workspaces": { workspaces: [{ workspace_id: "workspace" }] },
        "/agent-profiles": { agent_profiles: [] },
        "/workflows": { workflows: [] },
        "/workflows/workflow": {
          workflow: { workflow_id: "workflow", nodes: [], state: "completed" },
        },
        "/sessions/current": { session },
        "/sessions/current/turns": {
          turns: [{ turn_id: "turn-2", state: state === "busy" ? "running" : "completed" }],
        },
        "/sessions/current/inbox/messages": { inbox_messages: [] },
        "/sessions/current/events": { events: [] },
      };
      if (!(path in data)) throw new Error(`Unexpected endpoint: ${path}`);
      return envelope(data[path]);
    }),
  );
});

afterEach(async () => {
  stopEventStream();
  selectSession(null);
  timeline.resetTimelineState();
  selectedWorkflowId.set(null);
  token.set("");
  queryClient.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

test.each(["pi", "codex"])(
  "recovers an offline %s route on first successful connection without events",
  async (client) => {
    clientType = client;
    let overview: SessionOverviewSnapshot | undefined;
    let workspaceViews: { workspace_id: string }[] = [];
    render(SessionOverviewQueryHarness, {
      onSnapshot: (snapshot) => {
        overview = snapshot;
      },
    });
    render(WorkspacesQueryHarness, {
      onData: (workspaces) => {
        workspaceViews = workspaces;
      },
    });
    await vi.waitFor(() => expect(requests).toContain("/sessions/overview"));
    await Promise.allSettled([loadSessionDetail("current"), fetchWorkspaces()]);
    startEventStream();
    await vi.waitFor(() => expect(get(sseStatus)).toBe("reconnecting"));
    expect(get(sessionDetail)).toBeNull();

    online = true;
    state = "idle";
    await vi.advanceTimersByTimeAsync(1500);
    await vi.waitFor(() => expect(get(sessionDetail)?.session.state).toBe("idle"));
    await vi.waitFor(() => {
      expect(overview?.active[0]?.client_type).toBe(client);
      expect(overview?.active[0]?.state).toBe("idle");
    });
    expect(workspaceViews[0]?.workspace_id).toBe("workspace");
    expect(get(sessionDetail)?.turns[0]?.state).toBe("completed");
    const readCount = requests.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(requests).toHaveLength(readCount);
  },
);

test.each(["idle", "busy"])(
  "normal stream reconnect restores the latest %s snapshot with no business events",
  async (recoveredState) => {
    online = true;
    startEventStream();
    await vi.waitFor(() => expect(get(sessionDetail)?.session.state).toBe("busy"));
    state = recoveredState;
    streams[0].close();
    await vi.advanceTimersByTimeAsync(1500);
    await vi.waitFor(() => expect(streams).toHaveLength(2));
    await vi.waitFor(() => expect(get(sessionDetail)?.session.state).toBe(recoveredState));
    expect(requests.filter((path) => path === "/sessions/current")).toHaveLength(2);
  },
);

test("recovery refreshes shared workflow consumers", async () => {
  online = true;
  selectedWorkflowId.set("workflow");
  await refreshDashboardSnapshot({ reason: "sse_open" });
  expect(
    queryClient.getQueryData<WorkflowDetailView>(workflowKeys.detail("workflow"))?.workflow_id,
  ).toBe("workflow");
  expect(requests).toContain("/workflows");
});

test("recovers the Session independently of native history failure and uses its declared topology", async () => {
  online = true;
  supportsTimeline = true;
  const history = vi
    .spyOn(timeline, "loadSessionTimeline")
    .mockRejectedValueOnce(new Error("Native history unavailable"));
  await refreshDashboardSnapshot({ reason: "sse_open" });
  expect(get(sessionDetail)?.session.state).toBe("busy");
  expect(history).toHaveBeenCalledWith("current", {
    mode: "rebuild",
    latestTurnId: "turn-2",
    topology: true,
  });
});

test("keeps a recovery requested during an older snapshot in flight", async () => {
  online = true;
  render(WorkspacesQueryHarness, { onData: () => undefined });
  await fetchWorkspaces();
  const { listWorkspaces } = await import("../src/api/client");
  const api = await import("../src/api/client");
  let release!: () => void;
  vi.spyOn(api, "listWorkspaces")
    .mockImplementationOnce(async () => {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return [];
    })
    .mockImplementation(listWorkspaces);
  const first = refreshDashboardSnapshot({ reason: "sse_open" });
  await vi.waitFor(() => expect(get(sessionDetail)?.session.state).toBe("busy"));
  state = "idle";
  const recovery = refreshDashboardSnapshot({ reason: "sse_open" });
  release();
  await Promise.all([first, recovery]);
  expect(get(sessionDetail)?.session.state).toBe("idle");
  expect(queryClient.getQueryData(["workspaces", "list"])).toHaveLength(1);
  expect(requests.filter((path) => path === "/sessions/current")).toHaveLength(2);
});

test("replays native history from the last loaded turn rather than skipping offline turns", async () => {
  online = true;
  supportsTimeline = true;
  timeline.timelineState.update((value) => ({
    ...value,
    sessionId: "current",
    mode: "tree",
    status: "ready",
    latestTurnId: "turn-1",
  }));
  const refresh = vi.spyOn(timeline, "refreshSessionTimeline").mockResolvedValue(true);
  await refreshDashboardSnapshot({ reason: "sse_open" });
  expect(refresh).toHaveBeenCalledWith("current", "turn-1");
  expect(get(sessionDetail)?.turns[0]?.turn_id).toBe("turn-2");
});

test.each([409, 410])(
  "recovers from cursor rejection %s and then opens a fresh stream without a loop",
  async (status) => {
    online = true;
    dashboardStreamCursor.set("expired");
    const fetchSnapshot = fetch;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        if (String(input).includes("/dashboard/events/stream?after=expired"))
          return new Response("", { status });
        return fetchSnapshot(input, init);
      }),
    );
    startEventStream();
    await vi.waitFor(() => expect(get(sessionDetail)?.session.state).toBe("busy"));
    expect(get(dashboardStreamCursor)).toBeNull();
    state = "idle";
    await vi.advanceTimersByTimeAsync(1500);
    await vi.waitFor(() => expect(get(sessionDetail)?.session.state).toBe("idle"));
    expect(streams).toHaveLength(1);
    const readCount = requests.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(requests).toHaveLength(readCount);
  },
);
