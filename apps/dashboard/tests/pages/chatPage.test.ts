import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import ChatPage from '../../src/pages/ChatPage.svelte';
import type { SessionConsoleDetail } from '../../src/stores/sessions';
import type { AgentProfileView, CreateDagTaskResult, CreateSessionResult, InboxMessageView, SessionView, TimelineItem, TurnView, WorkspaceView } from '../../src/api/types';

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
  const agentProfiles = writableStore<AgentProfileView[]>([]);
  const agentProfilesLoading = writableStore(false);
  const agentProfilesError = writableStore<string | null>(null);
  const taskProposals = writableStore<unknown[]>([]);
  const taskProposalsLoading = writableStore(false);
  const taskProposalsError = writableStore<string | null>(null);
  const timelineState = writableStore({
    sessionId: '',
    bindingId: null,
    items: [] as TimelineItem[],
    headCursor: null,
    tailCursor: null,
        sourceId: null,
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });
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
    agentProfiles,
    agentProfilesLoading,
    agentProfilesError,
    taskProposals,
    taskProposalsLoading,
    taskProposalsError,
    timelineState,
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
    createDagTask: vi.fn(),
    loadTaskProposals: vi.fn(async () => []),
    loadSessionTimeline: vi.fn(async (sessionId: string) => null),
    handleTimelineMessageUpdated: vi.fn(async () => undefined),
    resetTimelineState: vi.fn((sessionId = '') => {
      mocks.timelineState.set({
        sessionId,
        bindingId: null,
        items: [],
        headCursor: null,
    tailCursor: null,
                sourceId: null,
        hasMore: false,
                loading: false,
        refreshing: false,
        error: null,
      });
    }),
    loadWorkspaces: vi.fn(async () => undefined),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    loadAgentProfiles: vi.fn(async () => undefined),
    toastError: vi.fn(),
    navigate: vi.fn(),
    pathParams: {} as Record<string, string>,
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
  cancelInboxMessage: mocks.cancelInboxMessage,
  dismissInboxMessage: mocks.dismissInboxMessage,
  resumeSession: mocks.resumeSession,
  restartSession: mocks.restartSession,
  interruptSession: mocks.interruptSession,
  terminateSession: mocks.terminateSession,
  updateSessionTitle: mocks.updateSessionTitle,
  createSession: mocks.createSession,
}));

vi.mock('../../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  workspaceGitStatuses: mocks.workspaceGitStatuses,
  workspaceGitStatusErrors: mocks.workspaceGitStatusErrors,
  loadWorkspaces: mocks.loadWorkspaces,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
}));

vi.mock('../../src/stores/tasks', () => ({
  createDagTask: mocks.createDagTask,
  taskProposals: mocks.taskProposals,
  taskProposalsLoading: mocks.taskProposalsLoading,
  taskProposalsError: mocks.taskProposalsError,
  loadTaskProposals: mocks.loadTaskProposals,
}));

vi.mock('../../src/stores/timeline', () => ({
  timelineState: mocks.timelineState,
  loadSessionTimeline: mocks.loadSessionTimeline,
  handleTimelineMessageUpdated: mocks.handleTimelineMessageUpdated,
  resetTimelineState: mocks.resetTimelineState,
}));

vi.mock('../../src/services/eventStream', () => ({
  subscribeDashboardEvents: (listener: (event: unknown) => void) => {
    mocks.dashboardEventListeners.add(listener);
    return () => mocks.dashboardEventListeners.delete(listener);
  },
}));

vi.mock('../../src/stores/agentProfiles', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../src/stores/agentProfiles')>();
  return {
    ...actual,
    agentProfiles: mocks.agentProfiles,
    agentProfilesLoading: mocks.agentProfilesLoading,
    agentProfilesError: mocks.agentProfilesError,
    loadAgentProfiles: mocks.loadAgentProfiles,
  };
});

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate, getPathParams: () => mocks.pathParams }));

vi.mock('svelte-sonner', () => ({
  toast: { error: mocks.toastError },
}));

const session = (overrides: Partial<SessionView> = {}): SessionView => ({
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
  capabilities: { accept_task: true },
  model: null,
  context_usage: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  metadata: {},
  ...overrides,
});

const turn = (overrides: Partial<TurnView> = {}): TurnView => ({
  turn_id: 'turn-1',
  session_id: 'session-1',
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

const inboxMessage = (overrides: Partial<InboxMessageView> = {}): InboxMessageView => ({
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

function timelineItemsFromTurns(turns: TurnView[]): TimelineItem[] {
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
      content_ref: `${item.turn_id}:user-ref`,
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
      content_ref: `${item.turn_id}:assistant-ref`,
      turn_id: item.turn_id,
    },
  ]);
}

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
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

const profile = (overrides: Partial<AgentProfileView> = {}): AgentProfileView => ({
  profile_id: 'coder',
  version: '1',
  name: 'Coder',
  description: null,
  supported_client_types: ['pi'],
  agent_kind: 'executor',
  system_prompt_template: null,
  turn_prompt_template: null,
  default_session_role: 'coder',
  default_session_description: 'Coding session',
  handle_prefix: 'coder',
  expected_output_schema: null,
  artifact_contract: {},
  default_execution_policy: {},
  default_review_policy: {},
  metadata: {},
  active: true,
  archived_at: null,
  archived_reason: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  ...overrides,
});

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
  mocks.agentProfiles.set([profile()]);
  mocks.agentProfilesLoading.set(false);
  mocks.agentProfilesError.set(null);
  mocks.taskProposals.set([]);
  mocks.taskProposalsLoading.set(false);
  mocks.taskProposalsError.set(null);
  mocks.timelineState.set({
    sessionId: '',
    bindingId: null,
    items: [],
    headCursor: null,
    tailCursor: null,
        sourceId: null,
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });
  mocks.dashboardEventListeners.clear();
  mocks.pathParams = {};
  mocks.createSession.mockResolvedValue({ session: activeSession, initial_turn: null } satisfies CreateSessionResult);
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    const detail = mocks.sessionDetail.get();
    const turns = detail?.turns ?? [];
    const page = {
      session_id: sessionId,
      binding_id: 'binding-1',
      items: timelineItemsFromTurns(turns),
      head_cursor: null,
      tail_cursor: null,
      has_more: false,
      source_id: 'source-1',
    };
    mocks.timelineState.set({
      sessionId,
      bindingId: page.binding_id,
      items: page.items,
      headCursor: page.head_cursor,
      tailCursor: page.tail_cursor,
      sourceId: page.source_id,
      hasMore: page.has_more,
      loading: false,
      refreshing: false,
      error: null,
    });
    return page;
  });
  mocks.createDagTask.mockResolvedValue({
    task: {
      task_id: 'task-new',
      input: 'Manual task',
      state: 'queued',
      routing_state: 'unassigned',
      workspace_id: 'workspace-1',
      session_id: null,
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T00:00:00Z',
      metadata: {},
    },
    planning_turn: { task_id: 'task-new', session_id: 'session-planner', turn_id: 'turn-planner', profile_id: 'planner' },
  } satisfies CreateDagTaskResult);
  window.localStorage.clear();
  document.body.style.pointerEvents = '';
  vi.clearAllMocks();
});

test('prefers the last selected new chat workspace when it is still available', async () => {
  window.localStorage.setItem('pontia.chat.lastWorkspaceId', 'workspace-2');
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(ChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(screen.getByLabelText(/workspace/i)).toHaveTextContent('sandbox');
});

test('falls back to the first new chat workspace when the remembered workspace is unavailable', async () => {
  window.localStorage.setItem('pontia.chat.lastWorkspaceId', 'missing-workspace');
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(ChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(screen.getByLabelText(/workspace/i)).toHaveTextContent('pontia');
});

test('remembers the selected new chat workspace after starting a chat', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-selected-workspace' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-selected-workspace' }) } satisfies CreateSessionResult);
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(ChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  const workspaceSelector = screen.getByLabelText(/workspace/i);
  await user.click(workspaceSelector);
  await user.keyboard('{ArrowDown}{Enter}{Escape}');
  expect(workspaceSelector).toHaveTextContent('sandbox');
  document.body.style.pointerEvents = '';
  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Use sandbox');
  await user.click(screen.getByRole('button', { name: /start chat/i }));

  await vi.waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({ workspace_id: 'workspace-2' })));
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-2');
});

test('renders a clean centered prompt input on the bare chat route instead of selecting an existing session', async () => {
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(promptInput).toHaveValue('');
  expect(screen.getByRole('heading', { name: /new chat/i })).toBeInTheDocument();
  expect(screen.getByText('Start a new agent session from a prompt, workspace, client, and profile.')).toBeInTheDocument();
  const centeredPanel = screen.getByTestId('new-chat-centered-panel');
  expect(centeredPanel).toHaveClass('justify-center');
  expect(centeredPanel).toContainElement(screen.getByRole('heading', { name: /new chat/i }));
  expect(centeredPanel).toContainElement(promptInput);
  expect(screen.queryByText(/Enter the first prompt/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/^Prompt$/i)).not.toBeInTheDocument();
  expect(screen.getByLabelText(/workspace/i)).toHaveTextContent('pontia');
  expect(screen.getByLabelText(/client/i)).toHaveTextContent('pi');
  expect(screen.queryByLabelText(/profile/i)).not.toBeInTheDocument();
  expect(mocks.loadSessionDetail).not.toHaveBeenCalled();
});

test('renders new chat selectors as compact metadata pills above the prompt input', async () => {
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(promptInput).toHaveClass('min-h-20');
  expect(promptInput).not.toHaveClass('min-h-28');
  expect(promptInput).not.toHaveClass('text-base');
  const workspaceSelector = screen.getByLabelText(/workspace/i);
  const clientSelector = screen.getByLabelText(/client/i);

  expect(screen.queryByLabelText(/profile/i)).not.toBeInTheDocument();
  expect(workspaceSelector.compareDocumentPosition(clientSelector) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(workspaceSelector.compareDocumentPosition(promptInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(clientSelector.compareDocumentPosition(promptInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  for (const selector of [workspaceSelector, clientSelector]) {
    expect(selector).toHaveClass('h-7');
    expect(selector).toHaveClass('rounded-full');
    expect(selector).toHaveClass('text-muted-foreground');
    expect(selector.closest('form')).toBeNull();
  }
  expect(workspaceSelector.querySelector('svg')).toHaveClass('lucide-folder');
  expect(clientSelector.querySelector('svg')).toHaveClass('lucide-terminal');
});

test('hides task mode toggle from the chat composer metadata controls', async () => {
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  const workspaceSelector = screen.getByLabelText(/workspace/i);
  const clientSelector = screen.getByLabelText(/client/i);
  const submit = screen.getByRole('button', { name: /start chat/i });

  expect(screen.queryByRole('button', { name: /task mode off/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /task mode on/i })).not.toBeInTheDocument();
  expect(workspaceSelector.parentElement).toBe(clientSelector.parentElement);
  expect(clientSelector.compareDocumentPosition(promptInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(workspaceSelector.compareDocumentPosition(submit) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test('shows new chat keyboard hint and submits with Shift Enter while preserving Enter for newlines', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-enter' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-enter' }) } satisfies CreateSessionResult);
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(screen.getByText('Shift+Enter / Ctrl+Enter to send · Enter for newline')).toBeInTheDocument();

  await user.type(promptInput, 'Line one');
  expect(await fireEvent.keyDown(promptInput, { key: 'Enter' })).toBe(true);
  expect(mocks.createSession).not.toHaveBeenCalled();

  expect(await fireEvent.keyDown(promptInput, { key: 'Enter', shiftKey: true })).toBe(false);
  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({
    initial_task: { input: 'Line one', metadata: { source: 'dashboard_chat' } },
  })));
});

test('new chat submits with Ctrl Enter', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-enter' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-enter' }) } satisfies CreateSessionResult);
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  await user.type(promptInput, 'Line two');

  expect(await fireEvent.keyDown(promptInput, { key: 'Enter', ctrlKey: true })).toBe(false);
  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({
    initial_task: { input: 'Line two', metadata: { source: 'dashboard_chat' } },
  })));
});


test('creates a session with initial prompt, workspace, and client then opens its chat', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-new' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-new' }) } satisfies CreateSessionResult);
  render(ChatPage);

  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Implement the dashboard chat flow');
  await fireEvent.click(screen.getByRole('button', { name: /start chat/i }));

  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith({
    client_type: 'pi',
    workspace_id: 'workspace-1',
    handle: null,
    role: null,
    title: 'Implement the dashboard chat flow',
    description: null,
    initial_task: { input: 'Implement the dashboard chat flow', metadata: { source: 'dashboard_chat' } },
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new');
});

test('shows busy agent status without the removed interrupt placeholder control', async () => {
  const busySession = session({ state: 'busy', current_turn_id: 'turn-1', capabilities: { interrupt: true } });
  mocks.loadedSessions = [busySession];
  mocks.sessions.set([busySession]);
  mocks.sessionDetail.set({ session: busySession, turns: [turn({ state: 'running', output: null, completed_at: null })], inboxMessages: [], events: [], artifacts: [] });
  mocks.pathParams = { sessionId: 'session-1' };
  window.history.pushState({}, '', '/dashboard/chat/session-1');

  render(ChatPage);

  expect(await screen.findByLabelText('Agent status: Agent working')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /interrupt agent/i })).not.toBeInTheDocument();
  expect(mocks.interruptSession).not.toHaveBeenCalled();
});

test('renames the selected chat session from advanced controls', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', title: 'Old title' });
  const renamed = session({ session_id: 'session-2', title: 'New title' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.updateSessionTitle.mockResolvedValue(renamed);
  render(ChatPage);

  await fireEvent.click(await screen.findByRole('button', { name: /advanced session controls/i }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: /rename session/i }));
  const titleInput = await screen.findByLabelText(/session title/i);
  await user.clear(titleInput);
  await user.type(titleInput, 'New title');
  await user.click(screen.getByRole('button', { name: /rename session/i }));

  await waitFor(() => expect(mocks.updateSessionTitle).toHaveBeenCalledWith('session-2', 'New title'));
});

test('shows the initial prompt immediately after starting a chat while timeline is empty', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-new', state: 'busy', current_turn_id: 'turn-new' });
  const initialTurn = turn({
    turn_id: 'turn-new',
    session_id: 'session-new',
    state: 'running',
    input: { summary: 'hi' },
    output: null,
    completed_at: null,
  });
  mocks.createSession.mockImplementation(async () => {
    mocks.sessions.set([created]);
    mocks.sessionDetail.set({ session: created, turns: [initialTurn], inboxMessages: [], events: [], artifacts: [] });
    return { session: created, initial_turn: initialTurn } satisfies CreateSessionResult;
  });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set({
      sessionId,
      bindingId: null,
      items: [],
      headCursor: null,
    tailCursor: null,
            sourceId: null,
      hasMore: false,
            loading: false,
      refreshing: false,
      error: null,
    });
    return null;
  });
  render(ChatPage);

  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'hi');
  await fireEvent.click(screen.getByRole('button', { name: /start chat/i }));

  await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new'));
  expect(await screen.findByText('hi')).toBeInTheDocument();
  expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();

  cleanup();
  mocks.pathParams = { sessionId: 'session-new' };
  render(ChatPage);

  expect(await screen.findByText('hi')).toBeInTheDocument();
  expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();
});

test('shows workspace name in the selected chat composer pill while retaining full path metadata', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle', workspace_id: 'workspace-1', workspace: '/repo/pontia' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.workspaces.set([workspace({ workspace_id: 'workspace-1', name: 'Pontia Workspace', canonical_path: '/repo/pontia', display_path: '~/repo/pontia' })]);

  render(ChatPage);

  const sessionDetailsButton = (await screen.findByText('Pontia Workspace')).closest('button');
  expect(sessionDetailsButton).not.toBeNull();
  expect(sessionDetailsButton).toHaveTextContent('Pontia Workspace');
  expect(sessionDetailsButton).not.toHaveTextContent('/repo/pontia');

  await user.click(sessionDetailsButton as HTMLButtonElement);
  const workspacePill = screen.getByLabelText('Workspace: /repo/pontia');
  expect(workspacePill).toHaveTextContent('Pontia Workspace');
});

test('loads earlier chat history when the chat scroll reaches the top', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    headCursor: 'older-cursor',
    tailCursor: 'tail-cursor',
        sourceId: 'source-1',
    hasMore: true,
        loading: false,
    refreshing: false,
    error: null,
  });

  render(ChatPage);

  expect(screen.queryByRole('button', { name: /load earlier messages/i })).not.toBeInTheDocument();

  Object.defineProperty(window, 'scrollY', { configurable: true, value: 40 });
  window.dispatchEvent(new Event('scroll'));

  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-2', { mode: 'more' }));
});

test('refreshes an already-loaded selected chat through the tail cursor without rebuilding loaded history', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-older', session_id: 'session-2', input: { summary: 'older question' }, output: { summary: 'older answer' } }),
      turn({ turn_id: 'turn-latest', session_id: 'session-2', input: { summary: 'latest question' }, output: { summary: 'latest answer' } }),
    ]),
    headCursor: 'older-cursor',
    tailCursor: 'tail-cursor',
        sourceId: 'source-1',
    hasMore: true,
        loading: false,
    refreshing: false,
    error: null,
  });

  render(ChatPage);

  await waitFor(() => expect(mocks.handleTimelineMessageUpdated).toHaveBeenCalledWith('session-2'));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalledWith('session-2', { mode: 'rebuild' });
  expect(mocks.resetTimelineState).not.toHaveBeenCalledWith('session-2');
  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  mocks.loadSessionDetail.mockClear();
  mocks.loadSessionTimeline.mockClear();
  mocks.handleTimelineMessageUpdated.mockClear();
  await new Promise((resolve) => setTimeout(resolve, 0));
  mocks.loadSessionTimeline.mockClear();
  mocks.handleTimelineMessageUpdated.mockClear();
  mocks.timelineState.set({
    ...mocks.timelineState.get(),
    sessionId: 'session-2',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-older', session_id: 'session-2', input: { summary: 'older question' }, output: { summary: 'older answer' } }),
      turn({ turn_id: 'turn-latest', session_id: 'session-2', input: { summary: 'latest question' }, output: { summary: 'latest answer' } }),
    ]),
  });

  window.dispatchEvent(new Event('focus'));

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2', { showLoading: false }));
  expect(mocks.handleTimelineMessageUpdated).toHaveBeenCalledWith('session-2');
});

test('does not show the earlier-history loading row for foreground tail refreshes', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    headCursor: 'older-cursor',
    tailCursor: 'tail-cursor',
        sourceId: 'source-1',
    hasMore: true,
        loading: false,
    refreshing: true,
    refreshKind: 'tail',
    error: null,
  });

  render(ChatPage);

  expect(await screen.findByText('hi there')).toBeInTheDocument();
  expect(screen.queryByText('Loading earlier messages…')).not.toBeInTheDocument();
});

test('coalesces bursty selected-session idle events into one git status refresh', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle', workspace_id: 'workspace-1' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  mocks.refreshWorkspaceGitStatus.mockClear();

  const idleEvent = (eventId: string, type: string) => ({
    kind: 'session_event' as const,
    id: eventId,
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: eventId,
      session_id: 'session-2',
      turn_id: null,
      source: 'runtime',
      type,
      time: '2026-05-14T00:00:00Z',
      payload: {},
    },
  });

  for (const listener of mocks.dashboardEventListeners) {
    listener(idleEvent('evt-ready', 'session.ready'));
    listener(idleEvent('evt-completed', 'turn.completed'));
    listener(idleEvent('evt-failed', 'turn.failed'));
  }

  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledTimes(1));
  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1');
});

test('toasts passive fetch errors from automatic chat refreshes', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    headCursor: null,
    tailCursor: null,
        sourceId: 'source-1',
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });

  render(ChatPage);

  await waitFor(() => expect(mocks.handleTimelineMessageUpdated).toHaveBeenCalledWith('session-2'));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalled();
  mocks.toastError.mockClear();

  mocks.sessionDetailError.set('fetch error');
  mocks.timelineState.set({ ...mocks.timelineState.get(), error: 'Failed to fetch' });
  mocks.sessionsError.set('NetworkError when attempting to fetch resource.');

  await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith('Chat error', { description: 'fetch error' }));
});

test('lets existing chat routes use document scroll with a fixed bottom composer', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  const selectedTurns = [turn({ session_id: 'session-2' })];
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: selectedTurns, inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns(selectedTurns),
    headCursor: null,
    tailCursor: null,
        sourceId: null,
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });

  const { container } = render(ChatPage);

  const composerInput = await screen.findByPlaceholderText('Send a follow-up message…');
  expect(composerInput).toHaveClass('h-10');
  expect(composerInput).toHaveClass('min-h-10');
  expect(composerInput).toHaveClass('md:min-h-20');
  const pageSection = container.querySelector('section');
  expect(pageSection).not.toHaveClass('h-full');
  expect(pageSection).not.toHaveClass('min-h-0');
  expect(pageSection).toHaveClass('pb-40');
  const conversationContent = container.querySelector('[data-chat-conversation-content]');
  expect(conversationContent).not.toBeNull();
  expect(conversationContent).toHaveClass('px-0');
  expect(conversationContent).toHaveClass('py-4');
  expect(conversationContent).toHaveClass('sm:p-4');
  const composerDock = container.querySelector('[data-chat-composer-dock="fixed"]');
  expect(composerDock).not.toBeNull();
  expect(composerDock).toHaveClass('fixed');
  expect(composerDock).toHaveClass('bottom-0');
  expect(composerDock?.firstElementChild).toHaveClass('mx-auto');
  expect(composerDock?.firstElementChild).toHaveClass('max-w-7xl');

  const stateBadge = screen.getByLabelText('Session state: idle');
  expect(stateBadge.querySelector('[data-chat-session-state-label]')).toBeNull();
  expect(stateBadge).not.toHaveTextContent('idle');

  const sessionDetailsButton = screen.getByRole('button', { name: 'Session details: pontia · pi · main' });
  expect(sessionDetailsButton).toHaveTextContent('pontia');
  expect(sessionDetailsButton).toHaveTextContent('pi');
  expect(sessionDetailsButton).toHaveTextContent('main');
  expect(sessionDetailsButton).not.toHaveTextContent('+2');
  expect(sessionDetailsButton).toHaveClass('bg-transparent');
  expect(sessionDetailsButton).toHaveClass('px-0');
  expect(sessionDetailsButton).toHaveClass('hover:bg-transparent');
  expect(sessionDetailsButton).not.toHaveAttribute('data-slot', 'button');
  expect(sessionDetailsButton.className.split(/\s+/)).not.toContain('border');
  expect(sessionDetailsButton.className).not.toContain('focus-visible:border-ring');
  expect(sessionDetailsButton.className).not.toContain('border-border');
  expect(sessionDetailsButton.className).not.toContain('dark:border-input');
  expect(sessionDetailsButton.querySelector('[data-chat-session-details-summary]')).toHaveClass('flex-1');
});

test('desktop composer resize button expands follow-up input height in place', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  const composerInput = await screen.findByPlaceholderText('Send a follow-up message…');
  expect(composerInput).toHaveClass('md:min-h-20');
  expect(composerInput).not.toHaveClass('md:min-h-56');

  await fireEvent.click(screen.getByRole('button', { name: 'Expand message composer' }));

  expect(composerInput).toHaveClass('md:min-h-56');
  expect(screen.queryByRole('dialog', { name: 'Expanded message composer' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Collapse message composer' })).toBeInTheDocument();
});

test('mobile composer resize button opens a fullscreen follow-up composer sharing the current input', async () => {
  const originalMatchMedia = window.matchMedia;
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: query.includes('max-width'),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  try {
    render(ChatPage);

    const composerInput = await screen.findByPlaceholderText('Send a follow-up message…');
    await user.type(composerInput, 'mobile draft');
    await fireEvent.click(screen.getByRole('button', { name: 'Expand message composer' }));

    const fullscreenComposer = screen.getByRole('dialog', { name: 'Expanded message composer' });
    const fullscreenInput = within(fullscreenComposer).getByPlaceholderText('Send a follow-up message…');
    expect(fullscreenInput).toHaveValue('mobile draft');
    expect(fullscreenInput).toHaveClass('min-h-[calc(100vh-12rem)]');

    await user.type(fullscreenInput, ' plus more');
    await fireEvent.click(within(fullscreenComposer).getByRole('button', { name: /send/i }));

    await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
      input: 'mobile draft plus more',
      delivery_policy: 'after_idle',
      metadata: { source: 'dashboard_chat' },
    }));
  } finally {
    window.matchMedia = originalMatchMedia;
  }
});

test('shows idle thought summary trigger above the final assistant response', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: [
      {
        item_id: 'turn-1:user',
        kind: 'user',
        raw_kind: 'user',
        role: 'user',
        title: null,
        status: null,
        occurred_at: '2026-05-14T00:00:00Z',
        content_preview: 'hello',
        content_ref: 'turn-1:user-ref',
        turn_id: 'turn-1',
      },
      {
        item_id: 'turn-1:thinking',
        kind: 'thinking',
        raw_kind: 'thinking',
        role: 'assistant',
        title: null,
        status: null,
        occurred_at: '2026-05-14T00:00:01Z',
        content_preview: 'I should inspect the code.',
        content_ref: 'turn-1:thinking-ref',
        turn_id: 'turn-1',
      },
      {
        item_id: 'turn-1:tool',
        kind: 'tool_call',
        raw_kind: 'tool_call',
        role: 'tool',
        title: 'read',
        status: 'started',
        occurred_at: '2026-05-14T00:00:02Z',
        content_preview: 'read {"path":"src/app.ts"}',
        content_ref: 'turn-1:tool-ref',
        turn_id: 'turn-1',
      },
      {
        item_id: 'turn-1:assistant',
        kind: 'assistant',
        raw_kind: 'text',
        role: 'assistant',
        title: null,
        status: null,
        occurred_at: '2026-05-14T00:00:03Z',
        content_preview: 'Final answer',
        content_ref: 'turn-1:assistant-ref',
        turn_id: 'turn-1',
      },
    ],
    headCursor: null,
    tailCursor: null,
        sourceId: 'source-1',
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  const timelineSnapshot = mocks.timelineState.get();
  mocks.loadSessionTimeline.mockImplementationOnce(async () => {
    mocks.timelineState.set(timelineSnapshot);
    return null;
  });

  render(ChatPage);

  expect(await screen.findByText('Final answer')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /view thought details/i })).toHaveTextContent('Worked for 2 steps');
  expect(screen.queryByText('Thought for 2 steps')).not.toBeInTheDocument();
  expect(screen.queryByText('I should inspect the code.')).not.toBeInTheDocument();
  expect(screen.queryByText('read {"path":"src/app.ts"}')).not.toBeInTheDocument();
  expect(screen.queryByText('started')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('started')).not.toBeInTheDocument();
});

test('keeps whitespace preservation on message text instead of the bubble wrapper', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2', input: { summary: 'hello' }, output: { summary: 'hi there' } })], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  const userText = await screen.findByText('hello');
  const userBubble = userText.parentElement;
  expect(userText).toHaveClass('whitespace-pre-wrap');
  expect(userBubble).not.toHaveClass('whitespace-pre-wrap');
});

test('renders assistant output as markdown while leaving user prompts as plain text', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({
      session_id: 'session-2',
      input: { summary: '**literal prompt**' },
      output: { summary: '**bold output**\n\n- first item' },
    })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  const { container } = render(ChatPage);

  expect(await screen.findByText('**literal prompt**')).toBeInTheDocument();
  expect(container.querySelector('strong')?.textContent).toBe('bold output');
  expect(container.querySelector('li')?.textContent).toBe('first item');
});

test('styles assistant markdown tables', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({
      session_id: 'session-2',
      output: { summary: '| Name | Status |\n| --- | --- |\n| Markdown | Rendered |' },
    })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  const { container } = render(ChatPage);

  expect(await screen.findByText('Markdown')).toBeInTheDocument();
  expect(container.querySelector('table')).toBeInTheDocument();
  expect(container.querySelector('th')?.textContent).toBe('Name');
  expect(container.querySelector('td')?.textContent).toBe('Markdown');
  expect(
    Array.from(container.querySelectorAll('div')).some((element) => element.className.includes('[&_table]:')),
  ).toBe(true);
});

test('highlights fenced code blocks in assistant markdown', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({
      session_id: 'session-2',
      output: { summary: '```ts\nconst answer: number = 42;\n```' },
    })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  const { container } = render(ChatPage);

  expect(await screen.findByText(/answer/)).toBeInTheDocument();
  expect(container.querySelector('code.hljs.language-ts')).toBeInTheDocument();
  expect(container.querySelector('.hljs-keyword')?.textContent).toBe('const');
  const markdownContainer = Array.from(container.querySelectorAll('div')).find((element) =>
    element.className.includes('[&_pre]:'),
  );
  expect(markdownContainer?.className).not.toContain('[&_pre]:bg-muted');
  expect(markdownContainer?.className).not.toContain('[&_pre]:p-3');
  expect(markdownContainer?.className).not.toContain('[&_pre_code]:bg-transparent');
  expect(markdownContainer?.className).not.toContain('[&_pre_code]:p-0');
});

test('opens an inbox sheet with actionable pending, failed, and dispatching messages only', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [],
    inboxMessages: [
      inboxMessage({
        message_id: 'message-dispatched',
        session_id: 'session-2',
        state: 'dispatched',
        input: { summary: 'Already sent' },
      }),
      inboxMessage({
        message_id: 'message-pending-old',
        session_id: 'session-2',
        state: 'pending',
        input: { summary: 'Continue implementation' },
        updated_at: '2026-05-14T00:00:04Z',
      }),
      inboxMessage({
        message_id: 'message-failed',
        session_id: 'session-2',
        state: 'failed',
        input: { summary: 'Fix the failing dashboard test' },
        metadata: { source: 'dashboard_chat', attempt: 1 },
        delivery_policy: 'interrupt_now',
        turn_id: 'turn-2',
        failure_message: 'runtime unavailable',
        updated_at: '2026-05-14T00:00:05Z',
      }),
      inboxMessage({
        message_id: 'message-dispatching',
        session_id: 'session-2',
        state: 'dispatching',
        input: { summary: 'Sending now' },
        updated_at: '2026-05-14T00:00:06Z',
      }),
    ],
    events: [],
    artifacts: [],
  });

  const { container } = render(ChatPage);

  const inboxButton = await screen.findByRole('button', { name: /open inbox, 2 messages/i });
  expect(inboxButton).toHaveTextContent('Inbox');
  expect(inboxButton).not.toHaveTextContent('2');
  expect(inboxButton.closest('[data-chat-desktop-inbox]')).toHaveClass('hidden');
  expect(inboxButton.closest('[data-chat-desktop-inbox]')).toHaveClass('sm:block');

  const advancedButton = screen.getByRole('button', { name: /advanced session controls, 2 inbox messages/i });
  const advancedBubble = advancedButton.parentElement?.querySelector('[data-chat-mobile-inbox-count]');
  expect(advancedBubble).toHaveTextContent('2');
  expect(advancedBubble).toHaveClass('sm:hidden');

  await fireEvent.click(advancedButton);
  const mobileInboxMenuItem = await screen.findByRole('menuitem', { name: /open inbox, 2 messages/i });
  expect(mobileInboxMenuItem).toHaveClass('sm:hidden');

  await fireEvent.click(mobileInboxMenuItem);

  expect(container.querySelector('[data-chat-desktop-inbox]')).toHaveClass('hidden');

  expect(await screen.findByRole('dialog')).toBeInTheDocument();
  expect(screen.getByText('Sending now')).toBeInTheDocument();
  expect(screen.getByText('Fix the failing dashboard test')).toBeInTheDocument();
  expect(screen.getByText('Continue implementation')).toBeInTheDocument();
  expect(screen.queryByText('Already sent')).not.toBeInTheDocument();
  expect(screen.getByText('runtime unavailable')).toBeInTheDocument();

  const articles = screen.getAllByRole('article');
  expect(articles.map((article) => article.textContent)).toEqual([
    expect.stringContaining('Sending now'),
    expect.stringContaining('Fix the failing dashboard test'),
    expect.stringContaining('Continue implementation'),
  ]);
  expect(articles[0]).not.toHaveTextContent('Cancel');
  expect(articles[0]).not.toHaveTextContent('Retry');
  expect(articles[1]).toHaveTextContent('Retry');
  expect(articles[1]).not.toHaveTextContent('Cancel');
  expect(articles[2]).toHaveTextContent('Cancel');
});

test('supports cancelling pending inbox messages and retrying or removing failed inbox messages', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [],
    inboxMessages: [
      inboxMessage({
        message_id: 'message-pending',
        session_id: 'session-2',
        state: 'pending',
        input: { summary: 'Continue implementation' },
      }),
      inboxMessage({
        message_id: 'message-failed',
        session_id: 'session-2',
        state: 'failed',
        input: { summary: 'Fix the failing dashboard test' },
        metadata: { source: 'dashboard_chat', attempt: 1 },
        delivery_policy: 'interrupt_now',
      }),
    ],
    events: [],
    artifacts: [],
  });

  render(ChatPage);
  await userEvent.click(await screen.findByRole('button', { name: /open inbox, 2 messages/i }));

  await userEvent.click(await screen.findByRole('button', { name: /cancel inbox message continue implementation/i }));
  expect(mocks.cancelInboxMessage).toHaveBeenCalledWith('session-2', 'message-pending');

  await userEvent.click(await screen.findByRole('button', { name: /retry inbox message fix the failing dashboard test/i }));
  expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'Fix the failing dashboard test',
    delivery_policy: 'interrupt_now',
    metadata: { source: 'dashboard_chat', attempt: 1 },
  });

  await userEvent.click(await screen.findByRole('button', { name: /remove inbox message fix the failing dashboard test/i }));
  expect(mocks.dismissInboxMessage).toHaveBeenCalledWith('session-2', 'message-failed');
});

test('opens an empty inbox sheet when the selected chat has no inbox messages', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  const inboxButton = await screen.findByRole('button', { name: /open inbox, 0 messages/i });
  await userEvent.click(inboxButton);

  expect(await screen.findByRole('dialog')).toBeInTheDocument();
  expect(screen.getByText('No inbox messages')).toBeInTheDocument();
  expect(screen.getByText('Follow-up messages submitted from this chat will appear here.')).toBeInTheDocument();
});

test('loads and renders an existing chat session with metadata, state, and workspace name above the prompt input without a page header', async () => {
  const selected = session({
    session_id: 'session-2',
    client_type: 'claude-code',
    handle: 'second',
    role: 'reviewer',
    description: 'Review dashboard changes',
    execution_profile_id: 'coder',
    execution_profile_version: '1',
    state: 'busy',
    workspace_id: 'workspace-1',
    workspace: '~/repo/pontia',
  });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.workspaces.set([workspace({ workspace_id: 'workspace-1', name: 'pontia', canonical_path: '/repo/pontia', display_path: '~/repo/pontia' })]);

  render(ChatPage);

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2'));
  expect(await screen.findByText('hi there')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /second · reviewer/i })).not.toBeInTheDocument();
  expect(screen.queryByText('Description: Review dashboard changes')).not.toBeInTheDocument();
  const sessionDetailsButton = screen.getByRole('button', { name: /Session details: pontia · claude-code · coder@1 · second/i });
  await userEvent.click(sessionDetailsButton);
  const clientBadge = screen.getAllByLabelText('Client: claude-code')[0];
  const profileBadge = screen.getAllByLabelText('Profile: coder@1')[0];
  expect(clientBadge).toBeInTheDocument();
  expect(profileBadge).toBeInTheDocument();
  expect(screen.getAllByLabelText('Handle: second')[0]).toBeInTheDocument();
  expect(within(clientBadge.closest('div') as HTMLElement).getByLabelText('Client')).toHaveClass('lucide-terminal');
  expect(within(profileBadge.closest('div') as HTMLElement).getByLabelText('Profile')).toHaveClass('lucide-bot');
  expect(screen.queryByText('Client: claude-code')).not.toBeInTheDocument();
  expect(screen.queryByText('Profile: coder@1')).not.toBeInTheDocument();
  expect(screen.queryByText('Handle: second')).not.toBeInTheDocument();
  expect(screen.queryByText('Workspace: workspace-1')).not.toBeInTheDocument();
  const stateBadge = screen.getByLabelText('Session state: busy');
  const workspaceBadge = screen.getAllByLabelText('Workspace: /repo/pontia')[0];
  const workspaceName = within(workspaceBadge).getByText('pontia');
  const clientDetail = screen.getAllByLabelText('Client: claude-code')[0];
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  expect(screen.queryByText('State: busy')).not.toBeInTheDocument();
  expect(stateBadge).not.toHaveTextContent('busy');
  expect(stateBadge).toHaveClass('h-7');
  expect(stateBadge.querySelector('svg')).toHaveClass('lucide-loader');
  expect(workspaceBadge).toContainElement(workspaceName);
  expect(stateBadge?.compareDocumentPosition(workspaceBadge) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(workspaceBadge.compareDocumentPosition(clientDetail) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(clientDetail.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByRole('button', { name: /new chat/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /new chat/i })).not.toBeInTheDocument();
});

test('shows supported context usage in chat session metadata while hiding unsupported usage', async () => {
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
  window.history.pushState({}, '', '/dashboard/chat/session-usage');
  mocks.pathParams = { sessionId: 'session-usage' };
  mocks.loadedSessions = [withUsage];
  mocks.sessions.set([withUsage]);
  mocks.sessionDetail.set({ session: withUsage, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  const contextBadge = await screen.findByRole('button', { name: /Session details: .*33% · 42k \/ 128k/i });
  expect(contextBadge).toBeInTheDocument();
  expect(contextBadge.querySelector('.lucide-gauge')).toBeInTheDocument();
  expect(screen.getAllByText('33% · 42k / 128k')[0]).toBeInTheDocument();
  expect(screen.queryByText('Context 33% · 42k / 128k')).not.toBeInTheDocument();

  cleanup();
  const unsupported = session({ session_id: 'session-unsupported', capabilities: { context_usage: 'unsupported' }, context_usage: null });
  window.history.pushState({}, '', '/dashboard/chat/session-unsupported');
  mocks.pathParams = { sessionId: 'session-unsupported' };
  mocks.loadedSessions = [unsupported];
  mocks.sessions.set([unsupported]);
  mocks.sessionDetail.set({ session: unsupported, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  expect(screen.queryByText(/context/i)).not.toBeInTheDocument();
});

test('places session controls near the prompt input and keeps advanced controls in a menu', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  const exitButton = screen.getByRole('button', { name: /exit session/i });
  const advancedButton = screen.getByRole('button', { name: /advanced session controls/i });
  expect(exitButton.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(advancedButton.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByRole('button', { name: /resume session/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /restart session/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /session console/i })).not.toBeInTheDocument();

  await fireEvent.click(advancedButton);
  await fireEvent.click(await screen.findByRole('menuitem', { name: /restart session/i }));
  expect(mocks.restartSession).toHaveBeenCalledWith('session-2');

  await fireEvent.click(advancedButton);
  await fireEvent.click(await screen.findByRole('menuitem', { name: /session console/i }));
  expect(mocks.navigate).toHaveBeenCalledWith('/sessions/session-2');

  await fireEvent.click(exitButton);
  expect(mocks.terminateSession).toHaveBeenCalledWith('session-2');
});

test('disables follow-up input for sessions that do not advertise web-write capability while keeping output visible', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { accept_task: false, stream_output: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({ session_id: 'session-2', input: { summary: 'tui input' }, output: { summary: 'tui output' } })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  render(ChatPage);

  expect(await screen.findByText('tui output')).toBeInTheDocument();
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  expect(followUpInput).toBeDisabled();
  expect(screen.getByText('此 session 当前不可从 Web 写入')).toBeInTheDocument();

  await user.type(followUpInput, 'should not send');
  await user.click(screen.getByRole('button', { name: /send/i }));

  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});

test('queues follow-up messages without rendering inline success chrome', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(ChatPage);

  await user.type(await screen.findByPlaceholderText('Send a follow-up message…'), 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalled());
  expect(screen.queryByText('Chat updated')).not.toBeInTheDocument();
  expect(screen.queryByText('Message queued for the selected session.')).not.toBeInTheDocument();
});

test('follow-up composer submits with Shift Enter while preserving Enter for newlines', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(ChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  expect(screen.getByText('Shift+Enter / Ctrl+Enter to send · Enter for newline')).toBeInTheDocument();

  await user.type(followUpInput, 'continue this session');
  expect(await fireEvent.keyDown(followUpInput, { key: 'Enter' })).toBe(true);
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  expect(await fireEvent.keyDown(followUpInput, { key: 'Enter', shiftKey: true })).toBe(false);
  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'continue this session',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
});

test('follow-up composer submits with Ctrl Enter', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(ChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  await user.type(followUpInput, 'another message');

  expect(await fireEvent.keyDown(followUpInput, { key: 'Enter', ctrlKey: true })).toBe(false);
  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'another message',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
});

test('does not render inline chat error alerts', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.sessionDetailError.set('Could not load session detail');

  render(ChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith('Chat error', { description: 'Could not load session detail' }));
  expect(screen.queryByText('Chat error')).not.toBeInTheDocument();
  expect(screen.queryByText('Could not load session detail')).not.toBeInTheDocument();
});

test('hides exit on exited sessions and waits for idle after automatic resume before sending a message', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'exited' });
  const starting = session({ session_id: 'session-2', state: 'starting' });
  const idle = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.resumeSession.mockImplementation(async () => {
    mocks.sessions.set([starting]);
    mocks.sessionDetail.set({ session: starting, turns: [], inboxMessages: [], events: [], artifacts: [] });
  });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(ChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  expect(followUpInput).not.toBeDisabled();
  expect(screen.queryByRole('button', { name: /exit session/i })).not.toBeInTheDocument();

  await user.type(followUpInput, 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));

  await waitFor(() => expect(mocks.resumeSession).toHaveBeenCalledWith('session-2'));
  expect(followUpInput).toHaveValue('continue this session');
  expect(followUpInput).toBeDisabled();
  expect(screen.getByRole('button', { name: /send/i })).toBeDisabled();
  await Promise.resolve();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  mocks.sessions.set([idle]);
  mocks.sessionDetail.set({ session: idle, turns: [], inboxMessages: [], events: [], artifacts: [] });

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'continue this session',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.resumeSession.mock.invocationCallOrder[0]).toBeLessThan(mocks.submitInboxMessage.mock.invocationCallOrder[0]);
});

test('hides planner draft DAG entry points in planner chat sessions', async () => {
  const planner = session({
    session_id: 'session-planner',
    execution_profile_id: 'planner',
    metadata: { dag_managed: true, dag_planning_role: 'planner', task_id: 'task-new' },
  });
  window.history.pushState({}, '', '/dashboard/chat/session-planner');
  mocks.pathParams = { sessionId: 'session-planner' };
  mocks.loadedSessions = [planner];
  mocks.sessions.set([planner]);
  mocks.sessionDetail.set({
    session: planner,
    turns: [turn({ session_id: 'session-planner', input: { summary: 'Please plan this' }, output: { summary: 'I drafted a DAG below.' } })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  render(ChatPage);

  expect(await screen.findByText('I drafted a DAG below.')).toBeInTheDocument();
  expect(mocks.loadTaskProposals).not.toHaveBeenCalled();
  expect(screen.queryByRole('button', { name: /view draft dag for turn/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /planner draft dag/i })).not.toBeInTheDocument();
});

test('does not navigate from planner chat to the task DAG when SSE reports approval', async () => {
  const planner = session({
    session_id: 'session-planner',
    execution_profile_id: 'planner',
    metadata: { dag_managed: true, dag_planning_role: 'planner', task_id: 'task-new' },
  });
  window.history.pushState({}, '', '/dashboard/chat/session-planner');
  mocks.pathParams = { sessionId: 'session-planner' };
  mocks.loadedSessions = [planner];
  mocks.sessions.set([planner]);
  mocks.sessionDetail.set({ session: planner, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  for (const listener of mocks.dashboardEventListeners) {
    listener({
      kind: 'task_event',
      id: 'evt-1',
      occurred_at: '2026-05-14T00:00:00Z',
      event: {
        event_id: 'evt-1',
        task_id: 'task-new',
        event_type: 'dag.approved',
        payload: { proposal_id: 'proposal-1' },
        created_at: '2026-05-14T00:00:00Z',
      },
    });
  }

  expect(mocks.navigate).not.toHaveBeenCalledWith('/tasks/task-new/dag');
});

test('does not open the task DAG immediately when the planner proposal was already applied', async () => {
  const planner = session({
    session_id: 'session-planner',
    execution_profile_id: 'planner',
    metadata: { dag_managed: true, dag_planning_role: 'planner', task_id: 'task-new' },
  });
  window.history.pushState({}, '', '/dashboard/chat/session-planner');
  mocks.pathParams = { sessionId: 'session-planner' };
  mocks.loadedSessions = [planner];
  mocks.sessions.set([planner]);
  mocks.sessionDetail.set({ session: planner, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.taskProposals.set([
    {
      proposal_id: 'proposal-1',
      task_id: 'task-new',
      mode: 'initial_dag',
      state: 'applied',
      summary: 'Applied plan',
      proposal_json: { mode: 'initial_dag', summary: 'Applied plan', work_items: [], edges: [] },
      validation_json: {},
      created_by_session_id: 'session-planner',
      revision: 1,
      supersedes_proposal_id: null,
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T00:00:00Z',
    },
  ]);

  render(ChatPage);

  await waitFor(() => expect(screen.getByRole('log')).toBeInTheDocument());
  expect(mocks.loadTaskProposals).not.toHaveBeenCalled();
  expect(mocks.navigate).not.toHaveBeenCalledWith('/tasks/task-new/dag');
});
