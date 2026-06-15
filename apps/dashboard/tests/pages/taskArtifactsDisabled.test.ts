import { render, screen } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import TaskArtifactsPage from '../../src/pages/task/TaskArtifactsPage.svelte';

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

  return {
    task: writableStore(null),
    taskDag: writableStore(null),
    taskArtifacts: writableStore([]),
    taskArtifactsLoading: writableStore(false),
    taskArtifactsError: writableStore(null),
    selectedArtifactContent: writableStore(null),
    artifactContentLoading: writableStore(false),
    artifactContentError: writableStore(null),
    discoverTaskArtifacts: vi.fn(),
    loadArtifactContent: vi.fn(),
    loadTaskArtifacts: vi.fn(),
  };
});

vi.mock('svelte-mini-router', () => ({ navigate: vi.fn(), getPathParams: () => ({ taskId: 'task-1' }) }));

vi.mock('../../src/stores/tasks', () => ({
  task: mocks.task,
  taskDag: mocks.taskDag,
  taskError: mocks.taskArtifactsError,
  taskLoading: mocks.taskArtifactsLoading,
  selectedTaskId: { set: vi.fn() },
  refreshTask: vi.fn(async () => null),
}));

vi.mock('../../src/stores/artifacts', () => ({
  taskArtifacts: mocks.taskArtifacts,
  taskArtifactsLoading: mocks.taskArtifactsLoading,
  taskArtifactsError: mocks.taskArtifactsError,
  selectedArtifactContent: mocks.selectedArtifactContent,
  artifactContentLoading: mocks.artifactContentLoading,
  artifactContentError: mocks.artifactContentError,
  discoverTaskArtifacts: mocks.discoverTaskArtifacts,
  loadArtifactContent: mocks.loadArtifactContent,
  loadTaskArtifacts: mocks.loadTaskArtifacts,
}));

test('task artifacts page is a disabled placeholder and does not auto-load artifacts', () => {
  render(TaskArtifactsPage);

  expect(mocks.loadTaskArtifacts).not.toHaveBeenCalled();
  expect(mocks.discoverTaskArtifacts).not.toHaveBeenCalled();
  expect(screen.getByText(/artifacts are temporarily disabled/i)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /discover artifacts/i })).not.toBeInTheDocument();
});
