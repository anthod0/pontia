import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, test, vi } from 'vitest';
import ChatPage from '../../src/pages/ChatPage.svelte';
import type { SessionConsoleDetail } from '../../src/stores/sessions';
import type { AgentProfileView, CreateDagTaskResult, CreateSessionResult, SessionView, TurnView, WorkspaceView } from '../../src/api/types';

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
  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesError = writableStore<string | null>(null);
  const agentProfiles = writableStore<AgentProfileView[]>([]);
  const agentProfilesLoading = writableStore(false);
  const agentProfilesError = writableStore<string | null>(null);
  const taskProposals = writableStore<unknown[]>([]);
  const taskProposalsLoading = writableStore(false);
  const taskProposalsError = writableStore<string | null>(null);
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
    agentProfiles,
    agentProfilesLoading,
    agentProfilesError,
    taskProposals,
    taskProposalsLoading,
    taskProposalsError,
    dashboardEventListeners,
    loadedSessions: [] as SessionView[],
    loadSessions: vi.fn(async () => mocks.loadedSessions),
    loadSessionDetail: vi.fn(async () => null),
    submitInboxMessage: vi.fn(),
    resumeSession: vi.fn(),
    restartSession: vi.fn(),
    terminateSession: vi.fn(),
    createSession: vi.fn(),
    createDagTask: vi.fn(),
    loadTaskProposals: vi.fn(async () => []),
    loadWorkspaces: vi.fn(async () => undefined),
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
  resumeSession: mocks.resumeSession,
  restartSession: mocks.restartSession,
  terminateSession: mocks.terminateSession,
  createSession: mocks.createSession,
}));

vi.mock('../../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  loadWorkspaces: mocks.loadWorkspaces,
}));

vi.mock('../../src/stores/tasks', () => ({
  createDagTask: mocks.createDagTask,
  taskProposals: mocks.taskProposals,
  taskProposalsLoading: mocks.taskProposalsLoading,
  taskProposalsError: mocks.taskProposalsError,
  loadTaskProposals: mocks.loadTaskProposals,
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

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: 'workspace-1',
  canonical_path: '/repo/llmparty',
  display_path: '~/repo/llmparty',
  name: 'llmparty',
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
  mocks.agentProfiles.set([profile()]);
  mocks.agentProfilesLoading.set(false);
  mocks.agentProfilesError.set(null);
  mocks.taskProposals.set([]);
  mocks.taskProposalsLoading.set(false);
  mocks.taskProposalsError.set(null);
  mocks.dashboardEventListeners.clear();
  mocks.pathParams = {};
  mocks.createSession.mockResolvedValue({ session: activeSession, initial_turn: null } satisfies CreateSessionResult);
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
  vi.clearAllMocks();
});

test('renders a clean centered prompt input on the bare chat route instead of selecting an existing session', async () => {
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(promptInput).toHaveValue('');
  expect(screen.getByRole('heading', { name: /new chat/i })).toBeInTheDocument();
  expect(screen.getByText('Start a new agent session from a prompt, workspace, client, and profile.')).toBeInTheDocument();
  expect(screen.queryByText(/Enter the first prompt/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/^Prompt$/i)).not.toBeInTheDocument();
  expect(screen.getByLabelText(/workspace/i)).toHaveTextContent('llmparty');
  expect(screen.getByLabelText(/client/i)).toHaveTextContent('pi');
  expect(screen.getByLabelText(/profile/i)).toHaveTextContent('Profile');
  expect(mocks.loadSessionDetail).not.toHaveBeenCalled();
});

test('places new chat selectors above the prompt input', async () => {
  render(ChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  const workspaceSelector = screen.getByLabelText(/workspace/i);
  const profileSelector = screen.getByLabelText(/profile/i);
  const clientSelector = screen.getByLabelText(/client/i);

  expect(workspaceSelector.compareDocumentPosition(promptInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(profileSelector.compareDocumentPosition(promptInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(clientSelector.compareDocumentPosition(promptInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test('places task mode toggle at the left edge of the prompt toolbar', async () => {
  render(ChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  const taskToggle = screen.getByRole('button', { name: /task mode off/i });
  const submit = screen.getByRole('button', { name: /start chat/i });
  const toolbar = taskToggle.parentElement;

  expect(toolbar).toHaveClass('justify-between');
  expect(taskToggle.compareDocumentPosition(submit) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test('creates a manual DAG task from task mode and opens the planner session chat', async () => {
  const user = userEvent.setup();
  render(ChatPage);

  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Plan this as a DAG task');
  await fireEvent.click(screen.getByRole('button', { name: /task mode off/i }));
  expect(screen.getByRole('button', { name: /task mode on/i })).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button', { name: /create task/i }));

  await waitFor(() => expect(mocks.createDagTask).toHaveBeenCalledWith({
    input: 'Plan this as a DAG task',
    workspace: '/repo/llmparty',
    client_type: 'pi',
    metadata: { source: 'dashboard_chat', action: 'manual_task' },
  }));
  expect(mocks.createSession).not.toHaveBeenCalled();
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-planner');
  expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-planner');
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
    description: null,
    initial_task: { input: 'Implement the dashboard chat flow', metadata: { source: 'dashboard_chat' } },
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new');
});

test('keeps an existing chat route constrained to viewport height so only the conversation scrolls', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });

  const { container } = render(ChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  const pageSection = container.querySelector('section');
  expect(pageSection).toHaveClass('h-full');
  expect(pageSection).toHaveClass('min-h-0');
  expect(pageSection).not.toHaveClass('h-[calc(100vh-5rem)]');
  expect(pageSection).not.toHaveClass('min-h-[42rem]');
});

test('loads and renders an existing chat session with metadata, state, and workspace path above the prompt input without a page header', async () => {
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
    workspace: '~/repo/llmparty',
  });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });

  render(ChatPage);

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2'));
  expect(await screen.findByText('hi there')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /second · reviewer/i })).not.toBeInTheDocument();
  expect(screen.queryByText('Description: Review dashboard changes')).not.toBeInTheDocument();
  const clientBadge = screen.getByLabelText('Client: claude-code');
  const profileBadge = screen.getByLabelText('Profile: coder@1');
  expect(clientBadge).toBeInTheDocument();
  expect(profileBadge).toBeInTheDocument();
  expect(screen.getByLabelText('Handle: second')).toBeInTheDocument();
  expect(clientBadge.querySelector('svg')).toHaveClass('lucide-terminal');
  expect(profileBadge.querySelector('svg')).toHaveClass('lucide-bot');
  expect(screen.queryByText('Client: claude-code')).not.toBeInTheDocument();
  expect(screen.queryByText('Profile: coder@1')).not.toBeInTheDocument();
  expect(screen.queryByText('Handle: second')).not.toBeInTheDocument();
  expect(screen.queryByText('Workspace: workspace-1')).not.toBeInTheDocument();
  const stateBadge = screen.getByText('busy').closest('[data-slot="badge"]');
  const workspacePath = screen.getByText('~/repo/llmparty');
  const clientDetail = screen.getByLabelText('Client: claude-code');
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  expect(screen.queryByText('State: busy')).not.toBeInTheDocument();
  expect(stateBadge).not.toBeNull();
  expect(stateBadge).toHaveClass('h-7');
  expect(stateBadge?.querySelector('svg')).toHaveClass('lucide-activity');
  expect(stateBadge?.compareDocumentPosition(workspacePath) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(workspacePath.compareDocumentPosition(clientDetail) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(clientDetail.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByRole('button', { name: /new chat/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /new chat/i })).not.toBeInTheDocument();
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

  await user.click(advancedButton);
  await user.click(await screen.findByRole('menuitem', { name: /restart session/i }));
  expect(mocks.restartSession).toHaveBeenCalledWith('session-2');

  await user.click(advancedButton);
  await user.click(await screen.findByRole('menuitem', { name: /session console/i }));
  expect(mocks.navigate).toHaveBeenCalledWith('/sessions/session-2');

  await user.click(exitButton);
  expect(mocks.terminateSession).toHaveBeenCalledWith('session-2');
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

test('loads planner task proposals from session metadata and renders the draft DAG', async () => {
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
      state: 'proposed',
      summary: 'Implement in two steps',
      proposal_json: {
        mode: 'initial_dag',
        summary: 'Implement in two steps',
        work_items: [
          { temp_id: 'draft-a', title: 'Design UI', description: 'Sketch chat planner UI', kind: 'implementation', action: 'Edit dashboard', execution_profile_id: 'coder', priority: 0, optional: false, parallelizable: true, acceptance_criteria: [] },
          { temp_id: 'draft-b', title: 'Wire events', description: 'Use SSE to navigate', kind: 'implementation', action: 'Subscribe to events', execution_profile_id: 'coder', priority: 1, optional: false, parallelizable: false, acceptance_criteria: [] },
        ],
        edges: [{ from_work_item_id: 'draft-a', to_work_item_id: 'draft-b', edge_type: 'depends_on' }],
      },
      validation_json: {},
      created_by_session_id: 'session-planner',
      revision: 1,
      supersedes_proposal_id: null,
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T00:00:00Z',
    },
  ]);

  render(ChatPage);

  await waitFor(() => expect(mocks.loadTaskProposals).toHaveBeenCalledWith('task-new'));
  expect(await screen.findByRole('heading', { name: /planner draft dag/i })).toBeInTheDocument();
  expect(screen.getByText('Implement in two steps')).toBeInTheDocument();
  expect(screen.getByText('Design UI')).toBeInTheDocument();
  expect(screen.getByText('Wire events')).toBeInTheDocument();
  expect(screen.getByText(/draft-a → draft-b/)).toBeInTheDocument();
});

test('navigates from planner chat to the task DAG when SSE reports approval', async () => {
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

  expect(mocks.navigate).toHaveBeenCalledWith('/tasks/task-new/dag');
});

test('opens the task DAG immediately when the planner proposal was already applied', async () => {
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

  await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/tasks/task-new/dag'));
});
