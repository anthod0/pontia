import { queryOptions } from "@tanstack/svelte-query";

import type { HistorySource, TurnTimelineDirection } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createTimelineQueries(device: DeviceRuntime) {
  const timelineKeys = {
    all: [device.id, "sessions", "timeline"] as const,
    snapshots: () => [...timelineKeys.all, "snapshot"] as const,
    snapshot: (sessionId: string) => [...timelineKeys.snapshots(), sessionId] as const,
    requests: () => [...timelineKeys.all, "request"] as const,
    page: (
      sessionId: string,
      direction: TurnTimelineDirection,
      turnId: string | null,
      limit: number,
      historySource?: HistorySource | null,
    ) =>
      [
        ...timelineKeys.requests(),
        "linear",
        sessionId,
        { direction, turnId, limit, historySource },
      ] as const,
    treeHistory: (sessionId: string, fromTurnId: string | null, limit: number) =>
      [...timelineKeys.requests(), "tree-history", sessionId, { fromTurnId, limit }] as const,
    treeUpdates: (sessionId: string, fromTurnId: string | null) =>
      [...timelineKeys.requests(), "tree-updates", sessionId, { fromTurnId }] as const,
  };

  function turnTimelineOptions(
    sessionId: string,
    direction: TurnTimelineDirection,
    turnId: string | null,
    limit: number,
    historySource?: HistorySource | null,
  ) {
    return queryOptions({
      queryKey: timelineKeys.page(sessionId, direction, turnId, limit, historySource),
      queryFn: ({ signal }) =>
        device.api.getTurnTimeline(sessionId, {
          direction,
          historySource,
          ...(turnId ? { turnId } : {}),
          limit,
          signal,
        }),
      staleTime: 0,
      gcTime: 0,
    });
  }

  function turnTreeHistoryOptions(sessionId: string, fromTurnId: string | null, limit: number) {
    return queryOptions({
      queryKey: timelineKeys.treeHistory(sessionId, fromTurnId, limit),
      queryFn: ({ signal }) =>
        device.api.getTurnTreeHistory(sessionId, {
          ...(fromTurnId ? { fromTurnId } : {}),
          limit,
          signal,
        }),
      staleTime: 0,
      gcTime: 0,
    });
  }

  function turnTreeUpdatesOptions(sessionId: string, fromTurnId: string | null) {
    return queryOptions({
      queryKey: timelineKeys.treeUpdates(sessionId, fromTurnId),
      queryFn: ({ signal }) =>
        device.api.getTurnTreeUpdates(sessionId, {
          ...(fromTurnId ? { fromTurnId } : {}),
          signal,
        }),
      staleTime: 0,
      gcTime: 0,
    });
  }

  function fetchTurnTimeline(
    sessionId: string,
    direction: TurnTimelineDirection,
    turnId: string | null,
    limit: number,
    historySource?: HistorySource | null,
  ) {
    return device.queryClient.fetchQuery(
      turnTimelineOptions(sessionId, direction, turnId, limit, historySource),
    );
  }

  function fetchTurnTreeHistory(sessionId: string, fromTurnId: string | null, limit: number) {
    return device.queryClient.fetchQuery(turnTreeHistoryOptions(sessionId, fromTurnId, limit));
  }

  function fetchTurnTreeUpdates(sessionId: string, fromTurnId: string | null) {
    return device.queryClient.fetchQuery(turnTreeUpdatesOptions(sessionId, fromTurnId));
  }

  function cancelTimelineRequests(sessionId: string): void {
    const matchesSession = (query: { queryKey: readonly unknown[] }) =>
      query.queryKey[1] === "sessions" &&
      query.queryKey[2] === "timeline" &&
      query.queryKey[3] === "request" &&
      query.queryKey[5] === sessionId;
    void device.queryClient.cancelQueries({ predicate: matchesSession });
    device.queryClient.removeQueries({ predicate: matchesSession });
  }
  return {
    timelineKeys,
    turnTimelineOptions,
    turnTreeHistoryOptions,
    turnTreeUpdatesOptions,
    fetchTurnTimeline,
    fetchTurnTreeHistory,
    fetchTurnTreeUpdates,
    cancelTimelineRequests,
  };
}
