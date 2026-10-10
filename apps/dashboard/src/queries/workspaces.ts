import { createMutation, createQuery, QueryObserver, queryOptions } from "@tanstack/svelte-query";
import { readable } from "svelte/store";
import {
  deleteWorkspace as requestDeleteWorkspace,
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
      onSuccess: invalidateWorkspaceCollections,
    }),
    () => queryClient,
  );
}

export function createRenameWorkspaceMutation() {
  return createMutation(
    () => ({
      mutationFn: ({ workspaceId, input }: { workspaceId: string; input: RenameWorkspaceInput }) =>
        requestRenameWorkspace(workspaceId, input),
      onSuccess: invalidateWorkspaceCollections,
    }),
    () => queryClient,
  );
}

export function createDeleteWorkspaceMutation() {
  return createMutation(
    () => ({
      mutationFn: (workspaceId: string) => requestDeleteWorkspace(workspaceId),
      onSuccess: async (_workspace, workspaceId) => {
        queryClient.removeQueries({ queryKey: [...workspaceKeys.filePickers(), workspaceId] });
        await invalidateWorkspaceCollections();
      },
    }),
    () => queryClient,
  );
}

export function refreshWorkspaceGitStatus(workspaceId: string) {
  return requestRefreshWorkspaceGitStatus(workspaceId);
}
