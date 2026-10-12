import type { DashboardStreamEvent } from "../api/types";
import { agentProfileKeys } from "./agentProfiles";
import { queryClient } from "./queryClient";
import { sessionOverviewKeys } from "./sessionOverview";
import { sessionKeys } from "./sessions";
import { timelineKeys } from "./timeline";
import { workflowKeys } from "./workflows";
import { workspaceKeys, workspaceRootKeys } from "./workspaces";

export async function invalidateQueriesForDashboardEvent(
  streamEvent: DashboardStreamEvent,
): Promise<void> {
  if (streamEvent.kind !== "session_event") return;

  const sessionId = streamEvent.event.session_id;
  const invalidations = [
    queryClient.invalidateQueries({ queryKey: sessionOverviewKeys.all }),
    queryClient.invalidateQueries({ queryKey: sessionKeys.detail(sessionId), exact: true }),
    queryClient.invalidateQueries({ queryKey: sessionKeys.turns(sessionId) }),
    queryClient.invalidateQueries({ queryKey: sessionKeys.inboxMessages(sessionId) }),
    queryClient.invalidateQueries({ queryKey: sessionKeys.events(sessionId) }),
    queryClient.invalidateQueries({
      queryKey: timelineKeys.snapshot(sessionId),
      refetchType: "none" as const,
    }),
  ];
  if (streamEvent.event.type === "session.model_updated") {
    invalidations.push(queryClient.invalidateQueries({ queryKey: sessionKeys.models(sessionId) }));
  }
  await Promise.all(invalidations);
}

function isMutableDashboardQuery(queryKey: readonly unknown[]): boolean {
  if (queryKey[0] === sessionKeys.all[0]) {
    if (queryKey[1] === "optimistic-inbox" || queryKey[1] === "optimistic-chat") return false;
    if (queryKey[1] === "timeline") return queryKey[2] === "snapshot";
    return queryKey[1] === "overview" || queryKey[1] === "detail";
  }
  if (queryKey[0] === workflowKeys.all[0]) {
    return (
      queryKey[1] === "list" ||
      (queryKey[1] === "detail" && (queryKey.length === 3 || queryKey[3] === "patches"))
    );
  }
  return (
    queryKey[0] === workspaceKeys.all[0] ||
    queryKey[0] === workspaceRootKeys.all[0] ||
    queryKey[0] === agentProfileKeys.all[0]
  );
}

export function invalidateQueriesAfterConnectionRecovery(): Promise<void> {
  return queryClient.invalidateQueries({
    predicate: (query) => isMutableDashboardQuery(query.queryKey),
  });
}

export function clearDashboardQueries(): void {
  queryClient.clear();
}
