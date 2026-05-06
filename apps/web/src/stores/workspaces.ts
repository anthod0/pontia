import { writable } from 'svelte/store';
import { listWorkspaces } from '../api/client';
import type { WorkspaceView } from '../api/types';

export const workspaces = writable<WorkspaceView[]>([]);
export const workspacesLoading = writable(false);
export const workspacesError = writable<string | null>(null);

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
