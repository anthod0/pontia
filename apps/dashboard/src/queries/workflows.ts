import { createMutation, createQuery, queryOptions } from "@tanstack/svelte-query";
import {
  getWorkflow,
  getWorkflowDocument,
  getWorkflowRevision,
  listWorkflowPatches,
  listWorkflows,
  pauseWorkflow as requestPauseWorkflow,
  resumeWorkflow as requestResumeWorkflow,
  retryWorkflow as requestRetryWorkflow,
} from "../api/client";
import type { WorkflowDetailView, WorkflowListItemView } from "../api/types";
import { queryClient } from "./queryClient";

export const workflowKeys = {
  all: ["workflows"] as const,
  lists: () => [...workflowKeys.all, "list"] as const,
  list: (limit: number) => [...workflowKeys.lists(), { limit }] as const,
  details: () => [...workflowKeys.all, "detail"] as const,
  detail: (workflowId: string) => [...workflowKeys.details(), workflowId] as const,
  revision: (workflowId: string, revision: number | null) =>
    [...workflowKeys.detail(workflowId), "revisions", revision] as const,
  patches: (workflowId: string) => [...workflowKeys.detail(workflowId), "patches"] as const,
  document: (workflowId: string, ref: string | null) =>
    [...workflowKeys.detail(workflowId), "documents", { ref }] as const,
};

function workflowsOptions(limit: number) {
  return queryOptions({
    queryKey: workflowKeys.list(limit),
    queryFn: ({ signal }) => listWorkflows(limit, { signal }),
  });
}

function workflowOptions(workflowId: string, enabled = true, poll = false) {
  return queryOptions({
    queryKey: workflowKeys.detail(workflowId),
    enabled: enabled && workflowId.length > 0,
    queryFn: ({ signal }) => getWorkflow(workflowId, { signal }),
    structuralSharing: false,
    refetchInterval: poll ? 2_000 : false,
    refetchIntervalInBackground: false,
  });
}

function workflowRevisionOptions(workflowId: string, revision: number | null) {
  return queryOptions({
    queryKey: workflowKeys.revision(workflowId, revision),
    enabled: revision !== null,
    queryFn: ({ signal }) => {
      if (revision === null) throw new Error("A workflow revision is required");
      return getWorkflowRevision(workflowId, revision, { signal });
    },
    staleTime: Number.POSITIVE_INFINITY,
  });
}

function workflowPatchesOptions(workflowId: string, enabled: boolean) {
  return queryOptions({
    queryKey: workflowKeys.patches(workflowId),
    enabled,
    queryFn: ({ signal }) => listWorkflowPatches(workflowId, { signal }),
    select: (patches) =>
      [...patches].sort(
        (a, b) =>
          Date.parse(b.requested_at) - Date.parse(a.requested_at) ||
          b.patch_id.localeCompare(a.patch_id),
      ),
  });
}

function workflowDocumentOptions(workflowId: string, ref: string | null, enabled: boolean) {
  return queryOptions({
    queryKey: workflowKeys.document(workflowId, ref),
    enabled: enabled && ref !== null,
    queryFn: ({ signal }) => {
      if (ref === null) throw new Error("A workflow document reference is required");
      return getWorkflowDocument(workflowId, ref, { signal });
    },
  });
}

export function createWorkflowsQuery(limit: () => number = () => 50) {
  return createQuery(
    () => workflowsOptions(limit()),
    () => queryClient,
  );
}

export function createWorkflowQuery(
  workflowId: () => string,
  enabled: () => boolean = () => true,
  poll: () => boolean = () => false,
) {
  return createQuery(
    () => workflowOptions(workflowId(), enabled(), poll()),
    () => queryClient,
  );
}

export function createWorkflowRevisionQuery(
  workflowId: () => string,
  revision: () => number | null,
) {
  return createQuery(
    () => workflowRevisionOptions(workflowId(), revision()),
    () => queryClient,
  );
}

export function createWorkflowPatchesQuery(
  workflowId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => workflowPatchesOptions(workflowId(), enabled()),
    () => queryClient,
  );
}

export function createWorkflowDocumentQuery(
  workflowId: () => string,
  ref: () => string | null,
  enabled: () => boolean,
) {
  return createQuery(
    () => workflowDocumentOptions(workflowId(), ref(), enabled()),
    () => queryClient,
  );
}

export function fetchWorkflows(limit = 50) {
  return queryClient.fetchQuery(workflowsOptions(limit));
}

export function fetchWorkflow(workflowId: string) {
  return queryClient.fetchQuery(workflowOptions(workflowId));
}

export function invalidateWorkflowLists(): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: workflowKeys.lists() });
}

export function invalidateWorkflow(workflowId: string): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: workflowKeys.detail(workflowId), exact: true });
}

function workflowListItem(detail: WorkflowDetailView, current: WorkflowListItemView) {
  return {
    ...current,
    title: detail.title,
    state: detail.state,
    current_revision: detail.current_revision,
    failure_message: detail.failure_message,
    agent_submitted_count: detail.agent_submitted_count,
    agent_total_count: detail.agent_total_count,
    started_at: detail.started_at,
    completed_at: detail.completed_at,
    updated_at: detail.updated_at,
    elapsed_ms: detail.elapsed_ms,
  };
}

function applyWorkflowDetail(detail: WorkflowDetailView): void {
  queryClient.setQueryData(workflowKeys.detail(detail.workflow_id), detail);
  queryClient.setQueriesData<WorkflowListItemView[]>({ queryKey: workflowKeys.lists() }, (items) =>
    items?.map((item) =>
      item.workflow_id === detail.workflow_id ? workflowListItem(detail, item) : item,
    ),
  );
}

export function createPauseWorkflowMutation() {
  return createMutation(
    () => ({
      mutationFn: requestPauseWorkflow,
      onSuccess: applyWorkflowDetail,
    }),
    () => queryClient,
  );
}

export function createResumeWorkflowMutation() {
  return createMutation(
    () => ({
      mutationFn: requestResumeWorkflow,
      onSuccess: applyWorkflowDetail,
    }),
    () => queryClient,
  );
}

export function createRetryWorkflowMutation() {
  return createMutation(
    () => ({
      mutationFn: ({
        workflowId,
        failureEventId,
      }: {
        workflowId: string;
        failureEventId: string;
      }) => requestRetryWorkflow(workflowId, failureEventId),
      onSuccess: applyWorkflowDetail,
    }),
    () => queryClient,
  );
}
