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
import { timelineKeys } from "../../src/queries/timeline";
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
  const currentOverviewKey = [
    ...sessionOverviewKeys.all,
    { includeArchived: false, workspaceId: null },
  ] as const;
  const archivedOverviewKey = [
    ...sessionOverviewKeys.all,
    { includeArchived: true, workspaceId: null },
  ] as const;
  const workspaceOverviewKey = [
    ...sessionOverviewKeys.all,
    { includeArchived: false, workspaceId: "workspace-1" },
  ] as const;
  const affectedDetailKey = sessionKeys.detail("session-1");
  const affectedModelsKey = sessionKeys.models("session-1");
  const affectedTimelineKey = timelineKeys.snapshot("session-1");
  const otherDetailKey = sessionKeys.detail("session-2");
  const otherTimelineKey = timelineKeys.snapshot("session-2");
  const workflowRevisionKey = workflowKeys.revision("workflow-1", 1);

  for (const key of [
    currentOverviewKey,
    archivedOverviewKey,
    workspaceOverviewKey,
    affectedDetailKey,
    affectedModelsKey,
    affectedTimelineKey,
    otherDetailKey,
    otherTimelineKey,
    workflowRevisionKey,
  ]) {
    queryClient.setQueryData(key, {});
  }

  await invalidateQueriesForDashboardEvent(sessionEvent("session.updated"));

  expect(isInvalidated(currentOverviewKey)).toBe(true);
  expect(isInvalidated(archivedOverviewKey)).toBe(true);
  expect(isInvalidated(workspaceOverviewKey)).toBe(true);
  expect(isInvalidated(affectedDetailKey)).toBe(true);
  expect(isInvalidated(affectedModelsKey)).toBe(false);
  expect(isInvalidated(affectedTimelineKey)).toBe(true);
  expect(isInvalidated(otherDetailKey)).toBe(false);
  expect(isInvalidated(otherTimelineKey)).toBe(false);
  expect(isInvalidated(workflowRevisionKey)).toBe(false);
});

test("a model event also invalidates the affected Session model query", async () => {
  const modelsKey = sessionKeys.models("session-1");
  const otherModelsKey = sessionKeys.models("session-2");
  queryClient.setQueryData(modelsKey, {});
  queryClient.setQueryData(otherModelsKey, {});

  await invalidateQueriesForDashboardEvent(sessionEvent("session.model_updated"));

  expect(isInvalidated(modelsKey)).toBe(true);
  expect(isInvalidated(otherModelsKey)).toBe(false);
});

test("connection recovery invalidates mutable snapshots without expiring immutable or optimistic data", async () => {
  const sessionKey = sessionKeys.detail("session-1");
  const workflowDetailKey = workflowKeys.detail("workflow-1");
  const workflowRevisionKey = workflowKeys.revision("workflow-1", 1);
  const workflowDocumentKey = workflowKeys.document("workflow-1", "reason.md");
  const optimisticKey = sessionKeys.optimisticInbox();
  for (const key of [
    sessionKey,
    workflowDetailKey,
    workflowRevisionKey,
    workflowDocumentKey,
    optimisticKey,
  ]) {
    queryClient.setQueryData(key, {});
  }

  await invalidateQueriesAfterConnectionRecovery();

  expect(isInvalidated(sessionKey)).toBe(true);
  expect(isInvalidated(workflowDetailKey)).toBe(true);
  expect(isInvalidated(workflowRevisionKey)).toBe(false);
  expect(isInvalidated(workflowDocumentKey)).toBe(false);
  expect(isInvalidated(optimisticKey)).toBe(false);
});

test("dashboard query cleanup removes all cached queries", () => {
  queryClient.setQueryData(sessionKeys.detail("session-1"), {});
  queryClient.setQueryData(workflowKeys.revision("workflow-1", 1), {});

  clearDashboardQueries();

  expect(queryClient.getQueryCache().getAll()).toEqual([]);
});
