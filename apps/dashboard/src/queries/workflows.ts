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

  return {
    workflowKeys,
    createWorkflowsQuery,
    createWorkflowQuery,
    fetchWorkflows,
    fetchWorkflow,
    invalidateWorkflowLists,
    invalidateWorkflow,
    createPauseWorkflowMutation,
    createResumeWorkflowMutation,
  };
}
