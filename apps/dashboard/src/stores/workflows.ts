import { get, writable } from "svelte/store";

import type { WorkflowDetailView } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createWorkflows(device: DeviceRuntime) {
  const selectedWorkflowId = writable<string | null>(null);

  const selectedWorkflowHistorySessionIds = writable<string[]>([]);

  function resetWorkflows(): void {
    selectedWorkflowId.set(null);
    selectedWorkflowHistorySessionIds.set([]);
  }

  function selectedWorkflowSessionIds(): string[] {
    const selectedId = get(selectedWorkflowId);
    if (!selectedId) return [];
    const detail = device.queryClient.getQueryData<WorkflowDetailView>(
      device.workflowQueries.workflowKeys.detail(selectedId),
    );
    if (!detail) return [];
    return [
      ...new Set(
        [
          ...detail.nodes.flatMap((node) => (node.session_id ? [node.session_id] : [])),
          detail.active_patch?.replanner_session_id,
          ...get(selectedWorkflowHistorySessionIds),
        ].filter((id): id is string => !!id),
      ),
    ];
  }
  return {
    selectedWorkflowId,
    selectedWorkflowHistorySessionIds,
    resetWorkflows,
    selectedWorkflowSessionIds,
  };
}
