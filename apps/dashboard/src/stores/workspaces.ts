import { writable } from 'svelte/store';
import {
  listWorkspaceRootEntries,
  listWorkspaceRoots,
  listWorkspaces,
  registerWorkspace as apiRegisterWorkspace,
} from '../api/client';
import type {
  RegisterWorkspaceInput,
  WorkspaceDirectoryListingView,
  WorkspaceRootView,
  WorkspaceView,
} from '../api/types';

export const workspaces = writable<WorkspaceView[]>([]);
export const workspacesLoading = writable(false);
export const workspacesError = writable<string | null>(null);
export const workspaceRoots = writable<WorkspaceRootView[]>([]);

export async function loadWorkspaces(): Promise<void> {
  workspacesLoading.set(true);
  workspacesError.set(null);
  try {
    workspaces.set(await listWorkspaces());
  } catch (error) {
    workspacesError.set(error instanceof Error ? error.message : String(error));
  } finally {
    workspacesLoading.set(false);
  }
}

export async function loadWorkspaceRoots(): Promise<WorkspaceRootView[]> {
  const roots = await listWorkspaceRoots();
  workspaceRoots.set(roots);
  return roots;
}

export async function browseWorkspaceRoot(rootId: string, path = ''): Promise<WorkspaceDirectoryListingView> {
  return listWorkspaceRootEntries(rootId, path);
}

export async function registerWorkspace(input: RegisterWorkspaceInput): Promise<WorkspaceView> {
  const workspace = await apiRegisterWorkspace(input);
  await loadWorkspaces();
  return workspace;
}
