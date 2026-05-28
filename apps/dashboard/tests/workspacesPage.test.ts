import { fireEvent, render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, test, vi } from 'vitest';
import WorkspacesPage from '../src/pages/WorkspacesPage.svelte';
import type { WorkspaceDirectoryListingView, WorkspaceRootView, WorkspaceView } from '../src/api/types';

const mocks = vi.hoisted(() => {
  function writableStore<T>(initial: T) {
    let value = initial;
    const subscribers = new Set<(value: T) => void>();
    return {
      subscribe(run: (value: T) => void) {
        subscribers.add(run);
        run(value);
        return () => subscribers.delete(run);
      },
      set(next: T) {
        value = next;
        for (const run of subscribers) run(value);
      },
    };
  }

  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesError = writableStore<string | null>(null);
  const workspaceRoots = writableStore<WorkspaceRootView[]>([]);

  return {
    workspaces,
    workspacesLoading,
    workspacesError,
    workspaceRoots,
    roots: [] as WorkspaceRootView[],
    listing: null as WorkspaceDirectoryListingView | null,
    loadWorkspaces: vi.fn(async () => undefined),
    loadWorkspaceRoots: vi.fn(async () => {
      workspaceRoots.set(mocks.roots);
      return mocks.roots;
    }),
    browseWorkspaceRoot: vi.fn(async () => mocks.listing),
    registerWorkspace: vi.fn(async () => undefined),
    renameWorkspace: vi.fn(async () => undefined),
    deleteWorkspace: vi.fn(async () => undefined),
  };
});

vi.mock('../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  workspaceRoots: mocks.workspaceRoots,
  loadWorkspaces: mocks.loadWorkspaces,
  loadWorkspaceRoots: mocks.loadWorkspaceRoots,
  browseWorkspaceRoot: mocks.browseWorkspaceRoot,
  registerWorkspace: mocks.registerWorkspace,
  renameWorkspace: mocks.renameWorkspace,
  deleteWorkspace: mocks.deleteWorkspace,
}));

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: 'workspace-1',
  name: 'llmparty',
  canonical_path: '/repo/llmparty',
  display_path: '/repo/llmparty',
  state: 'active',
  metadata: {},
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  last_used_at: null,
  ...overrides,
});

beforeEach(() => {
  mocks.roots = [{ root_id: 'root-1', label: 'Projects', canonical_path: '/repo', state: 'active' }];
  mocks.listing = {
    root_id: 'root-1',
    path: '',
    canonical_path: '/repo',
    parent_path: null,
    entries: [
      { name: 'llmparty', path: 'llmparty', kind: 'directory', is_workspace: true },
      { name: 'sandbox', path: 'sandbox', kind: 'directory', is_workspace: false },
    ],
    warnings: [],
  };
  mocks.workspaceRoots.set(mocks.roots);
  mocks.workspaces.set([workspace()]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesError.set(null);
  vi.clearAllMocks();
});

test('renders workspace browser and active workspace cards from store data', async () => {
  const { container } = render(WorkspacesPage);

  expect(await screen.findByText('Root browser')).toBeInTheDocument();
  expect(screen.getByText('Active workspaces')).toBeInTheDocument();

  const activeSection = screen.getByText('Active workspaces').closest('.xl\\:order-2');
  expect(activeSection).not.toBeNull();
  expect(within(activeSection as HTMLElement).getByText('llmparty')).toBeInTheDocument();
  expect(container.querySelector('.workspace-folder-preview')).toBeInTheDocument();
  expect(container.querySelector('.workspace-folder-tab')).toBeInTheDocument();
  expect(container.querySelector('.workspace-folder-body')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Rename llmparty' })).toBeInTheDocument();
});

test('renders a compact directory/action table and opens directories through the folder-name button', async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  const table = await screen.findByRole('table');
  expect(within(table).getByRole('columnheader', { name: 'Directory' })).toBeInTheDocument();
  expect(within(table).getByRole('columnheader', { name: 'Action' })).toBeInTheDocument();
  expect(within(table).queryByRole('columnheader', { name: 'Kind' })).not.toBeInTheDocument();
  expect(within(table).queryByRole('columnheader', { name: 'Workspace' })).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Open directory sandbox' }));
  expect(mocks.browseWorkspaceRoot).toHaveBeenCalledWith('root-1', 'sandbox');
});

test('opens registration and rename dialogs from user actions', async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  await screen.findByRole('button', { name: 'Activate sandbox' });
  await user.click(screen.getByRole('button', { name: 'Activate sandbox' }));

  expect(screen.getByRole('heading', { name: 'Confirm workspace registration' })).toBeInTheDocument();
  expect(screen.getByLabelText('Display name')).toHaveValue('sandbox');

  await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await user.click(screen.getByRole('button', { name: 'Rename llmparty' }));

  expect(screen.getByRole('heading', { name: 'Confirm workspace rename' })).toBeInTheDocument();
  expect(screen.getByLabelText('Display name')).toHaveValue('llmparty');
});
