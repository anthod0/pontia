import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import { writable } from 'svelte/store';
import App from '../src/App.svelte';
import { token } from '../src/stores/auth';

const mocks = vi.hoisted(() => ({
  startEventStream: vi.fn(),
  stopEventStream: vi.fn(),
  loadAgentProfiles: vi.fn(async () => undefined),
  loadSessions: vi.fn(async () => undefined),
  loadTasks: vi.fn(async () => undefined),
  loadWorkspaces: vi.fn(async () => undefined),
}));

vi.mock('../src/services/eventStream', () => ({
  startEventStream: mocks.startEventStream,
  stopEventStream: mocks.stopEventStream,
}));
vi.mock('../src/stores/agentProfiles', () => ({
  agentProfiles: writable([]),
  agentProfilesError: writable(null),
  agentProfilesLoading: writable(false),
  loadAgentProfiles: mocks.loadAgentProfiles,
}));
vi.mock('../src/stores/sessions', () => ({
  sessions: writable([]),
  sessionsLoading: writable(false),
  loadSessions: mocks.loadSessions,
}));
vi.mock('../src/stores/tasks', () => ({
  tasks: writable([]),
  tasksError: writable(null),
  tasksLoading: writable(false),
  loadTasks: mocks.loadTasks,
}));
vi.mock('../src/stores/workspaces', () => ({
  workspaces: writable([]),
  workspacesError: writable(null),
  workspacesLoading: writable(false),
  loadWorkspaces: mocks.loadWorkspaces,
}));

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/tasks');
  localStorage.clear();
  token.set('');
  vi.clearAllMocks();
});

test('blocks dashboard routes behind a token prompt when no token is saved', async () => {
  render(App);

  expect(screen.getByRole('heading', { name: /enter external api token/i })).toBeInTheDocument();
  expect(screen.getByLabelText(/bearer token/i)).toBeInTheDocument();
  expect(screen.queryByText('pilotfy Dashboard')).not.toBeInTheDocument();
  expect(mocks.loadTasks).not.toHaveBeenCalled();
  expect(mocks.loadWorkspaces).not.toHaveBeenCalled();
  expect(mocks.loadAgentProfiles).not.toHaveBeenCalled();
  expect(mocks.loadSessions).not.toHaveBeenCalled();
  expect(mocks.startEventStream).not.toHaveBeenCalled();
});

test('saves the entered token and opens the requested dashboard route', async () => {
  render(App);

  await fireEvent.input(screen.getByLabelText(/bearer token/i), { target: { value: ' dev-token ' } });
  await fireEvent.click(screen.getByRole('button', { name: /continue/i }));

  await waitFor(() => expect(localStorage.getItem('pilotfy.externalApiToken')).toBe('dev-token'));
  await waitFor(() => expect(screen.queryByRole('heading', { name: /enter external api token/i })).not.toBeInTheDocument());
  expect(screen.getByText('pilotfy Dashboard')).toBeInTheDocument();
  expect(mocks.loadTasks).toHaveBeenCalled();
  expect(mocks.loadWorkspaces).toHaveBeenCalled();
  expect(mocks.loadAgentProfiles).toHaveBeenCalled();
  expect(mocks.loadSessions).toHaveBeenCalled();
  expect(mocks.startEventStream).toHaveBeenCalled();
});
