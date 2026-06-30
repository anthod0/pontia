import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import SessionDetailPage from '../../src/pages/SessionDetailPage.svelte';
import SessionsPage from '../../src/pages/SessionsPage.svelte';
import type { SessionConsoleDetail } from '../../src/stores/sessions';
import type { SessionView } from '../../src/api/types';

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

  const sessions = writableStore<SessionView[]>([]);
  const sessionDetail = writableStore<SessionConsoleDetail | null>(null);

  return {
    navigate: vi.fn(),
    pathParams: {} as Record<string, string>,
    sessions,
    sessionsLoading: writableStore(false),
    sessionsError: writableStore<string | null>(null),
    sessionDetail,
    sessionDetailLoading: writableStore(false),
    sessionDetailError: writableStore<string | null>(null),
    loadedSessions: [] as SessionView[],
    loadSessions: vi.fn(async () => mocks.loadedSessions),
    loadSessionDetail: vi.fn(async (sessionId: string) => {
      const selected = mocks.loadedSessions.find((session) => session.session_id === sessionId) ?? null;
      if (selected) mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
      return null;
    }),
    createSession: vi.fn(),
    interruptSession: vi.fn(),
    restartSession: vi.fn(),
    submitInboxMessage: vi.fn(),
    terminateSession: vi.fn(),
    workspaces: writableStore([]),
    loadWorkspaces: vi.fn(async () => []),
    agentProfiles: writableStore([]),
    loadAgentProfiles: vi.fn(async () => []),
  };
});

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate, getPathParams: () => mocks.pathParams }));
vi.mock('../../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  sessionsError: mocks.sessionsError,
  sessionDetail: mocks.sessionDetail,
  sessionDetailLoading: mocks.sessionDetailLoading,
  sessionDetailError: mocks.sessionDetailError,
  loadSessions: mocks.loadSessions,
  loadSessionDetail: mocks.loadSessionDetail,
  createSession: mocks.createSession,
  interruptSession: mocks.interruptSession,
  restartSession: mocks.restartSession,
  submitInboxMessage: mocks.submitInboxMessage,
  terminateSession: mocks.terminateSession,
}));
vi.mock('../../src/stores/workspaces', () => ({ workspaces: mocks.workspaces, loadWorkspaces: mocks.loadWorkspaces }));
vi.mock('../../src/stores/agentProfiles', () => ({
  agentProfiles: mocks.agentProfiles,
  loadAgentProfiles: mocks.loadAgentProfiles,
  clientTypeOptionsForProfile: () => ['pi'],
  defaultHandleForProfile: () => '',
  sessionProfileFields: () => ({}),
}));

const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: 'session-1',
  client_type: 'pi',
  handle: 'main',
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  state: 'idle',
  current_turn_id: null,
  workspace_id: 'workspace-1',
  workspace: null,
  capabilities: { context_usage: 'unsupported' },
  model: null,
  context_usage: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  metadata: {},
  ...overrides,
});

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/sessions');
  const first = session({ session_id: 'session-1', handle: 'first' });
  const second = session({ session_id: 'session-2', handle: 'second' });
  mocks.pathParams = {};
  mocks.loadedSessions = [first, second];
  mocks.sessions.set([first, second]);
  mocks.sessionDetail.set(null);
  mocks.sessionsLoading.set(false);
  mocks.sessionsError.set(null);
  mocks.sessionDetailLoading.set(false);
  mocks.sessionDetailError.set(null);
  vi.clearAllMocks();
});

test('sessions index is a pure list page and does not load session details', async () => {
  render(SessionsPage);

  await waitFor(() => expect(mocks.loadSessions).toHaveBeenCalled());
  expect(mocks.loadSessionDetail).not.toHaveBeenCalled();
  expect(screen.getByRole('heading', { name: /sessions/i })).toBeInTheDocument();
  expect(screen.queryByText(/create manual session/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/current turn output/i)).not.toBeInTheDocument();
});

test('sessions index rows navigate to the session detail page', async () => {
  render(SessionsPage);

  await fireEvent.click(await screen.findByRole('button', { name: /first/i }));
  expect(mocks.navigate).toHaveBeenCalledWith('/sessions/session-1');
});

test('session detail page shows unsupported context usage state', async () => {
  window.history.pushState({}, '', '/dashboard/sessions/session-1');
  mocks.pathParams = { sessionId: 'session-1' };

  render(SessionDetailPage);

  expect(await screen.findByText(/context usage not supported by this client/i)).toBeInTheDocument();
});

test('session detail page renders populated context usage', async () => {
  const withUsage = session({
    session_id: 'session-usage',
    capabilities: { context_usage: 'estimated' },
    context_usage: {
      used_tokens: 42000,
      max_tokens: 128000,
      remaining_tokens: 86000,
      usage_ratio: 0.328125,
      input_tokens: null,
      output_tokens: null,
      cache_tokens: null,
      confidence: 'estimated',
      observed_at: '2026-06-13T00:00:00Z',
    },
    model: 'example-model',
  });
  mocks.loadedSessions = [withUsage];
  mocks.sessions.set([withUsage]);
  window.history.pushState({}, '', '/dashboard/sessions/session-usage');
  mocks.pathParams = { sessionId: 'session-usage' };

  render(SessionDetailPage);

  expect(await screen.findByText(/context 33% · 42k \/ 128k · estimated/i)).toBeInTheDocument();
  expect(screen.getByText(/example-model/i)).toBeInTheDocument();
});

test('session detail page loads the selected session without the transcript timeline panel', async () => {
  window.history.pushState({}, '', '/dashboard/sessions/session-2');
  mocks.pathParams = { sessionId: 'session-2' };

  render(SessionDetailPage);

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2'));
  expect(await screen.findByRole('button', { name: /back to sessions/i })).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /agent transcript timeline/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /first/i })).not.toBeInTheDocument();
});
