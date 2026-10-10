import { startEventStream, stopEventStream } from "./eventStream";
import { loadAgentProfiles, resetAgentProfiles } from "../stores/agentProfiles";
import { resetConnectionState } from "../stores/connection";
import { resetSessions } from "../stores/sessions";
import { loadTasks, resetTasks } from "../stores/tasks";
import { resetTimelineState } from "../stores/timeline";
import { loadWorkspaces, resetWorkspaces } from "../stores/workspaces";
import { loadWorkflows, resetWorkflows } from "../stores/workflows";
import { queryClient } from "../lib/queryClient";

export function startDashboardRuntime(): void {
  void Promise.all([
    loadTasks(),
    loadWorkspaces(),
    loadAgentProfiles(),
    loadWorkflows({ showLoading: false }),
  ]);
  startEventStream();
}

export function stopDashboardRuntime(): void {
  stopEventStream();
}

export function clearDashboardRuntimeState(): void {
  queryClient.clear();
  resetConnectionState();
  resetTimelineState();
  resetAgentProfiles();
  resetSessions();
  resetTasks();
  resetWorkspaces();
  resetWorkflows();
}
