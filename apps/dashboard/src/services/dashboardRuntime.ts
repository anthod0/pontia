import { startEventStream, stopEventStream } from "./eventStream";
import { loadAgentProfiles } from "../stores/agentProfiles";
import { loadSessions } from "../stores/sessions";
import { loadTasks } from "../stores/tasks";
import { loadWorkspaces } from "../stores/workspaces";
import { loadWorkflows } from "../stores/workflows";

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
