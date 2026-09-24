import { get, writable } from 'svelte/store';
import { getWorkflow, listWorkflows, pauseWorkflow as apiPauseWorkflow, resumeWorkflow as apiResumeWorkflow } from '../api/client';
import type { WorkflowDetailView, WorkflowListItemView } from '../api/types';

type LoadOptions = { showLoading?: boolean };

export const workflows = writable<WorkflowListItemView[]>([]);
export const workflowsLoading = writable(false);
export const workflowsError = writable<string | null>(null);
export const workflowDetail = writable<WorkflowDetailView | null>(null);
export const workflowDetailLoading = writable(false);
export const workflowDetailError = writable<string | null>(null);
export const selectedWorkflowId = writable<string | null>(null);
export const selectedWorkflowHistorySessionIds = writable<string[]>([]);

let listRequest = 0;

export async function loadWorkflows(options: LoadOptions = {}): Promise<WorkflowListItemView[]> {
  const request = ++listRequest;
  const showLoading = options.showLoading ?? true;
  if (showLoading) workflowsLoading.set(true);
  workflowsError.set(null);
  try {
    const loaded = await listWorkflows();
    if (request === listRequest) workflows.set(loaded);
    return loaded;
  } catch (error) {
    if (request === listRequest) workflowsError.set(error instanceof Error ? error.message : String(error));
    return [];
  } finally {
    if (request === listRequest) workflowsLoading.set(false);
  }
}

let detailRequest = 0;
let appliedDetailRequest = 0;

export async function refreshWorkflow(workflowId: string, options: LoadOptions = {}): Promise<WorkflowDetailView | null> {
  if (!workflowId) return null;
  const request = ++detailRequest;
  const isCurrent = () => request >= appliedDetailRequest && (!get(selectedWorkflowId) || get(selectedWorkflowId) === workflowId);
  const showLoading = options.showLoading ?? true;
  if (showLoading) workflowDetailLoading.set(true);
  workflowDetailError.set(null);
  try {
    const loaded = await getWorkflow(workflowId);
    if (!isCurrent()) return null;
    appliedDetailRequest = request;
    applyWorkflowDetail(loaded);
    return loaded;
  } catch (error) {
    if (!isCurrent()) return null;
    appliedDetailRequest = request;
    workflowDetailError.set(error instanceof Error ? error.message : String(error));
    if (showLoading) workflowDetail.set(null);
    return null;
  } finally {
    if (isCurrent()) workflowDetailLoading.set(false);
  }
}

export async function pauseWorkflow(workflowId: string): Promise<WorkflowDetailView> {
  workflowDetailError.set(null);
  try {
    const loaded = await apiPauseWorkflow(workflowId);
    applyWorkflowDetail(loaded);
    return loaded;
  } catch (error) {
    workflowDetailError.set(error instanceof Error ? error.message : String(error));
    throw error;
  }
}

export async function resumeWorkflow(workflowId: string): Promise<WorkflowDetailView> {
  workflowDetailError.set(null);
  try {
    const loaded = await apiResumeWorkflow(workflowId);
    applyWorkflowDetail(loaded);
    return loaded;
  } catch (error) {
    workflowDetailError.set(error instanceof Error ? error.message : String(error));
    throw error;
  }
}

function applyWorkflowDetail(loaded: WorkflowDetailView): void {
  workflowDetail.set(loaded);
  workflows.update((items) => items.map((item) => item.workflow_id === loaded.workflow_id ? {
    ...item,
    title: loaded.title,
    state: loaded.state,
    current_revision: loaded.current_revision,
    failure_message: loaded.failure_message,
    agent_submitted_count: loaded.agent_submitted_count,
    agent_total_count: loaded.agent_total_count,
    started_at: loaded.started_at,
    completed_at: loaded.completed_at,
    updated_at: loaded.updated_at,
    elapsed_ms: loaded.elapsed_ms,
  } : item));
}

export function selectedWorkflowSessionIds(): string[] {
  const selectedId = get(selectedWorkflowId);
  const detail = get(workflowDetail);
  if (!selectedId || detail?.workflow_id !== selectedId) return [];
  return [...new Set([
    ...detail.nodes.flatMap((node) => node.session_id ? [node.session_id] : []),
    detail.active_patch?.replanner_session_id,
    ...get(selectedWorkflowHistorySessionIds),
  ].filter((id): id is string => !!id))];
}
