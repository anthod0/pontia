import { get } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";

export type DashboardSnapshotRefreshReason = "sse_open" | "sse_fallback";

export type DashboardSnapshotRefreshOptions = {
  reason: DashboardSnapshotRefreshReason;
};
export function createSnapshotRefresh(device: DeviceRuntime) {
  let refreshInFlight: Promise<void> | null = null;

  let refreshPending = false;

  function refreshDashboardSnapshot(_options: DashboardSnapshotRefreshOptions): Promise<void> {
    refreshPending = true;
    if (refreshInFlight) return refreshInFlight;

    refreshInFlight = (async () => {
      do {
        refreshPending = false;
        await refreshDashboardSnapshotNow();
      } while (refreshPending);
    })().finally(() => {
      refreshInFlight = null;
    });
    return refreshInFlight;
  }

  async function refreshSelectedSession(sessionId: string): Promise<void> {
    device.timeline.resetTimelineState(sessionId);
    const detail = await device.sessions.loadSessionDetail(sessionId, { showLoading: false });
    if (
      get(device.sessions.selectedSessionId) !== sessionId ||
      !detail?.session.capabilities.timeline
    )
      return;
    await device.timeline.loadSessionTimeline(sessionId, {
      mode: "rebuild",
      topology: detail.session.capabilities.topology === true,
    });
  }

  async function refreshDashboardSnapshotNow(): Promise<void> {
    const sessionId = get(device.sessions.selectedSessionId);
    const workflowId = get(device.workflows.selectedWorkflowId);
    const refreshes: Promise<unknown>[] = [
      device.invalidation.invalidateQueriesAfterConnectionRecovery(),
      device.workflowQueries.fetchWorkflows(),
    ];

    if (workflowId) refreshes.push(device.workflowQueries.fetchWorkflow(workflowId));
    if (sessionId) refreshes.push(refreshSelectedSession(sessionId));

    await Promise.allSettled(refreshes);
  }
  return { refreshDashboardSnapshot };
}
