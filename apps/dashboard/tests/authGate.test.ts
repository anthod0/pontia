import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import { writable } from 'svelte/store';
import AppLayoutHost from './components/AppLayoutHost.svelte';
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
  vi.unstubAllGlobals();
});

function mockValidateToken(status: number): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn(async () => new Response(
    status === 200
      ? JSON.stringify({ data: { authenticated: true }, error: null })
      : JSON.stringify({ data: null, error: { code: 'authentication_failed', message: 'missing or invalid bearer token' } }),
    { status },
  ));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

test('blocks dashboard routes behind a token prompt when no token is saved', async () => {
  render(AppLayoutHost);

  expect(screen.getByRole('heading', { name: /enter external api token/i })).toBeInTheDocument();
  expect(screen.getByLabelText(/bearer token/i)).toBeInTheDocument();
  expect(screen.queryByText('PONTIA')).not.toBeInTheDocument();
  expect(mocks.loadTasks).not.toHaveBeenCalled();
  expect(mocks.loadWorkspaces).not.toHaveBeenCalled();
  expect(mocks.loadAgentProfiles).not.toHaveBeenCalled();
  expect(mocks.loadSessions).not.toHaveBeenCalled();
  expect(mocks.startEventStream).not.toHaveBeenCalled();
});

test('rejects an invalid entered token without opening the dashboard', async () => {
  const fetchMock = mockValidateToken(401);
  render(AppLayoutHost);

  await fireEvent.input(screen.getByLabelText(/bearer token/i), { target: { value: ' wrong-token ' } });
  await fireEvent.click(screen.getByRole('button', { name: /continue/i }));

  await waitFor(() => expect(fetchMock).toHaveBeenCalledWith('/external/v1/auth/validate', expect.objectContaining({
    headers: expect.any(Headers),
  })));
  const headers = fetchMock.mock.calls[0][1]?.headers as Headers;
  expect(headers.get('Authorization')).toBe('Bearer wrong-token');
  expect(localStorage.getItem('pontia.externalApiToken')).not.toBe('wrong-token');
  expect(screen.getByRole('heading', { name: /enter external api token/i })).toBeInTheDocument();
  expect(await screen.findByText(/invalid token/i)).toBeInTheDocument();
  expect(screen.queryByText('PONTIA')).not.toBeInTheDocument();
  expect(mocks.startEventStream).not.toHaveBeenCalled();
});

test('saves the entered token and opens the requested dashboard route after validation succeeds', async () => {
  mockValidateToken(200);
  render(AppLayoutHost);

  await fireEvent.input(screen.getByLabelText(/bearer token/i), { target: { value: ' dev-token ' } });
  await fireEvent.click(screen.getByRole('button', { name: /continue/i }));

  await waitFor(() => expect(localStorage.getItem('pontia.externalApiToken')).toBe('dev-token'));
  await waitFor(() => expect(screen.queryByRole('heading', { name: /enter external api token/i })).not.toBeInTheDocument());
  expect(screen.getByText('PONTIA')).toBeInTheDocument();
  expect(mocks.loadTasks).toHaveBeenCalled();
  expect(mocks.loadWorkspaces).toHaveBeenCalled();
  expect(mocks.loadAgentProfiles).toHaveBeenCalled();
  expect(mocks.loadSessions).toHaveBeenCalled();
  expect(mocks.startEventStream).toHaveBeenCalled();
});

test('opens dashboard immediately when a saved token exists without startup validation', async () => {
  localStorage.setItem('pontia.externalApiToken', 'saved-token');
  token.set('saved-token');
  const fetchMock = mockValidateToken(401);

  render(AppLayoutHost);

  expect(screen.queryByRole('heading', { name: /enter external api token/i })).not.toBeInTheDocument();
  expect(screen.getByText('PONTIA')).toBeInTheDocument();
  expect(localStorage.getItem('pontia.externalApiToken')).toBe('saved-token');
  expect(fetchMock).not.toHaveBeenCalled();
  expect(mocks.loadTasks).toHaveBeenCalled();
  expect(mocks.startEventStream).toHaveBeenCalled();
});
