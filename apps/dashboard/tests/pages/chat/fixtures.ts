import { cleanup } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { SessionConsoleDetail } from '../../../src/stores/sessions';
import type { TimelineState } from '../../../src/stores/timeline';
import type { CreateSessionResult, InboxMessageView, SessionView, TimelineItem, TurnView, WorkspaceView } from '../../../src/api/types';
import { optimisticInitialMessages } from '../../../src/stores/optimisticChat';

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
      get() {
        return value;
      },
    };
  }

  const timelineStateValue = (overrides: Partial<TimelineState> = {}): TimelineState => ({
    sessionId: '',
    mode: 'linear',
    groups: [],
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
    ...overrides,
  });

  const sessions = writableStore<SessionView[]>([]);
  const sessionsLoading = writableStore(false);
  const sessionsError = writableStore<string | null>(null);
  const sessionDetail = writableStore<SessionConsoleDetail | null>(null);
  const sessionDetailLoading = writableStore(false);
  const sessionDetailError = writableStore<string | null>(null);
  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesError = writableStore<string | null>(null);
  const workspaceGitStatuses = writableStore({});
  const workspaceGitStatusErrors = writableStore({});
  const timelineState = writableStore<TimelineState>(timelineStateValue());
  const dashboardEventListeners = new Set<(event: unknown) => void>();

  return {
    sessions,
    sessionsLoading,
    sessionsError,
    sessionDetail,
    sessionDetailLoading,
    sessionDetailError,
    workspaces,
    workspacesLoading,
    workspacesError,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    timelineState,
    timelineStateValue,
    dashboardEventListeners,
    loadedSessions: [] as SessionView[],
    loadSessions: vi.fn(async () => mocks.loadedSessions),
    loadSessionDetail: vi.fn(async () => null),
    submitInboxMessage: vi.fn(),
    cancelInboxMessage: vi.fn(),
    dismissInboxMessage: vi.fn(),
    resumeSession: vi.fn(),
    restartSession: vi.fn(),
    interruptSession: vi.fn(),
    terminateSession: vi.fn(),
    updateSessionTitle: vi.fn(),
    createSession: vi.fn(),
    loadSessionTimeline: vi.fn(async (sessionId: string) => null),
    refreshSessionTimeline: vi.fn(async () => undefined),
    resetTimelineState: vi.fn((sessionId = '') => {
      mocks.timelineState.set(mocks.timelineStateValue({ sessionId }));
    }),
    loadWorkspaces: vi.fn(async () => undefined),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    loadAgentProfiles: vi.fn(async () => undefined),
    toastError: vi.fn(),
    navigate: vi.fn(),
    pathParams: {} as Record<string, string>,
  };
});

vi.mock('../../../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  sessionsError: mocks.sessionsError,
  sessionDetail: mocks.sessionDetail,
  sessionDetailLoading: mocks.sessionDetailLoading,
  sessionDetailError: mocks.sessionDetailError,
  loadSessions: mocks.loadSessions,
  loadSessionDetail: mocks.loadSessionDetail,
  submitInboxMessage: mocks.submitInboxMessage,
  cancelInboxMessage: mocks.cancelInboxMessage,
  dismissInboxMessage: mocks.dismissInboxMessage,
  resumeSession: mocks.resumeSession,
  restartSession: mocks.restartSession,
  interruptSession: mocks.interruptSession,
  terminateSession: mocks.terminateSession,
  updateSessionTitle: mocks.updateSessionTitle,
  createSession: mocks.createSession,
}));

vi.mock('../../../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  workspaceGitStatuses: mocks.workspaceGitStatuses,
  workspaceGitStatusErrors: mocks.workspaceGitStatusErrors,
  loadWorkspaces: mocks.loadWorkspaces,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
}));

vi.mock('../../../src/stores/timeline', () => ({
  timelineState: mocks.timelineState,
  loadSessionTimeline: mocks.loadSessionTimeline,
  refreshSessionTimeline: mocks.refreshSessionTimeline,
  hasTimelineSnapshot: (state: TimelineState, sessionId: string) => state.sessionId === sessionId && (state.status === 'ready' || state.status === 'empty'),
  resetTimelineState: mocks.resetTimelineState,
}));

vi.mock('../../../src/services/eventStream', () => ({
  subscribeDashboardEvents: (listener: (event: unknown) => void) => {
    mocks.dashboardEventListeners.add(listener);
    return () => mocks.dashboardEventListeners.delete(listener);
  },
}));

vi.mock('$lib/navigation', () => ({ navigate: mocks.navigate }));

vi.mock('svelte-sonner', () => ({
  toast: { error: mocks.toastError },
}));

export const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: 'session-1',
  client_type: 'pi',
  title: null,
  handle: 'main',
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  state: 'idle',
  current_turn_id: null,
  workspace_id: 'workspace-1',
  workspace: null,
  capabilities: { accept_task: true, timeline: true },
  model: null,
  context_usage: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  metadata: {},
  ...overrides,
});

export const turn = (overrides: Partial<TurnView> = {}): TurnView => ({
  turn_id: 'turn-1',
  session_id: 'session-1',
  parent_turn_id: null,
  topology_status: 'unknown',
  state: 'completed',
  input: { summary: 'hello' },
  output: { summary: 'hi there' },
  failure: null,
  created_at: '2026-05-14T00:00:00Z',
  started_at: '2026-05-14T00:00:01Z',
  completed_at: '2026-05-14T00:00:02Z',
  metadata: {},
  ...overrides,
});

export const inboxMessage = (overrides: Partial<InboxMessageView> = {}): InboxMessageView => ({
  message_id: 'message-1',
  session_id: 'session-1',
  state: 'pending',
  delivery_policy: 'after_idle',
  input: { summary: 'queued follow-up' },
  metadata: {},
  turn_id: null,
  superseded_by_message_id: null,
  failure_message: null,
  created_at: '2026-05-14T00:00:03Z',
  updated_at: '2026-05-14T00:00:04Z',
  dispatched_at: null,
  cancelled_at: null,
  ...overrides,
});

export function timelineItemsFromTurns(turns: TurnView[]): TimelineItem[] {
  return turns.flatMap((item): TimelineItem[] => [
    {
      item_id: `${item.turn_id}:user`,
      kind: 'user',
      raw_kind: 'user',
      role: 'user',
      title: null,
      status: null,
      occurred_at: item.created_at,
      content_preview: typeof item.input?.summary === 'string' ? item.input.summary : null,
      turn_id: item.turn_id,
    },
    {
      item_id: `${item.turn_id}:assistant`,
      kind: 'assistant',
      raw_kind: 'text',
      role: 'assistant',
      title: null,
      status: item.failure ? 'error' : null,
      occurred_at: item.completed_at ?? item.created_at,
      content_preview: item.output?.summary ?? (typeof item.failure?.message === 'string' ? item.failure.message : null),
      turn_id: item.turn_id,
    },
  ]);
}

export const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: 'workspace-1',
  canonical_path: '/repo/pontia',
  display_path: '~/repo/pontia',
  name: 'pontia',
  state: 'active',
  metadata: {},
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  last_used_at: null,
  ...overrides,
});

export { mocks };
export const timelineStateValue = mocks.timelineStateValue;

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  if (!Element.prototype.hasPointerCapture) Element.prototype.hasPointerCapture = () => false;
  if (!Element.prototype.releasePointerCapture) Element.prototype.releasePointerCapture = () => undefined;
  window.history.pushState({}, '', '/dashboard/chat');
  const activeSession = session();
  mocks.loadedSessions = [activeSession];
  mocks.sessions.set([activeSession]);
  mocks.sessionsLoading.set(false);
  mocks.sessionsError.set(null);
  mocks.sessionDetail.set(null);
  mocks.sessionDetailLoading.set(false);
  mocks.sessionDetailError.set(null);
  mocks.workspaces.set([workspace()]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesError.set(null);
  mocks.workspaceGitStatuses.set({});
  mocks.workspaceGitStatusErrors.set({});
  mocks.timelineState.set(mocks.timelineStateValue());
  optimisticInitialMessages.set({});
  mocks.dashboardEventListeners.clear();
  mocks.pathParams = {};
  mocks.loadSessionDetail.mockReset().mockResolvedValue(null);
  mocks.loadSessionTimeline.mockReset();
  mocks.refreshSessionTimeline.mockReset().mockResolvedValue(undefined);
  mocks.createSession.mockResolvedValue({ session: activeSession, initial_turn: null } satisfies CreateSessionResult);
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    const detail = mocks.sessionDetail.get();
    const turns = detail?.turns ?? [];
    const page = {
      session_id: sessionId,
      items: timelineItemsFromTurns(turns),
      direction: 'backward' as const,
      next_turn_id: null,
    };
    mocks.timelineState.set(mocks.timelineStateValue({
      sessionId,
      items: page.items,
      nextOlderTurnId: page.next_turn_id,
      latestTurnId: page.items.at(-1)?.turn_id ?? null,
      hasMore: page.next_turn_id !== null,
      loading: false,
      refreshing: false,
      refreshKind: null,
      status: page.items.length ? 'ready' : 'empty',
    }));
    return page;
  });
  window.localStorage.clear();
  document.body.style.pointerEvents = '';
  vi.clearAllMocks();
});
