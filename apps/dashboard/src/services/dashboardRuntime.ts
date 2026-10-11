import { startEventStream, stopEventStream } from "./eventStream";
import { resetConnectionState } from "../stores/connection";
import { resetSessions } from "../stores/sessions";
import { resetTimelineState } from "../stores/timeline";
import { fetchWorkspaces } from "../queries/workspaces";
import { resetWorkflows } from "../stores/workflows";
import { fetchWorkflows } from "../queries/workflows";
import { clearDashboardQueries } from "../queries/dashboardInvalidation";

export function startDashboardRuntime(): void {
  void Promise.all([fetchWorkspaces(), fetchWorkflows()]);
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
