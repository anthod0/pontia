import type { DeviceRuntime } from "../device/runtime";

export function createLifecycle(device: DeviceRuntime) {
  function startDashboardRuntime(onDisconnected?: () => void): void {
    void Promise.allSettled([
      device.workspaceQueries.fetchWorkspaces(),
      device.workflowQueries.fetchWorkflows(),
    ]);
    device.events.startEventStream(onDisconnected);
  }

  function stopDashboardRuntime(): void {
    device.events.stopEventStream();
  }

  function clearDashboardRuntimeState(): void {
    device.invalidation.clearDashboardQueries();
    device.connection.resetConnectionState();
    device.timeline.resetTimelineState();
    device.sessions.resetSessions();
    device.workflows.resetWorkflows();
  }
  return { startDashboardRuntime, stopDashboardRuntime, clearDashboardRuntimeState };
}
