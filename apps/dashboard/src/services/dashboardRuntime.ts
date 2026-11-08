import type { DeviceRuntime } from "../device/runtime";

export function createLifecycle(device: DeviceRuntime) {
  function startDashboardRuntime(): void {
    void Promise.all([
      device.workspaceQueries.fetchWorkspaces(),
      device.workflowQueries.fetchWorkflows(),
    ]);
    device.events.startEventStream();
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
