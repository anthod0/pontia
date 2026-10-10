import { afterEach, expect, test } from "vitest";
import type { DashboardStreamEvent } from "../../src/api/types";
import {
  clearDashboardQueries,
  invalidateQueriesAfterConnectionRecovery,
  invalidateQueriesForDashboardEvent,
} from "../../src/queries/dashboardInvalidation";
import { queryClient } from "../../src/queries/queryClient";
import { sessionOverviewKeys } from "../../src/queries/sessionOverview";
import { sessionKeys } from "../../src/queries/sessions";
import { workflowKeys } from "../../src/queries/workflows";

function sessionEvent(type: string, sessionId = "session-1"): DashboardStreamEvent {
  return {
    kind: "session_event",
    id: `stream-${type}`,
    occurred_at: "2026-05-14T00:00:00Z",
    event: {
      event_id: `event-${type}`,
      session_id: sessionId,
      turn_id: null,
      source: "runtime",
      type,
      time: "2026-05-14T00:00:00Z",
      payload: {},
    },
  };
}

function isInvalidated(queryKey: readonly unknown[]): boolean | undefined {
  return queryClient.getQueryState(queryKey)?.isInvalidated;
}

afterEach(() => {
  queryClient.clear();
});

test("a session event invalidates its session snapshots and every overview variant", async () => {
  const currentOverviewKey = [...sessionOverviewKeys.all, { includeArchived: false }] as const;
  const archivedOverviewKey = [...sessionOverviewKeys.all, { includeArchived: true }] as const;
  const affectedDetailKey = sessionKeys.detail("session-1");
  const affectedModelsKey = sessionKeys.models("session-1");
  const otherDetailKey = sessionKeys.detail("session-2");
  const workflowRevisionKey = workflowKeys.revision("workflow-1", 1);

  for (const key of [
    currentOverviewKey,
    archivedOverviewKey,
    affectedDetailKey,
    affectedModelsKey,
    otherDetailKey,
    workflowRevisionKey,
  ]) {
    queryClient.setQueryData(key, {});
  }

  await invalidateQueriesForDashboardEvent(sessionEvent("session.updated"));

  expect(isInvalidated(currentOverviewKey)).toBe(true);
  expect(isInvalidated(archivedOverviewKey)).toBe(true);
  expect(isInvalidated(affectedDetailKey)).toBe(true);
  expect(isInvalidated(affectedModelsKey)).toBe(true);
  expect(isInvalidated(otherDetailKey)).toBe(false);
  expect(isInvalidated(workflowRevisionKey)).toBe(false);
});

test("connection recovery invalidates all cached dashboard queries", async () => {
  const sessionKey = sessionKeys.detail("session-1");
  const workflowKey = workflowKeys.revision("workflow-1", 1);
  queryClient.setQueryData(sessionKey, {});
  queryClient.setQueryData(workflowKey, {});

  await invalidateQueriesAfterConnectionRecovery();

  expect(isInvalidated(sessionKey)).toBe(true);
  expect(isInvalidated(workflowKey)).toBe(true);
});

test("dashboard query cleanup removes all cached queries", () => {
  queryClient.setQueryData(sessionKeys.detail("session-1"), {});
  queryClient.setQueryData(workflowKeys.revision("workflow-1", 1), {});

  clearDashboardQueries();

  expect(queryClient.getQueryCache().getAll()).toEqual([]);
});
