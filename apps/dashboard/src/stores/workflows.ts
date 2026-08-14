import { get, writable } from 'svelte/store';
import { getWorkflow, listWorkflows } from '../api/client';
import type { WorkflowDetailView, WorkflowListItemView } from '../api/types';

type LoadOptions = { showLoading?: boolean };

export const workflows = writable<WorkflowListItemView[]>([]);
export const workflowsLoading = writable(false);
export const workflowsError = writable<string | null>(null);
export const workflowDetail = writable<WorkflowDetailView | null>(null);
export const workflowDetailLoading = writable(false);
export const workflowDetailError = writable<string | null>(null);
export const selectedWorkflowId = writable<string | null>(null);

export async function loadWorkflows(options: LoadOptions = {}): Promise<WorkflowListItemView[]> {
  const showLoading = options.showLoading ?? true;
  if (showLoading) workflowsLoading.set(true);
  workflowsError.set(null);
  try {
    const loaded = await listWorkflows();
    workflows.set(loaded);
    return loaded;
  } catch (error) {
    workflowsError.set(error instanceof Error ? error.message : String(error));
    if (showLoading) workflows.set([]);
    return [];
  } finally {
    if (showLoading) workflowsLoading.set(false);
  }
}

export async function refreshWorkflow(workflowId: string, options: LoadOptions = {}): Promise<WorkflowDetailView | null> {
  if (!workflowId) return null;
  const showLoading = options.showLoading ?? true;
  if (showLoading) workflowDetailLoading.set(true);
  workflowDetailError.set(null);
  try {
    const loaded = await getWorkflow(workflowId);
    workflowDetail.set(loaded);
    workflows.update((items) => items.map((item) => item.workflow_id === loaded.workflow_id ? {
      ...item,
      title: loaded.title,
      state: loaded.state,
      failure_message: loaded.failure_message,
      agent_submitted_count: loaded.agent_submitted_count,
      agent_total_count: loaded.agent_total_count,
      started_at: loaded.started_at,
      completed_at: loaded.completed_at,
      updated_at: loaded.updated_at,
      elapsed_ms: loaded.elapsed_ms,
    } : item));
    return loaded;
  } catch (error) {
    workflowDetailError.set(error instanceof Error ? error.message : String(error));
    if (showLoading) workflowDetail.set(null);
    return null;
  } finally {
    if (showLoading) workflowDetailLoading.set(false);
  }
}

export function selectedWorkflowSessionIds(): string[] {
  const selectedId = get(selectedWorkflowId);
  const detail = get(workflowDetail);
  if (!selectedId || detail?.workflow_id !== selectedId) return [];
  return detail.nodes.flatMap((node) => node.session_id ? [node.session_id] : []);
}
