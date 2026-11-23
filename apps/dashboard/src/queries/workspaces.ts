import {
  createInfiniteQuery,
  createMutation,
  createQuery,
  MutationObserver,
  QueryObserver,
  queryOptions,
} from "@tanstack/svelte-query";

import { readable } from "svelte/store";

import type {
  GitChangeGroup,
  RegisterWorkspaceInput,
  RenameWorkspaceInput,
  WorkspaceGitStatusView,
} from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export type GitDiffTarget = { path: string; orig_path?: string; group: GitChangeGroup };
/** A file changed by a commit. */
export type GitCommitFileTarget = { path: string; orig_path?: string };

/** How often the open Git tab re-reads the change list. */
const GIT_CHANGES_POLL_MS = 5_000;

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
    git: (workspaceId: string) => [...workspaceKeys.all, "git", workspaceId] as const,
    gitChanges: (workspaceId: string) => [...workspaceKeys.git(workspaceId), "changes"] as const,
    gitDiff: (workspaceId: string, file: GitDiffTarget) =>
      [...workspaceKeys.git(workspaceId), "diff", file] as const,
    gitBranches: (workspaceId: string) => [...workspaceKeys.git(workspaceId), "branches"] as const,
    gitCommits: (workspaceId: string, ref: string) =>
      [...workspaceKeys.git(workspaceId), "commits", ref] as const,
    gitCommit: (workspaceId: string, sha: string) =>
      [...workspaceKeys.git(workspaceId), "commit", sha] as const,
    gitCommitDiff: (workspaceId: string, sha: string, file: GitCommitFileTarget) =>
      [...workspaceKeys.gitCommit(workspaceId, sha), "diff", file] as const,
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

  /** Reads only while `enabled`, so a hidden Git tab sends no requests. */
  function createWorkspaceGitChangesQuery(workspaceId: () => string, enabled: () => boolean) {
    return createQuery(
      () => ({
        queryKey: workspaceKeys.gitChanges(workspaceId()),
        queryFn: ({ signal }) => device.api.getWorkspaceGitChanges(workspaceId(), { signal }),
        enabled: enabled() && workspaceId().length > 0,
        refetchInterval: GIT_CHANGES_POLL_MS,
        refetchIntervalInBackground: false,
      }),
      () => device.queryClient,
    );
  }

  function createWorkspaceGitDiffQuery(workspaceId: () => string, file: () => GitDiffTarget) {
    return createQuery(
      () => ({
        queryKey: workspaceKeys.gitDiff(workspaceId(), file()),
        queryFn: ({ signal }) => device.api.getWorkspaceGitDiff(workspaceId(), file(), { signal }),
        staleTime: 0,
      }),
      () => device.queryClient,
    );
  }

  function createWorkspaceGitBranchesQuery(workspaceId: () => string) {
    return createQuery(
      () => ({
        queryKey: workspaceKeys.gitBranches(workspaceId()),
        queryFn: ({ signal }) => device.api.getWorkspaceGitBranches(workspaceId(), { signal }),
        staleTime: 0,
      }),
      () => device.queryClient,
    );
  }

  /** The history reachable from `ref`, newest first, one page at a time; a null `ref` reads nothing. */
  function createWorkspaceGitCommitsQuery(workspaceId: () => string, ref: () => string | null) {
    return createInfiniteQuery(
      () => ({
        queryKey: workspaceKeys.gitCommits(workspaceId(), ref() ?? ""),
        queryFn: ({ pageParam, signal }) =>
          device.api.getWorkspaceGitCommits(workspaceId(), ref()!, pageParam, { signal }),
        initialPageParam: 0,
        getNextPageParam: (last, pages) =>
          last.has_more ? pages.reduce((total, page) => total + page.commits.length, 0) : undefined,
        enabled: ref() !== null,
        staleTime: 0,
      }),
      () => device.queryClient,
    );
  }

  // A commit's contents never change, so they are read once.
  function createWorkspaceGitCommitQuery(workspaceId: () => string, sha: () => string) {
    return createQuery(
      () => ({
        queryKey: workspaceKeys.gitCommit(workspaceId(), sha()),
        queryFn: ({ signal }) => device.api.getWorkspaceGitCommit(workspaceId(), sha(), { signal }),
        staleTime: Infinity,
      }),
      () => device.queryClient,
    );
  }

  function createWorkspaceGitCommitDiffQuery(
    workspaceId: () => string,
    sha: () => string,
    file: () => GitCommitFileTarget,
  ) {
    return createQuery(
      () => ({
        queryKey: workspaceKeys.gitCommitDiff(workspaceId(), sha(), file()),
        queryFn: ({ signal }) =>
          device.api.getWorkspaceGitCommitDiff(workspaceId(), sha(), file(), { signal }),
        staleTime: Infinity,
      }),
      () => device.queryClient,
    );
  }

  /** Re-reads the Git tab's data for a Workspace, for example after an Agent turn ends. */
  function invalidateWorkspaceGit(workspaceId: string): Promise<void> {
    return device.queryClient.invalidateQueries({ queryKey: workspaceKeys.git(workspaceId) });
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
    createWorkspaceGitChangesQuery,
    createWorkspaceGitDiffQuery,
    createWorkspaceGitBranchesQuery,
    createWorkspaceGitCommitsQuery,
    createWorkspaceGitCommitQuery,
    createWorkspaceGitCommitDiffQuery,
    invalidateWorkspaceGit,
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
