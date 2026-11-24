import { get, writable } from "svelte/store";

import type { WorkflowDetailView } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createWorkflows(device: DeviceRuntime) {
  const selectedWorkflowId = writable<string | null>(null);

  function resetWorkflows(): void {
    selectedWorkflowId.set(null);
  }

  function selectedWorkflowSessionIds(): string[] {
    const selectedId = get(selectedWorkflowId);
    if (!selectedId) return [];
    const detail = device.queryClient.getQueryData<WorkflowDetailView>(
      device.workflowQueries.workflowKeys.detail(selectedId),
    );
    if (!detail) return [];
    return detail.nodes.flatMap((node) => (node.session_id ? [node.session_id] : []));
  }
  return {
    selectedWorkflowId,
    resetWorkflows,
    selectedWorkflowSessionIds,
  };
}
