import { render, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import SessionChatPage from '../../src/pages/SessionChatPage.svelte';
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
      update(fn: (value: T) => T) {
        value = fn(value);
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
    submitInboxMessage: vi.fn(),
    cancelInboxMessage: vi.fn(),
    dismissInboxMessage: vi.fn(),
    interruptSession: vi.fn(),
    restartSession: vi.fn(),
    resumeSession: vi.fn(),
    terminateSession: vi.fn(),
    updateSessionTitle: vi.fn(),
    chatDraft: writableStore(''),
    optimisticInitialMessages: writableStore({}),
    workspaces: writableStore([]),
    workspacesError: writableStore<string | null>(null),
    workspaceGitStatuses: writableStore({}),
    workspaceGitStatusErrors: writableStore({}),
    loadWorkspaces: vi.fn(async () => []),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    timelineState: writableStore({
      sessionId: '',
      items: [],
      nextOlderTurnId: null,
      latestTurnId: null,
      hasMore: false,
      loading: false,
      refreshing: false,
      refreshKind: null,
      status: 'idle',
      errorCode: null,
      error: null,
    }),
    loadSessionTimeline: vi.fn(async () => null),
    resetTimelineState: vi.fn(),
    refreshSessionTimeline: vi.fn(async () => undefined),
    subscribeDashboardEvents: vi.fn(() => () => undefined),
  };
});

vi.mock('$lib/navigation', () => ({ navigate: mocks.navigate }));
vi.mock('svelte-sonner', () => ({ toast: { error: vi.fn() } }));
vi.mock('../../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsError: mocks.sessionsError,
  sessionDetail: mocks.sessionDetail,
  sessionDetailLoading: mocks.sessionDetailLoading,
  sessionDetailError: mocks.sessionDetailError,
  loadSessions: mocks.loadSessions,
  loadSessionDetail: mocks.loadSessionDetail,
  submitInboxMessage: mocks.submitInboxMessage,
  cancelInboxMessage: mocks.cancelInboxMessage,
  dismissInboxMessage: mocks.dismissInboxMessage,
  interruptSession: mocks.interruptSession,
  restartSession: mocks.restartSession,
  resumeSession: mocks.resumeSession,
  terminateSession: mocks.terminateSession,
  updateSessionTitle: mocks.updateSessionTitle,
}));
vi.mock('../../src/stores/chatDraft', () => ({ chatDraft: mocks.chatDraft, clearChatDraft: vi.fn() }));
vi.mock('../../src/stores/optimisticChat', () => ({ optimisticInitialMessages: mocks.optimisticInitialMessages, chatMessagesWithOptimistic: (_sessionId: string, loadedMessages: unknown[]) => loadedMessages }));
vi.mock('../../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesError: mocks.workspacesError,
  workspaceGitStatuses: mocks.workspaceGitStatuses,
  workspaceGitStatusErrors: mocks.workspaceGitStatusErrors,
  loadWorkspaces: mocks.loadWorkspaces,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
}));
vi.mock('../../src/stores/timeline', () => ({
  timelineState: mocks.timelineState,
  loadSessionTimeline: mocks.loadSessionTimeline,
  resetTimelineState: mocks.resetTimelineState,
  refreshSessionTimeline: mocks.refreshSessionTimeline,
  hasTimelineSnapshot: () => false,
}));
vi.mock('../../src/services/eventStream', () => ({ subscribeDashboardEvents: mocks.subscribeDashboardEvents }));

const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: 'session-1',
  client_type: 'generic',
  handle: 'generic-session',
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  state: 'idle',
  current_turn_id: null,
  workspace_id: 'workspace-1',
  workspace: null,
  capabilities: { timeline: false, context_usage: 'unsupported' },
  model: null,
  context_usage: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  metadata: {},
  ...overrides,
});

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/chat/session-1');
  const selected = session();
  mocks.pathParams = { sessionId: 'session-1' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set(null);
  mocks.sessionsError.set(null);
  mocks.sessionDetailError.set(null);
  mocks.sessionDetailLoading.set(false);
  mocks.chatDraft.set('');
  vi.clearAllMocks();
});

test('chat session route redirects clients without timeline support to session detail without loading timeline', async () => {
  render(SessionChatPage);

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-1'));
  await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/sessions/session-1'));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalled();
});
