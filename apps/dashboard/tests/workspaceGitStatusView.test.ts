import { render, screen, within } from '@testing-library/svelte';
import type { Writable } from 'svelte/store';
import { expect, test, vi } from 'vitest';
import WorkspacesPage from '../src/pages/WorkspacesPage.svelte';
import type { WorkspaceView } from '../src/api/types';

function writableStore<T>(initial: T): Writable<T> {
  let value = initial;
  const subscribers = new Set<(value: T) => void>();
  return {
    subscribe(run) {
      run(value);
      subscribers.add(run);
      return () => subscribers.delete(run);
    },
    set(next) {
      value = next;
      subscribers.forEach((run) => run(value));
    },
    update(updater) {
      value = updater(value);
      subscribers.forEach((run) => run(value));
    },
  };
}

const mocks = vi.hoisted(() => ({
  workspaces: writableStore<WorkspaceView[]>([]),
  workspacesLoading: writableStore(false),
  workspacesError: writableStore<string | null>(null),
  workspaceRoots: writableStore([]),
  workspaceGitStatuses: writableStore({}),
  workspaceGitStatusErrors: writableStore({}),
  loadWorkspaces: vi.fn(async () => undefined),
  loadWorkspaceRoots: vi.fn(async () => []),
  browseWorkspaceRoot: vi.fn(),
  registerWorkspace: vi.fn(),
  renameWorkspace: vi.fn(),
  deleteWorkspace: vi.fn(),
  loadWorkspaceGitStatus: vi.fn(async () => undefined),
  refreshWorkspaceGitStatus: vi.fn(async () => undefined),
}));

vi.mock('../src/stores/workspaces', () => mocks);

const workspace = (): WorkspaceView => ({
  workspace_id: 'workspace-1',
  canonical_path: '/repo/app',
  display_path: '/repo/app',
  name: 'App',
  state: 'active',
  metadata: {},
  created_at: '2026-01-01T00:00:00.000Z',
  updated_at: '2026-01-01T00:00:00.000Z',
  last_used_at: null,
});

test('shows workspace git status summary from store projection', () => {
  mocks.workspaces.set([workspace()]);
  mocks.workspaceGitStatuses.set({
    'workspace-1': {
      workspace_id: 'workspace-1',
      state: 'observed',
      repo_root: '/repo/app',
      branch: 'main',
      upstream: null,
      ahead: 0,
      behind: 1,
      staged_count: 1,
      unstaged_count: 2,
      untracked_count: 3,
      conflicted_count: 0,
      clean: false,
      failure: null,
      observed_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-01T00:00:00.000Z',
    },
  });

  render(WorkspacesPage);

  const list = screen.getByTestId('active-workspaces-list');
  expect(within(list).getByText('main')).toBeInTheDocument();
  expect(within(list).getByText('dirty')).toBeInTheDocument();
  expect(within(list).getByText('↓1')).toBeInTheDocument();
  expect(within(list).getByText('+1')).toBeInTheDocument();
  expect(within(list).getByText('~2')).toBeInTheDocument();
  expect(within(list).getByText('?3')).toBeInTheDocument();
});
