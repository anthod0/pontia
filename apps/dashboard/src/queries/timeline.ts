import { queryOptions } from "@tanstack/svelte-query";
import { getTurnTimeline, getTurnTreeHistory, getTurnTreeUpdates } from "../api/client";
import type { TurnTimelineDirection } from "../api/types";
import { queryClient } from "./queryClient";

export const timelineKeys = {
  all: ["sessions", "timeline"] as const,
  snapshots: () => [...timelineKeys.all, "snapshot"] as const,
  snapshot: (sessionId: string) => [...timelineKeys.snapshots(), sessionId] as const,
  requests: () => [...timelineKeys.all, "request"] as const,
  page: (
    sessionId: string,
    direction: TurnTimelineDirection,
    turnId: string | null,
    limit: number,
  ) => [...timelineKeys.requests(), "linear", sessionId, { direction, turnId, limit }] as const,
  treeHistory: (sessionId: string, fromTurnId: string | null, limit: number) =>
    [...timelineKeys.requests(), "tree-history", sessionId, { fromTurnId, limit }] as const,
  treeUpdates: (sessionId: string, fromTurnId: string | null) =>
    [...timelineKeys.requests(), "tree-updates", sessionId, { fromTurnId }] as const,
};

export function turnTimelineOptions(
  sessionId: string,
  direction: TurnTimelineDirection,
  turnId: string | null,
  limit: number,
) {
  return queryOptions({
    queryKey: timelineKeys.page(sessionId, direction, turnId, limit),
    queryFn: ({ signal }) =>
      getTurnTimeline(sessionId, {
        direction,
        ...(turnId ? { turnId } : {}),
        limit,
        signal,
      }),
    staleTime: 0,
    gcTime: 0,
  });
}

export function turnTreeHistoryOptions(
  sessionId: string,
  fromTurnId: string | null,
  limit: number,
) {
  return queryOptions({
    queryKey: timelineKeys.treeHistory(sessionId, fromTurnId, limit),
    queryFn: ({ signal }) =>
      getTurnTreeHistory(sessionId, {
        ...(fromTurnId ? { fromTurnId } : {}),
        limit,
        signal,
      }),
    staleTime: 0,
    gcTime: 0,
  });
}

export function turnTreeUpdatesOptions(sessionId: string, fromTurnId: string | null) {
  return queryOptions({
    queryKey: timelineKeys.treeUpdates(sessionId, fromTurnId),
    queryFn: ({ signal }) =>
      getTurnTreeUpdates(sessionId, {
        ...(fromTurnId ? { fromTurnId } : {}),
        signal,
      }),
    staleTime: 0,
    gcTime: 0,
  });
}

export function fetchTurnTimeline(
  sessionId: string,
  direction: TurnTimelineDirection,
  turnId: string | null,
  limit: number,
) {
  return queryClient.fetchQuery(turnTimelineOptions(sessionId, direction, turnId, limit));
}

export function fetchTurnTreeHistory(sessionId: string, fromTurnId: string | null, limit: number) {
  return queryClient.fetchQuery(turnTreeHistoryOptions(sessionId, fromTurnId, limit));
}

export function fetchTurnTreeUpdates(sessionId: string, fromTurnId: string | null) {
  return queryClient.fetchQuery(turnTreeUpdatesOptions(sessionId, fromTurnId));
}

export function cancelTimelineRequests(sessionId: string): void {
  const matchesSession = (query: { queryKey: readonly unknown[] }) =>
    query.queryKey[0] === "sessions" &&
    query.queryKey[1] === "timeline" &&
    query.queryKey[2] === "request" &&
    query.queryKey[4] === sessionId;
  void queryClient.cancelQueries({ predicate: matchesSession });
  queryClient.removeQueries({ predicate: matchesSession });
}
