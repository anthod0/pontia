import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import ArchivedSessionsPage from '../../src/pages/ArchivedSessionsPage.svelte';
import type { SessionView } from '../../src/api/types';
import { loadSessions, selectSession, sessionDetail, sessions, sessionsError } from '../../src/stores/sessions';
import { workspaces } from '../../src/stores/workspaces';

const { navigate, successToast } = vi.hoisted(() => ({ navigate: vi.fn(), successToast: vi.fn() }));
vi.mock('$lib/navigation', () => ({ navigate }));
vi.mock('svelte-sonner', () => ({ toast: { success: successToast } }));

function session(client: string, overrides: Partial<SessionView> = {}): SessionView {
  return {
    session_id: `session-${client}`, client_type: client, title: `${client} history`,
    handle: null, role: null, description: null, execution_profile_id: null, execution_profile_version: null,
    state: 'exited', current_turn_id: null, workspace_id: 'workspace-1', workspace: '/projects/example',
    pinned_at: null, archived_at: '2026-09-24T10:00:00Z', capabilities: {}, model: null,
    context_usage: null, lineage: null, created_at: '2026-09-20T10:00:00Z', updated_at: '2026-09-24T10:00:00Z',
    metadata: { purpose: 'preserve me' }, ...overrides,
  };
}

function response(data: unknown): Response {
  return new Response(JSON.stringify({ data }), { status: 200 });
}

function failure(message: string): Response {
  return new Response(JSON.stringify({ error: { code: 'unavailable', message } }), { status: 503 });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

let backendSessions: SessionView[];
let requests: { url: string; method: string }[];
let archiveFailure: string | null;
let restoreFailure: string | null;
let listFailure: string | null;
let restoreResponse: (() => Promise<Response>) | null;

beforeEach(() => {
  vi.clearAllMocks();
  navigate.mockResolvedValue(undefined);
  backendSessions = [session('pi'), session('codex')];
  requests = [];
  archiveFailure = restoreFailure = listFailure = null;
  restoreResponse = null;
  sessions.set([]);
  sessionsError.set(null);
  sessionDetail.set(null);
  selectSession(null);
  workspaces.set([]);
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit) => {
    const url = String(input);
    requests.push({ url, method: init.method ?? 'GET' });
    if (url === '/api/v1/sessions?include_archived=true') {
      return archiveFailure ? failure(archiveFailure) : response({ sessions: backendSessions });
    }
    if (url === '/api/v1/sessions?limit=50&include_pinned=true') {
      return listFailure ? failure(listFailure) : response({ sessions: backendSessions.filter((item) => !item.archived_at) });
    }
    const restoring = backendSessions.find((item) => url === `/api/v1/sessions/${encodeURIComponent(item.session_id)}/unarchive`);
    if (restoring && init.method === 'POST') {
      if (restoreResponse) return restoreResponse();
      if (restoreFailure) return failure(restoreFailure);
      restoring.archived_at = null;
      return response({ session: restoring });
    }
    throw new Error(`Unexpected request: ${init.method} ${url}`);
  }));
});

afterEach(() => { vi.unstubAllGlobals(); });

test.each(['pi', 'codex'])('restores and opens the original %s session without resuming execution', async (client) => {
  const archived = session(client, client === 'codex' ? { codex: { connection: 'archived', thread_id: 'original-thread' } } : {});
  backendSessions = [archived];
  sessionDetail.set({ session: archived, turns: [], inboxMessages: [], events: [] });
  const before = structuredClone(archived);
  render(ArchivedSessionsPage);

  await fireEvent.click(await screen.findByRole('button', { name: `Restore and open ${client} history` }));

  await waitFor(() => expect(navigate).toHaveBeenCalledWith(`/chat/session-${client}`));
  const restored = { ...before, archived_at: null };
  expect(get(sessions)).toEqual([restored]);
  expect(get(sessionDetail)?.session).toEqual(restored);
  expect(requests.filter((request) => request.method === 'POST')).toEqual([
    { method: 'POST', url: `/api/v1/sessions/session-${client}/unarchive` },
  ]);
});

test('finds older archived sessions by workspace or client without replacing the normal list', async () => {
  const normal = session('pi', { session_id: 'normal', archived_at: null });
  backendSessions = [...Array.from({ length: 60 }, (_, i) => ({ ...normal, session_id: `normal-${i}` })), ...backendSessions];
  sessions.set([normal]);
  render(ArchivedSessionsPage);

  await screen.findByRole('button', { name: 'Restore and open codex history' });
  expect(screen.getAllByRole('listitem')).toHaveLength(2);
  const search = screen.getByRole('textbox', { name: 'Search archived sessions' });
  await fireEvent.input(search, { target: { value: 'example' } });
  expect(screen.getAllByRole('listitem')).toHaveLength(2);
  await fireEvent.input(search, { target: { value: 'CODEX' } });
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
  expect(within(screen.getByRole('listitem')).getByText('codex history')).toBeInTheDocument();
  expect(get(sessions)).toEqual([normal]);
});

test('distinguishes failed loading, an empty archive and search with no matches; refresh failure retains rows', async () => {
  archiveFailure = 'Archive is offline';
  render(ArchivedSessionsPage);
  expect(await screen.findByRole('alert')).toHaveTextContent('Archive is offline');
  expect(screen.queryByText('No archived sessions')).not.toBeInTheDocument();

  archiveFailure = null;
  backendSessions = [];
  await fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  expect(await screen.findByText('No archived sessions')).toBeInTheDocument();

  backendSessions = [session('pi')];
  await fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  await screen.findByRole('button', { name: 'Restore and open pi history' });
  await fireEvent.input(screen.getByRole('textbox'), { target: { value: 'missing' } });
  expect(screen.getByText('No matching sessions')).toBeInTheDocument();
  await fireEvent.input(screen.getByRole('textbox'), { target: { value: '' } });
  archiveFailure = 'Refresh failed';
  await fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Refresh failed');
  expect(screen.getByRole('button', { name: 'Restore and open pi history' })).toBeEnabled();
});

test('retains the failed target for retry and accepts an already restored session from another tab', async () => {
  restoreFailure = 'Restore rejected';
  render(ArchivedSessionsPage);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore and open pi history' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Restore rejected');
  expect(screen.getByRole('button', { name: 'Restore and open pi history' })).toBeEnabled();
  expect(navigate).not.toHaveBeenCalled();
  expect(successToast).not.toHaveBeenCalled();

  restoreFailure = null;
  backendSessions[0].archived_at = null;
  await fireEvent.click(screen.getByRole('button', { name: 'Restore and open pi history' }));
  await waitFor(() => expect(navigate).toHaveBeenCalledWith('/chat/session-pi'));
  expect(get(sessions).filter((item) => item.session_id === 'session-pi')).toHaveLength(1);
});

test('keeps a confirmed restore accessible after list refresh failure and retries only the read', async () => {
  listFailure = 'Normal list offline';
  render(ArchivedSessionsPage);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore and open pi history' }));

  expect(await screen.findByRole('alert')).toHaveTextContent('Session restored');
  expect(screen.getByRole('alert')).toHaveTextContent('Normal list offline');
  await waitFor(() => expect(screen.getByRole('button', { name: 'Open session', exact: true })).toBeEnabled());
  expect(get(sessions).map((item) => item.session_id)).toContain('session-pi');
  expect(navigate).not.toHaveBeenCalled();
  expect(successToast).not.toHaveBeenCalled();

  listFailure = null;
  await fireEvent.click(screen.getByRole('button', { name: 'Retry list refresh' }));
  await waitFor(() => expect(navigate).toHaveBeenCalledWith('/chat/session-pi'));
  expect(requests.filter((request) => request.method === 'POST')).toHaveLength(1);
});

test('does not claim success if the server still reports an archived session', async () => {
  restoreResponse = async () => response({ session: session('pi') });
  render(ArchivedSessionsPage);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore and open pi history' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('still archived');
  expect(screen.getByRole('button', { name: 'Restore and open pi history' })).toBeEnabled();
  expect(navigate).not.toHaveBeenCalled();
  expect(get(sessions)).toEqual([]);
});

test('disables repeated restores while pending and does not navigate after leaving the page', async () => {
  const pending = deferred<Response>();
  restoreResponse = () => pending.promise;
  const view = render(ArchivedSessionsPage);
  const button = await screen.findByRole('button', { name: 'Restore and open pi history' });
  await fireEvent.click(button);
  expect(button).toBeDisabled();
  await fireEvent.click(button);
  expect(requests.filter((request) => request.method === 'POST')).toHaveLength(1);
  view.unmount();
  pending.resolve(response({ session: session('pi', { archived_at: null }) }));
  await waitFor(() => expect(get(sessions)).toHaveLength(1));
  expect(navigate).not.toHaveBeenCalled();
});

test('offers the original session again if navigation fails after a confirmed restore', async () => {
  navigate.mockRejectedValueOnce(new Error('Route unavailable'));
  render(ArchivedSessionsPage);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore and open pi history' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Route unavailable');
  await fireEvent.click(screen.getByRole('button', { name: 'Open session', exact: true }));
  expect(navigate).toHaveBeenLastCalledWith('/chat/session-pi');
  expect(requests.filter((request) => request.method === 'POST')).toHaveLength(1);
});

test('an older normal-list response cannot hide a confirmed restore', async () => {
  const pending = deferred<Response>();
  vi.mocked(fetch).mockImplementationOnce(() => pending.promise);
  const oldList = loadSessions();
  listFailure = 'Refresh failed';
  render(ArchivedSessionsPage);
  await fireEvent.click(await screen.findByRole('button', { name: 'Restore and open pi history' }));
  await screen.findByRole('alert');
  pending.resolve(response({ sessions: [] }));
  await oldList;
  expect(get(sessions).map((item) => item.session_id)).toContain('session-pi');
});
