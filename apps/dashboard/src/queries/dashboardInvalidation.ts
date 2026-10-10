import type { DashboardStreamEvent } from "../api/types";
import { queryClient } from "./queryClient";
import { sessionOverviewKeys } from "./sessionOverview";
import { sessionKeys } from "./sessions";

export async function invalidateQueriesForDashboardEvent(
  streamEvent: DashboardStreamEvent,
): Promise<void> {
  if (streamEvent.kind !== "session_event") return;

  await Promise.all([
    queryClient.invalidateQueries({ queryKey: sessionOverviewKeys.all }),
    queryClient.invalidateQueries({ queryKey: sessionKeys.detail(streamEvent.event.session_id) }),
  ]);
}

export function invalidateQueriesAfterConnectionRecovery(): Promise<void> {
  return queryClient.invalidateQueries();
}

export function clearDashboardQueries(): void {
  queryClient.clear();
}
