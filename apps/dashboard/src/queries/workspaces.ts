import {
  createMutation,
  createQuery,
  MutationObserver,
  QueryObserver,
  queryOptions,
} from "@tanstack/svelte-query";

import { readable } from "svelte/store";

import type {
  RegisterWorkspaceInput,
  RenameWorkspaceInput,
  WorkspaceGitStatusView,
} from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createWorkspaceQueries(device: DeviceRuntime) {
  const workspaceKeys = {
    all: [device.id, "workspaces"] as const,
    lists: () => [...workspaceKeys.all, "list"] as const,
    list: () => [...workspaceKeys.lists()] as const,
    filePickers: () => [...workspaceKeys.all, "file-picker"] as const,
    filePicker: (workspaceId: string, query: string, limit: number | undefined) =>
      [...workspaceKeys.filePickers(), workspaceId, { query, limit }] as const,
    gitStatuses: () => [...workspaceKeys.all, "git-status"] as const,
    gitStatus: (workspaceId: string) => [...workspaceKeys.gitStatuses(), workspaceId] as const,
  };

  const workspaceRootKeys = {
    all: [device.id, "workspace-roots"] as const,
    entries: (rootId: string, path: string) =>
      [...workspaceRootKeys.all, rootId, "entries", { path }] as const,
  };

  function workspacesOptions() {
    return queryOptions({
      queryKey: workspaceKeys.list(),
      queryFn: ({ signal }) => device.api.listWorkspaces({ signal }),
    });
  }

  function workspaceFilePickerOptions(
    workspaceId: string,
    query: string,
    limit: number | undefined,
    requestSignal?: AbortSignal,
  ) {
    return queryOptions({
      queryKey: workspaceKeys.filePicker(workspaceId, query, limit),
      queryFn: ({ signal }) =>
        device.api.listWorkspaceFilePickerEntries(workspaceId, query, {
          limit,
          signal: requestSignal ? AbortSignal.any([signal, requestSignal]) : signal,
        }),
    });
  }

  function workspaceRootsOptions() {
    return queryOptions({
      queryKey: workspaceRootKeys.all,
      queryFn: ({ signal }) => device.api.listWorkspaceRoots({ signal }),
    });
  }

  function workspaceRootEntriesOptions(rootId: string, path: string, enabled = true) {
    return queryOptions({
      queryKey: workspaceRootKeys.entries(rootId, path),
      enabled: enabled && rootId.length > 0,
      queryFn: ({ signal }) => device.api.listWorkspaceRootEntries(rootId, path, { signal }),
    });
  }

  function createWorkspacesQuery() {
    return createQuery(workspacesOptions, () => device.queryClient);
  }

  function createWorkspacesStore() {
    const observer = new QueryObserver(device.queryClient, workspacesOptions());
    return readable(observer.getCurrentResult(), (set) => {
      set(observer.getCurrentResult());
      return observer.subscribe(set);
    });
  }

  function createWorkspaceRootsQuery() {
    return createQuery(workspaceRootsOptions, () => device.queryClient);
  }

  function workspaceGitStatuses(): Record<string, WorkspaceGitStatusView> {
    return Object.fromEntries(
      device.queryClient
        .getQueryCache()
        .findAll({ queryKey: workspaceKeys.gitStatuses() })
        .flatMap((query) => {
          const workspaceId = query.queryKey[3];
          const status = query.state.data;
          return typeof workspaceId === "string" && status
            ? [[workspaceId, status as WorkspaceGitStatusView]]
            : [];
        }),
    );
  }

  function createWorkspaceGitStatusesStore() {
    return readable(workspaceGitStatuses(), (set) => {
      set(workspaceGitStatuses());
      return device.queryClient.getQueryCache().subscribe((event) => {
        if (
          event.query.queryKey[1] === workspaceKeys.all[1] &&
          event.query.queryKey[2] === "git-status"
        ) {
          set(workspaceGitStatuses());
        }
      });
    });
  }

  function createWorkspaceRootEntriesQuery(
    rootId: () => string,
    path: () => string,
    enabled: () => boolean = () => true,
  ) {
    return createQuery(
      () => workspaceRootEntriesOptions(rootId(), path(), enabled()),
      () => device.queryClient,
    );
  }

  function fetchWorkspaces() {
    return device.queryClient.fetchQuery(workspacesOptions());
  }

  function fetchWorkspaceFilePickerEntries(
    workspaceId: string,
    query = "",
    options: { limit?: number; signal?: AbortSignal } = {},
  ) {
    return device.queryClient.fetchQuery(
      workspaceFilePickerOptions(workspaceId, query, options.limit, options.signal),
    );
  }

  function invalidateWorkspaceQueries(): Promise<void> {
    return device.queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
  }

  async function invalidateWorkspaceCollections(): Promise<void[]> {
    return Promise.all([
      device.queryClient.invalidateQueries({ queryKey: workspaceKeys.lists() }),
      device.queryClient.invalidateQueries({ queryKey: workspaceRootKeys.all }),
    ]);
  }

  function createRegisterWorkspaceMutation() {
    return createMutation(
      () => ({
        mutationFn: (input: RegisterWorkspaceInput) => device.api.registerWorkspace(input),
        onSuccess: invalidateWorkspaceCollections,
      }),
      () => device.queryClient,
    );
  }

  function createRenameWorkspaceMutation() {
    return createMutation(
      () => ({
        mutationFn: ({
          workspaceId,
          input,
        }: {
          workspaceId: string;
          input: RenameWorkspaceInput;
        }) => device.api.renameWorkspace(workspaceId, input),
        onSuccess: invalidateWorkspaceCollections,
      }),
      () => device.queryClient,
    );
  }

  function createDeleteWorkspaceMutation() {
    return createMutation(
      () => ({
        mutationFn: (workspaceId: string) => device.api.deleteWorkspace(workspaceId),
        onSuccess: async (_workspace, workspaceId) => {
          device.queryClient.removeQueries({
            queryKey: [...workspaceKeys.filePickers(), workspaceId],
          });
          await invalidateWorkspaceCollections();
        },
      }),
      () => device.queryClient,
    );
  }

  const refreshWorkspaceGitStatusMutationOptions = () => ({
    mutationFn: (workspaceId: string) => device.api.refreshWorkspaceGitStatus(workspaceId),
    onSuccess: (status: WorkspaceGitStatusView) => {
      device.queryClient.setQueryData(workspaceKeys.gitStatus(status.workspace_id), status);
    },
  });

  function createRefreshWorkspaceGitStatusMutation() {
    return createMutation(refreshWorkspaceGitStatusMutationOptions, () => device.queryClient);
  }

  async function refreshWorkspaceGitStatus(workspaceId: string): Promise<WorkspaceGitStatusView> {
    const observer = new MutationObserver(
      device.queryClient,
      refreshWorkspaceGitStatusMutationOptions(),
    );
    try {
      return await observer.mutate(workspaceId);
    } finally {
      observer.reset();
    }
  }
  return {
    workspaceKeys,
    workspaceRootKeys,
    createWorkspacesQuery,
    createWorkspacesStore,
    createWorkspaceRootsQuery,
    createWorkspaceGitStatusesStore,
    createWorkspaceRootEntriesQuery,
    fetchWorkspaces,
    fetchWorkspaceFilePickerEntries,
    invalidateWorkspaceQueries,
    createRegisterWorkspaceMutation,
    createRenameWorkspaceMutation,
    createDeleteWorkspaceMutation,
    createRefreshWorkspaceGitStatusMutation,
    refreshWorkspaceGitStatus,
  };
}
