import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import ChatPage from '../../src/pages/ChatPage.svelte';
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
  const sessionsLoading = writableStore(false);
  const sessionsError = writableStore<string | null>(null);
  const sessionDetail = writableStore<SessionConsoleDetail | null>(null);
  const sessionDetailLoading = writableStore(false);
  const sessionDetailError = writableStore<string | null>(null);

  return {
    sessions,
    sessionsLoading,
    sessionsError,
    sessionDetail,
    sessionDetailLoading,
    sessionDetailError,
    loadedSessions: [] as SessionView[],
    loadSessions: vi.fn(async () => mocks.loadedSessions),
    loadSessionDetail: vi.fn(async () => null),
    submitInboxMessage: vi.fn(),
    navigate: vi.fn(),
  };
});

vi.mock('../../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  sessionsError: mocks.sessionsError,
  sessionDetail: mocks.sessionDetail,
  sessionDetailLoading: mocks.sessionDetailLoading,
  sessionDetailError: mocks.sessionDetailError,
  loadSessions: mocks.loadSessions,
  loadSessionDetail: mocks.loadSessionDetail,
  submitInboxMessage: mocks.submitInboxMessage,
}));

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate }));

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
  capabilities: {},
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  metadata: {},
  ...overrides,
});

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/chat');
  const activeSession = session();
  mocks.loadedSessions = [activeSession];
  mocks.sessions.set([activeSession]);
  mocks.sessionsLoading.set(false);
  mocks.sessionsError.set(null);
  mocks.sessionDetail.set({ session: activeSession, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.sessionDetailLoading.set(false);
  mocks.sessionDetailError.set(null);
  vi.clearAllMocks();
});

test('does not render a manual refresh button in the chat header', () => {
  render(ChatPage);

  expect(screen.getByRole('button', { name: /session console/i })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /refresh/i })).not.toBeInTheDocument();
});

test('does not render the sessions list inside chat content', () => {
  render(ChatPage);

  expect(screen.queryByText('Pick an existing session to continue.')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /active/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^all$/i })).not.toBeInTheDocument();
});

test('loads the session selected by the chat query parameter', async () => {
  const first = session({ session_id: 'session-1', handle: 'first' });
  const second = session({ session_id: 'session-2', handle: 'second' });
  window.history.pushState({}, '', '/dashboard/chat?session=session-2');
  mocks.loadedSessions = [first, second];
  mocks.sessions.set([first, second]);
  mocks.sessionDetail.set({ session: second, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2'));
});

test('updates the selected chat session when the query parameter changes on the mounted page', async () => {
  const first = session({ session_id: 'session-1', handle: 'first' });
  const second = session({ session_id: 'session-2', handle: 'second' });
  mocks.loadedSessions = [first, second];
  mocks.sessions.set([first, second]);
  mocks.sessionDetail.set({ session: first, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);
  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-1'));

  window.history.pushState({}, '', '/dashboard/chat?session=session-2');
  window.dispatchEvent(new PopStateEvent('popstate'));

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2'));
});
