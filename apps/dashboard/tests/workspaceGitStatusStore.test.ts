import { get } from 'svelte/store';
import { beforeEach, expect, test, vi } from 'vitest';
import { loadWorkspaceGitStatus, refreshWorkspaceGitStatus, workspaceGitStatuses } from '../src/stores/workspaces';

const mocks = vi.hoisted(() => ({
  getWorkspaceGitStatus: vi.fn(),
  refreshWorkspaceGitStatusApi: vi.fn(),
}));

vi.mock('../src/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api/client')>()),
  getWorkspaceGitStatus: mocks.getWorkspaceGitStatus,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatusApi,
}));

beforeEach(() => {
  workspaceGitStatuses.set({});
  mocks.getWorkspaceGitStatus.mockReset();
  mocks.refreshWorkspaceGitStatusApi.mockReset();
});

test('stores git status by workspace id after read and refresh', async () => {
  mocks.getWorkspaceGitStatus.mockResolvedValue({ workspace_id: 'workspace-1', state: 'unknown', observed_at: null });
  mocks.refreshWorkspaceGitStatusApi.mockResolvedValue({ workspace_id: 'workspace-1', state: 'observed', branch: 'main', clean: false, observed_at: 'now' });

  await loadWorkspaceGitStatus('workspace-1');
  expect(get(workspaceGitStatuses)['workspace-1']).toMatchObject({ state: 'unknown' });

  await refreshWorkspaceGitStatus('workspace-1');
  expect(get(workspaceGitStatuses)['workspace-1']).toMatchObject({ state: 'observed', branch: 'main', clean: false });
});
