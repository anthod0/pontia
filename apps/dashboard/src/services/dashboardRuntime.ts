import { startEventStream, stopEventStream } from "./eventStream";
import { resetConnectionState } from "../stores/connection";
import { resetSessions } from "../stores/sessions";
import { resetTimelineState } from "../stores/timeline";
import { fetchWorkspaces } from "../queries/workspaces";
import { loadWorkflows, resetWorkflows } from "../stores/workflows";
import { clearDashboardQueries } from "../queries/dashboardInvalidation";

export function startDashboardRuntime(): void {
  void Promise.all([fetchWorkspaces(), loadWorkflows({ showLoading: false })]);
  startEventStream();
}

export function stopDashboardRuntime(): void {
  stopEventStream();
}

export function clearDashboardRuntimeState(): void {
  clearDashboardQueries();
  resetConnectionState();
  resetTimelineState();
  resetSessions();
  resetWorkflows();
}
