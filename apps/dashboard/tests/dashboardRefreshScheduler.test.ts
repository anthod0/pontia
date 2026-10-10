import { expect, test } from "vitest";
import { createDashboardRefreshScheduler } from "../src/services/dashboardRefreshScheduler.ts";
import type { DashboardStreamEvent } from "../src/api/types.ts";

function sessionEvent(type = "session.updated", sessionId = "session-1"): DashboardStreamEvent {
  return {
    kind: "session_event",
    id: `event-session-${type}`,
    occurred_at: "2026-05-14T00:00:00Z",
    event: {
      event_id: `event-session-${type}`,
      session_id: sessionId,
      turn_id: null,
      source: "runtime",
      type,
      time: "2026-05-14T00:00:00Z",
      payload: {},
    },
  };
}

function scheduler(
  calls: string[],
  options: {
    sessionId?: string | null;
    workflowId?: string | null;
    workflowSessionIds?: string[];
  } = {},
) {
  return createDashboardRefreshScheduler({
    delayMs: 0,
    getSelectedSessionId: () => options.sessionId ?? null,
    getSelectedWorkflowId: () => options.workflowId ?? null,
    getSelectedWorkflowSessionIds: () => options.workflowSessionIds ?? [],
    loadWorkspaces: async () => {
      calls.push("workspaces");
    },
    loadWorkflows: async () => {
      calls.push("workflows");
    },
    refreshSession: async (sessionId) => {
      calls.push(`session:${sessionId}`);
    },
    refreshWorkflow: async (workflowId) => {
      calls.push(`workflow:${workflowId}`);
    },
  });
}

test("coalesces bursts of dashboard stream events into one refresh per affected resource", async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { sessionId: "session-1" });
  refreshes.handleEvent(sessionEvent());
  refreshes.handleEvent(sessionEvent());
  await refreshes.flushNow();
  expect(calls.sort()).toEqual(["session:session-1", "workflows"].sort());
});

test("refreshes selected session detail and workflow list for a session event", async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { sessionId: "session-1" });
  refreshes.handleEvent(sessionEvent());
  await refreshes.flushNow();
  expect(calls.sort()).toEqual(["session:session-1", "workflows"].sort());
});

test("refreshes selected workflow when the event belongs to one of its sessions", async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { workflowId: "wf-1", workflowSessionIds: ["session-2"] });
  refreshes.handleEvent(sessionEvent("session.updated", "session-2"));
  await refreshes.flushNow();
  expect(calls.sort()).toEqual(["workflow:wf-1", "workflows"].sort());
});

test("discards detail refreshes queued for a route that is no longer selected", async () => {
  const calls: string[] = [];
  const selection = {
    sessionId: "session-1",
    workflowId: "wf-1",
    workflowSessionIds: ["session-1"],
  };
  const refreshes = scheduler(calls, selection);
  refreshes.handleEvent(sessionEvent());
  selection.sessionId = "session-2";
  selection.workflowId = "wf-2";
  await refreshes.flushNow();
  expect(calls).toEqual(["workflows"]);
});
