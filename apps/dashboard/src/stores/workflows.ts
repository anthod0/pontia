import { get, writable } from "svelte/store";
import type { WorkflowDetailView } from "../api/types";
import { queryClient } from "../queries/queryClient";
import { workflowKeys } from "../queries/workflows";

export const selectedWorkflowId = writable<string | null>(null);
export const selectedWorkflowHistorySessionIds = writable<string[]>([]);

export function resetWorkflows(): void {
  selectedWorkflowId.set(null);
  selectedWorkflowHistorySessionIds.set([]);
}

export function selectedWorkflowSessionIds(): string[] {
  const selectedId = get(selectedWorkflowId);
  if (!selectedId) return [];
  const detail = queryClient.getQueryData<WorkflowDetailView>(workflowKeys.detail(selectedId));
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
