import { startEventStream, stopEventStream } from "./eventStream";
import { loadAgentProfiles, resetAgentProfiles } from "../stores/agentProfiles";
import { resetConnectionState } from "../stores/connection";
import { loadSessions, resetSessions } from "../stores/sessions";
import { loadTasks, resetTasks } from "../stores/tasks";
import { resetTimelineState } from "../stores/timeline";
import { loadWorkspaces, resetWorkspaces } from "../stores/workspaces";
import { loadWorkflows, resetWorkflows } from "../stores/workflows";

export function startDashboardRuntime(): void {
  void Promise.all([
    loadTasks(),
    loadWorkspaces(),
    loadAgentProfiles(),
    loadSessions(),
    loadWorkflows({ showLoading: false }),
  ]);
  startEventStream();
}

export function stopDashboardRuntime(): void {
  stopEventStream();
}

export function clearDashboardRuntimeState(): void {
  resetConnectionState();
  resetTimelineState();
  resetAgentProfiles();
  resetSessions();
  resetTasks();
  resetWorkspaces();
  resetWorkflows();
}
