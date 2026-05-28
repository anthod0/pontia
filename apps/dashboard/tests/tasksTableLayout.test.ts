import { render, screen, within } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import TasksPage from '../src/pages/TasksPage.svelte';
import type { TaskView, WorkspaceView } from '../src/api/types';

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

  const tasks = writableStore<TaskView[]>([]);
  const tasksLoading = writableStore(false);
  const tasksError = writableStore<string | null>(null);
  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesError = writableStore<string | null>(null);

  return {
    tasks,
    tasksLoading,
    tasksError,
    workspaces,
    workspacesLoading,
    workspacesError,
    loadTasks: vi.fn(async () => undefined),
    createDagTask: vi.fn(),
    loadWorkspaces: vi.fn(async () => undefined),
    navigate: vi.fn(),
  };
});

vi.mock('../src/stores/tasks', () => ({
  tasks: mocks.tasks,
  tasksLoading: mocks.tasksLoading,
  tasksError: mocks.tasksError,
  loadTasks: mocks.loadTasks,
  createDagTask: mocks.createDagTask,
}));

vi.mock('../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  loadWorkspaces: mocks.loadWorkspaces,
}));

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate }));

const task = (overrides: Partial<TaskView> = {}): TaskView => ({
  task_id: 'task_abcdef123456',
  state: 'running',
  input: 'Implement dashboard test migration',
  workspace_id: 'workspace-with-a-long-identifier',
  session_id: null,
  turn_id: null,
  routing_state: 'unrouted',
  routing_reason: null,
  routing_confidence: null,
  metadata: {},
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:10:00Z',
  ...overrides,
});

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: 'workspace-1',
  name: 'llmparty',
  canonical_path: '/home/cheny/projects/llmparty',
  display_path: '/home/cheny/projects/llmparty',
  state: 'active',
  metadata: {},
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  last_used_at: null,
  ...overrides,
});

beforeEach(() => {
  mocks.tasks.set([task()]);
  mocks.tasksLoading.set(false);
  mocks.tasksError.set(null);
  mocks.workspaces.set([workspace()]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesError.set(null);
  vi.clearAllMocks();
});

test('renders DAG task table with fixed layout and clipped workspace cells', () => {
  render(TasksPage);

  const table = screen.getByRole('table');
  expect(table).toHaveClass('table-fixed');
  expect(screen.getByRole('columnheader', { name: 'Task' })).toHaveClass('w-[45%]');
  expect(screen.getByRole('columnheader', { name: 'Workspace' })).toHaveClass('w-[24%]');

  const row = screen.getByText('Implement dashboard test migration').closest('tr');
  expect(row).not.toBeNull();
  expect(within(row as HTMLTableRowElement).getByText('Implement dashboard test migration')).toHaveClass('truncate');

  const workspaceText = within(row as HTMLTableRowElement).getByText('workspace-with-a-long-identifier');
  expect(workspaceText).toHaveClass('truncate');
  expect(workspaceText.closest('td')).toHaveClass('max-w-0');
});
