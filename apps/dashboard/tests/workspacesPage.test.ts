import { render, screen, within } from '@testing-library/svelte';
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
      update(updater: (value: T) => T) {
        value = updater(value);
        for (const run of subscribers) run(value);
      },
    };
  }

  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesError = writableStore<string | null>(null);
  const workspaceRoots = writableStore<WorkspaceRootView[]>([]);
  const workspaceGitStatuses = writableStore({});
  const workspaceGitStatusErrors = writableStore({});

  return {
    workspaces,
    workspacesLoading,
    workspacesError,
    workspaceRoots,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    roots: [] as WorkspaceRootView[],
    listing: null as WorkspaceDirectoryListingView | null,
    loadWorkspaces: vi.fn(async () => undefined),
    loadWorkspaceRoots: vi.fn(async () => {
      workspaceRoots.set(mocks.roots);
      return mocks.roots;
    }),
    browseWorkspaceRoot: vi.fn(async () => mocks.listing),
    loadWorkspaceGitStatus: vi.fn(async () => undefined),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
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
  workspaceGitStatuses: mocks.workspaceGitStatuses,
  workspaceGitStatusErrors: mocks.workspaceGitStatusErrors,
  loadWorkspaces: mocks.loadWorkspaces,
  loadWorkspaceRoots: mocks.loadWorkspaceRoots,
  browseWorkspaceRoot: mocks.browseWorkspaceRoot,
  loadWorkspaceGitStatus: mocks.loadWorkspaceGitStatus,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
  registerWorkspace: mocks.registerWorkspace,
  renameWorkspace: mocks.renameWorkspace,
  deleteWorkspace: mocks.deleteWorkspace,
}));

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: 'workspace-1',
  name: 'pontia',
  canonical_path: '/repo/pontia',
  display_path: '/repo/pontia',
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
      { name: 'pontia', path: 'pontia', kind: 'directory', is_workspace: true },
      { name: 'sandbox', path: 'sandbox', kind: 'directory', is_workspace: false },
    ],
    warnings: [],
  };
  mocks.workspaceRoots.set(mocks.roots);
  mocks.workspaces.set([workspace()]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesError.set(null);
  mocks.workspaceGitStatuses.set({});
  mocks.workspaceGitStatusErrors.set({});
  vi.clearAllMocks();
});

test('renders a single root browser with active workspace controls in the directory list', async () => {
  const { container } = render(WorkspacesPage);

  expect(await screen.findByText('Browser')).toBeInTheDocument();
  expect(screen.queryByText('Root browser')).not.toBeInTheDocument();
  expect(screen.queryByText('Active workspaces')).not.toBeInTheDocument();
  expect(screen.queryByTestId('active-workspaces-list')).not.toBeInTheDocument();

  const browserCard = screen.getByText('Browser').closest('[data-slot="card"]');
  expect(browserCard).toHaveClass('mx-auto', 'max-w-5xl');

  const table = await screen.findByRole('table');
  expect(within(table).getAllByRole('button', { name: /Open directory/ })[0]).toHaveAccessibleName('Open directory pontia');
  const pontiaRow = within(table).getByRole('row', { name: /pontia/i });
  const deactivateButton = within(pontiaRow).getByRole('button', { name: 'Deactivate pontia' });
  expect(deactivateButton).toBeInTheDocument();
  expect(deactivateButton).toHaveTextContent('Deactivate');
  expect(deactivateButton.querySelector('svg')).not.toBeInTheDocument();
  expect(within(pontiaRow).getByRole('button', { name: 'Rename pontia' })).toBeInTheDocument();
  expect(within(pontiaRow).queryByRole('button', { name: 'Delete pontia' })).not.toBeInTheDocument();
  expect(container.querySelector('.workspace-folder-preview')).not.toBeInTheDocument();
  expect(mocks.loadWorkspaceGitStatus).not.toHaveBeenCalled();
  expect(screen.queryByRole('button', { name: /refresh git status/i })).not.toBeInTheDocument();
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
  expect(mocks.browseWorkspaceRoot).toHaveBeenLastCalledWith('root-1', 'sandbox', {});
});

test('shows outside-root active workspace banner and revokes workspaces from the dialog', async () => {
  const user = userEvent.setup();
  mocks.roots = [{ root_id: 'root-1', label: 'Projects', canonical_path: '/repo/project', state: 'available' }];
  mocks.workspaceRoots.set(mocks.roots);
  mocks.workspaces.set([
    workspace({ workspace_id: 'inside', name: 'inside', canonical_path: '/repo/project/app', display_path: '/repo/project/app' }),
    workspace({ workspace_id: 'outside', name: 'outside', canonical_path: '/repo/project-other/app', display_path: '/repo/project-other/app' }),
  ]);

  render(WorkspacesPage);

  expect(await screen.findByText('1 active workspace outside configured roots')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Review outside-root workspaces' }));

  const dialog = screen.getByRole('dialog', { name: 'Outside-root active workspaces' });
  expect(within(dialog).getByText('/repo/project-other/app')).toBeInTheDocument();
  expect(within(dialog).queryByText('/repo/project/app')).not.toBeInTheDocument();

  await user.click(within(dialog).getByRole('button', { name: 'Revoke outside' }));

  expect(mocks.deleteWorkspace).toHaveBeenCalledWith('outside');
});

test('uses path-boundary checks when classifying outside-root workspaces', async () => {
  mocks.roots = [{ root_id: 'root-1', label: 'Projects', canonical_path: '/repo/project', state: 'available' }];
  mocks.workspaceRoots.set(mocks.roots);
  mocks.workspaces.set([
    workspace({ workspace_id: 'nearby', name: 'nearby', canonical_path: '/repo/project-old', display_path: '/repo/project-old' }),
  ]);

  render(WorkspacesPage);

  expect(await screen.findByText('1 active workspace outside configured roots')).toBeInTheDocument();
});

test('toggles workspace active state directly and keeps rename dialog for editing names', async () => {
  const user = userEvent.setup();
  const confirmSpy = vi.spyOn(window, 'confirm');
  render(WorkspacesPage);

  const activateButton = await screen.findByRole('button', { name: 'Activate sandbox' });
  expect(activateButton).toHaveTextContent('Activate');
  await user.click(activateButton);

  expect(mocks.registerWorkspace).toHaveBeenCalledWith({ root_id: 'root-1', path: 'sandbox', name: 'sandbox' });
  expect(screen.queryByRole('heading', { name: 'Confirm workspace registration' })).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Deactivate pontia' }));

  expect(confirmSpy).not.toHaveBeenCalled();
  expect(mocks.deleteWorkspace).toHaveBeenCalledWith('workspace-1');

  await user.click(screen.getByRole('button', { name: 'Rename pontia' }));

  expect(screen.getByRole('dialog', { name: 'Confirm workspace rename' })).toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'Confirm workspace rename' })).toBeInTheDocument();
  expect(screen.getByLabelText('Display name')).toHaveValue('pontia');

  confirmSpy.mockRestore();
});

test('aborts initial settings workspace requests when the page unmounts', async () => {
  const { unmount } = render(WorkspacesPage);

  await vi.waitFor(() => expect(mocks.loadWorkspaces).toHaveBeenCalled());
  const workspaceOptions = mocks.loadWorkspaces.mock.calls[0][0] as { signal?: AbortSignal } | undefined;
  const rootsOptions = mocks.loadWorkspaceRoots.mock.calls[0][0] as { signal?: AbortSignal } | undefined;

  expect(workspaceOptions?.signal).toBeInstanceOf(AbortSignal);
  expect(rootsOptions?.signal).toBe(workspaceOptions?.signal);
  expect(workspaceOptions?.signal?.aborted).toBe(false);

  unmount();

  expect(workspaceOptions?.signal?.aborted).toBe(true);
});
