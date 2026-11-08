import type { DashboardStreamEvent } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createInvalidation(device: DeviceRuntime) {
  async function invalidateQueriesForDashboardEvent(
    streamEvent: DashboardStreamEvent,
  ): Promise<void> {
    if (streamEvent.kind !== "session_event") return;

    const sessionId = streamEvent.event.session_id;
    const invalidations = [
      device.queryClient.invalidateQueries({
        queryKey: device.sessionOverview.sessionOverviewKeys.all,
      }),
      device.sessionQueries.invalidateSessionDetail(sessionId, { includeModels: false }),
      device.queryClient.invalidateQueries({
        queryKey: device.timelineQueries.timelineKeys.snapshot(sessionId),
        refetchType: "none" as const,
      }),
    ];
    if (streamEvent.event.type === "session.model_updated") {
      invalidations.push(
        device.queryClient.invalidateQueries({
          queryKey: device.sessionQueries.sessionKeys.models(sessionId),
        }),
      );
    }
    await Promise.all(invalidations);
  }

  function isMutableDashboardQuery(queryKey: readonly unknown[]): boolean {
    if (queryKey[1] === device.sessionQueries.sessionKeys.all[1]) {
      if (queryKey[2] === "optimistic-inbox" || queryKey[2] === "optimistic-chat") return false;
      if (queryKey[2] === "timeline") return queryKey[3] === "snapshot";
      return queryKey[2] === "overview" || queryKey[2] === "detail";
    }
    if (queryKey[1] === device.workflowQueries.workflowKeys.all[1]) {
      return (
        queryKey[2] === "list" ||
        (queryKey[2] === "detail" && (queryKey.length === 4 || queryKey[4] === "patches"))
      );
    }
    return (
      queryKey[1] === device.workspaceQueries.workspaceKeys.all[1] ||
      queryKey[1] === device.workspaceQueries.workspaceRootKeys.all[1] ||
      queryKey[1] === device.agentProfileQueries.agentProfileKeys.all[1]
    );
  }

  function invalidateQueriesAfterConnectionRecovery(): Promise<void> {
    return device.queryClient.invalidateQueries({
      predicate: (query) => isMutableDashboardQuery(query.queryKey),
    });
  }

  function clearDashboardQueries(): void {
    device.queryClient.clear();
  }
  return {
    invalidateQueriesForDashboardEvent,
    invalidateQueriesAfterConnectionRecovery,
    clearDashboardQueries,
  };
}
