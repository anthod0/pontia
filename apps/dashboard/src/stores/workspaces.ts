import { writable } from 'svelte/store';
import {
  listWorkspaceRootEntries,
  deleteWorkspace as apiDeleteWorkspace,
  getWorkspaceGitStatus,
  listWorkspaceRoots,
  listWorkspaces,
  refreshWorkspaceGitStatus as apiRefreshWorkspaceGitStatus,
  registerWorkspace as apiRegisterWorkspace,
  renameWorkspace as apiRenameWorkspace,
  type ReadRequestOptions,
} from '../api/client';
import type {
  RegisterWorkspaceInput,
  RenameWorkspaceInput,
  WorkspaceDirectoryListingView,
  WorkspaceGitStatusView,
  WorkspaceRootView,
  WorkspaceView,
} from '../api/types';

export const workspaces = writable<WorkspaceView[]>([]);
export const workspacesLoading = writable(false);
export const workspacesError = writable<string | null>(null);
export const workspaceRoots = writable<WorkspaceRootView[]>([]);
export const workspaceGitStatuses = writable<Record<string, WorkspaceGitStatusView>>({});
export const workspaceGitStatusErrors = writable<Record<string, string>>({});

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

export async function loadWorkspaces(options: ReadRequestOptions = {}): Promise<void> {
  workspacesLoading.set(true);
  workspacesError.set(null);
  try {
    workspaces.set(await listWorkspaces(options));
  } catch (error) {
    if (!isAbortError(error)) workspacesError.set(error instanceof Error ? error.message : String(error));
  } finally {
    workspacesLoading.set(false);
  }
}

export async function loadWorkspaceRoots(options: ReadRequestOptions = {}): Promise<WorkspaceRootView[]> {
  const roots = await listWorkspaceRoots(options);
  workspaceRoots.set(roots);
  return roots;
}

export async function browseWorkspaceRoot(rootId: string, path = '', options: ReadRequestOptions = {}): Promise<WorkspaceDirectoryListingView> {
  return listWorkspaceRootEntries(rootId, path, options);
}

function setGitStatus(status: WorkspaceGitStatusView): void {
  workspaceGitStatuses.update((statuses) => ({ ...statuses, [status.workspace_id]: status }));
  workspaceGitStatusErrors.update((errors) => {
    const next = { ...errors };
    delete next[status.workspace_id];
    return next;
  });
}

function setGitStatusError(workspaceId: string, error: unknown): void {
  workspaceGitStatusErrors.update((errors) => ({
    ...errors,
    [workspaceId]: error instanceof Error ? error.message : String(error),
  }));
}

export async function loadWorkspaceGitStatus(workspaceId: string, options: ReadRequestOptions = {}): Promise<void> {
  try {
    setGitStatus(await getWorkspaceGitStatus(workspaceId, options));
  } catch (error) {
    if (!isAbortError(error)) setGitStatusError(workspaceId, error);
  }
}

export async function refreshWorkspaceGitStatus(workspaceId: string): Promise<void> {
  try {
    setGitStatus(await apiRefreshWorkspaceGitStatus(workspaceId));
  } catch (error) {
    setGitStatusError(workspaceId, error);
  }
}

export async function registerWorkspace(input: RegisterWorkspaceInput): Promise<WorkspaceView> {
  const workspace = await apiRegisterWorkspace(input);
  await loadWorkspaces();
  return workspace;
}

export async function renameWorkspace(workspaceId: string, input: RenameWorkspaceInput): Promise<WorkspaceView> {
  const workspace = await apiRenameWorkspace(workspaceId, input);
  await loadWorkspaces();
  return workspace;
}

export async function deleteWorkspace(workspaceId: string): Promise<WorkspaceView> {
  const workspace = await apiDeleteWorkspace(workspaceId);
  await loadWorkspaces();
  return workspace;
}
