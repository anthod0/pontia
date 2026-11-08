import { createMutation, createQuery, queryOptions } from "@tanstack/svelte-query";

import type { WorkflowDetailView, WorkflowListItemView } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createWorkflowQueries(device: DeviceRuntime) {
  const workflowKeys = {
    all: [device.id, "workflows"] as const,
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
      queryFn: ({ signal }) => device.api.listWorkflows(limit, { signal }),
    });
  }

  function workflowOptions(workflowId: string, enabled = true, poll = false) {
    return queryOptions({
      queryKey: workflowKeys.detail(workflowId),
      enabled: enabled && workflowId.length > 0,
      queryFn: ({ signal }) => device.api.getWorkflow(workflowId, { signal }),
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
        return device.api.getWorkflowRevision(workflowId, revision, { signal });
      },
      staleTime: Number.POSITIVE_INFINITY,
    });
  }

  function workflowPatchesOptions(workflowId: string, enabled: boolean) {
    return queryOptions({
      queryKey: workflowKeys.patches(workflowId),
      enabled,
      queryFn: ({ signal }) => device.api.listWorkflowPatches(workflowId, { signal }),
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
        return device.api.getWorkflowDocument(workflowId, ref, { signal });
      },
    });
  }

  function createWorkflowsQuery(limit: () => number = () => 50) {
    return createQuery(
      () => workflowsOptions(limit()),
      () => device.queryClient,
    );
  }

  function createWorkflowQuery(
    workflowId: () => string,
    enabled: () => boolean = () => true,
    poll: () => boolean = () => false,
  ) {
    return createQuery(
      () => workflowOptions(workflowId(), enabled(), poll()),
      () => device.queryClient,
    );
  }

  function createWorkflowRevisionQuery(workflowId: () => string, revision: () => number | null) {
    return createQuery(
      () => workflowRevisionOptions(workflowId(), revision()),
      () => device.queryClient,
    );
  }

  function createWorkflowPatchesQuery(
    workflowId: () => string,
    enabled: () => boolean = () => true,
  ) {
    return createQuery(
      () => workflowPatchesOptions(workflowId(), enabled()),
      () => device.queryClient,
    );
  }

  function createWorkflowDocumentQuery(
    workflowId: () => string,
    ref: () => string | null,
    enabled: () => boolean,
  ) {
    return createQuery(
      () => workflowDocumentOptions(workflowId(), ref(), enabled()),
      () => device.queryClient,
    );
  }

  function fetchWorkflows(limit = 50) {
    return device.queryClient.fetchQuery(workflowsOptions(limit));
  }

  function fetchWorkflow(workflowId: string) {
    return device.queryClient.fetchQuery(workflowOptions(workflowId));
  }

  function invalidateWorkflowLists(): Promise<void> {
    return device.queryClient.invalidateQueries({ queryKey: workflowKeys.lists() });
  }

  function invalidateWorkflow(workflowId: string): Promise<void> {
    return device.queryClient.invalidateQueries({
      queryKey: workflowKeys.detail(workflowId),
      exact: true,
    });
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
    device.queryClient.setQueryData(workflowKeys.detail(detail.workflow_id), detail);
    device.queryClient.setQueriesData<WorkflowListItemView[]>(
      { queryKey: workflowKeys.lists() },
      (items) =>
        items?.map((item) =>
          item.workflow_id === detail.workflow_id ? workflowListItem(detail, item) : item,
        ),
    );
  }

  function createPauseWorkflowMutation() {
    return createMutation(
      () => ({
        mutationFn: device.api.pauseWorkflow,
        onSuccess: applyWorkflowDetail,
      }),
      () => device.queryClient,
    );
  }

  function createResumeWorkflowMutation() {
    return createMutation(
      () => ({
        mutationFn: device.api.resumeWorkflow,
        onSuccess: applyWorkflowDetail,
      }),
      () => device.queryClient,
    );
  }

  function createRetryWorkflowMutation() {
    return createMutation(
      () => ({
        mutationFn: ({
          workflowId,
          failureEventId,
        }: {
          workflowId: string;
          failureEventId: string;
        }) => device.api.retryWorkflow(workflowId, failureEventId),
        onSuccess: applyWorkflowDetail,
      }),
      () => device.queryClient,
    );
  }
  return {
    workflowKeys,
    createWorkflowsQuery,
    createWorkflowQuery,
    createWorkflowRevisionQuery,
    createWorkflowPatchesQuery,
    createWorkflowDocumentQuery,
    fetchWorkflows,
    fetchWorkflow,
    invalidateWorkflowLists,
    invalidateWorkflow,
    createPauseWorkflowMutation,
    createResumeWorkflowMutation,
    createRetryWorkflowMutation,
  };
}
