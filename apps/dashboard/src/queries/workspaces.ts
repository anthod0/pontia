import { createQuery, queryOptions } from "@tanstack/svelte-query";
import { listWorkspaceRootEntries, listWorkspaceRoots } from "../api/client";
import { queryClient } from "../lib/queryClient";

export const workspaceRootKeys = {
  all: ["workspace-roots"] as const,
  entries: (rootId: string, path: string) =>
    [...workspaceRootKeys.all, rootId, "entries", { path }] as const,
};

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

export function fetchWorkspaceRootEntries(rootId: string, path = "") {
  return queryClient.fetchQuery(workspaceRootEntriesOptions(rootId, path));
}
