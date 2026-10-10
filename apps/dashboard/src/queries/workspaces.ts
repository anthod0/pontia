import { createMutation, createQuery, QueryObserver, queryOptions } from "@tanstack/svelte-query";
import { readable } from "svelte/store";
import {
  deleteWorkspace as requestDeleteWorkspace,
  getWorkspace,
  getWorkspaceGitStatus,
  listWorkspaceFilePickerEntries,
  listWorkspaceRootEntries,
  listWorkspaceRoots,
  listWorkspaces,
  refreshWorkspaceGitStatus as requestRefreshWorkspaceGitStatus,
  registerWorkspace as requestRegisterWorkspace,
  renameWorkspace as requestRenameWorkspace,
} from "../api/client";
import type { RegisterWorkspaceInput, RenameWorkspaceInput } from "../api/types";
import { queryClient } from "./queryClient";

export const workspaceKeys = {
  all: ["workspaces"] as const,
  lists: () => [...workspaceKeys.all, "list"] as const,
  list: () => [...workspaceKeys.lists()] as const,
  details: () => [...workspaceKeys.all, "detail"] as const,
  detail: (workspaceId: string) => [...workspaceKeys.details(), workspaceId] as const,
  gitStatuses: () => [...workspaceKeys.all, "git-status"] as const,
  gitStatus: (workspaceId: string) => [...workspaceKeys.gitStatuses(), workspaceId] as const,
  filePickers: () => [...workspaceKeys.all, "file-picker"] as const,
  filePicker: (workspaceId: string, query: string, limit: number | undefined) =>
    [...workspaceKeys.filePickers(), workspaceId, { query, limit }] as const,
};

export const workspaceRootKeys = {
  all: ["workspace-roots"] as const,
  entries: (rootId: string, path: string) =>
    [...workspaceRootKeys.all, rootId, "entries", { path }] as const,
};

function workspacesOptions() {
  return queryOptions({
    queryKey: workspaceKeys.list(),
    queryFn: ({ signal }) => listWorkspaces({ signal }),
  });
}

function workspaceOptions(workspaceId: string, enabled = true) {
  return queryOptions({
    queryKey: workspaceKeys.detail(workspaceId),
    enabled: enabled && workspaceId.length > 0,
    queryFn: ({ signal }) => getWorkspace(workspaceId, { signal }),
  });
}

function workspaceGitStatusOptions(workspaceId: string, enabled = true) {
  return queryOptions({
    queryKey: workspaceKeys.gitStatus(workspaceId),
    enabled: enabled && workspaceId.length > 0,
    queryFn: ({ signal }) => getWorkspaceGitStatus(workspaceId, { signal }),
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
      listWorkspaceFilePickerEntries(workspaceId, query, {
        limit,
        signal: requestSignal ? AbortSignal.any([signal, requestSignal]) : signal,
      }),
  });
}

function workspaceRootsOptions() {
  return queryOptions({
    queryKey: workspaceRootKeys.all,
    queryFn: ({ signal }) => listWorkspaceRoots({ signal }),
  });
}

function workspaceRootEntriesOptions(rootId: string, path: string, enabled = true) {
  return queryOptions({
    queryKey: workspaceRootKeys.entries(rootId, path),
    enabled: enabled && rootId.length > 0,
    queryFn: ({ signal }) => listWorkspaceRootEntries(rootId, path, { signal }),
  });
}

export function createWorkspacesQuery() {
  return createQuery(workspacesOptions, () => queryClient);
}

export function createWorkspacesStore() {
  const observer = new QueryObserver(queryClient, workspacesOptions());
  return readable(observer.getCurrentResult(), (set) => {
    set(observer.getCurrentResult());
    return observer.subscribe(set);
  });
}

export function createWorkspaceQuery(
  workspaceId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => workspaceOptions(workspaceId(), enabled()),
    () => queryClient,
  );
}

export function createWorkspaceGitStatusQuery(
  workspaceId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => workspaceGitStatusOptions(workspaceId(), enabled()),
    () => queryClient,
  );
}

export function createWorkspaceRootsQuery() {
  return createQuery(workspaceRootsOptions, () => queryClient);
}

export function createWorkspaceRootEntriesQuery(
  rootId: () => string,
  path: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => workspaceRootEntriesOptions(rootId(), path(), enabled()),
    () => queryClient,
  );
}

export function fetchWorkspaces() {
  return queryClient.fetchQuery(workspacesOptions());
}

export function fetchWorkspaceGitStatus(workspaceId: string) {
  return queryClient.fetchQuery(workspaceGitStatusOptions(workspaceId));
}

export function fetchWorkspaceRootEntries(rootId: string, path = "") {
  return queryClient.fetchQuery(workspaceRootEntriesOptions(rootId, path));
}

export function fetchWorkspaceFilePickerEntries(
  workspaceId: string,
  query = "",
  options: { limit?: number; signal?: AbortSignal } = {},
) {
  return queryClient.fetchQuery(
    workspaceFilePickerOptions(workspaceId, query, options.limit, options.signal),
  );
}

export function invalidateWorkspaceQueries(): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
}

async function invalidateWorkspaceCollections(): Promise<void[]> {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: workspaceKeys.lists() }),
    queryClient.invalidateQueries({ queryKey: workspaceRootKeys.all }),
  ]);
}

export function createRegisterWorkspaceMutation() {
  return createMutation(
    () => ({
      mutationFn: (input: RegisterWorkspaceInput) => requestRegisterWorkspace(input),
      onSuccess: async (workspace) => {
        queryClient.setQueryData(workspaceKeys.detail(workspace.workspace_id), workspace);
        await invalidateWorkspaceCollections();
      },
    }),
    () => queryClient,
  );
}

export function createRenameWorkspaceMutation() {
  return createMutation(
    () => ({
      mutationFn: ({ workspaceId, input }: { workspaceId: string; input: RenameWorkspaceInput }) =>
        requestRenameWorkspace(workspaceId, input),
      onSuccess: async (workspace) => {
        queryClient.setQueryData(workspaceKeys.detail(workspace.workspace_id), workspace);
        await invalidateWorkspaceCollections();
      },
    }),
    () => queryClient,
  );
}

export function createDeleteWorkspaceMutation() {
  return createMutation(
    () => ({
      mutationFn: (workspaceId: string) => requestDeleteWorkspace(workspaceId),
      onSuccess: async (_workspace, workspaceId) => {
        await queryClient.cancelQueries({ queryKey: workspaceKeys.detail(workspaceId) });
        queryClient.removeQueries({ queryKey: workspaceKeys.detail(workspaceId) });
        queryClient.removeQueries({ queryKey: workspaceKeys.gitStatus(workspaceId) });
        queryClient.removeQueries({ queryKey: [...workspaceKeys.filePickers(), workspaceId] });
        await invalidateWorkspaceCollections();
      },
    }),
    () => queryClient,
  );
}

function refreshWorkspaceGitStatusMutationOptions() {
  return {
    mutationFn: (workspaceId: string) => requestRefreshWorkspaceGitStatus(workspaceId),
    onSuccess: (status: Awaited<ReturnType<typeof requestRefreshWorkspaceGitStatus>>) => {
      queryClient.setQueryData(workspaceKeys.gitStatus(status.workspace_id), status);
    },
  };
}

export function createRefreshWorkspaceGitStatusMutation() {
  return createMutation(refreshWorkspaceGitStatusMutationOptions, () => queryClient);
}

export function refreshWorkspaceGitStatus(workspaceId: string) {
  return queryClient
    .getMutationCache()
    .build(queryClient, refreshWorkspaceGitStatusMutationOptions())
    .execute(workspaceId);
}
