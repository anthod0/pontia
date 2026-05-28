import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, test, vi } from 'vitest';
import ChatPage from '../../src/pages/ChatPage.svelte';
import type { SessionConsoleDetail } from '../../src/stores/sessions';
import type { AgentProfileView, CreateSessionResult, SessionView, TurnView, WorkspaceView } from '../../src/api/types';

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
    loadedSessions: [] as SessionView[],
    loadSessions: vi.fn(async () => mocks.loadedSessions),
    loadSessionDetail: vi.fn(async () => null),
    submitInboxMessage: vi.fn(),
    createSession: vi.fn(),
    loadWorkspaces: vi.fn(async () => undefined),
    loadAgentProfiles: vi.fn(async () => undefined),
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
  createSession: mocks.createSession,
}));

vi.mock('../../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  loadWorkspaces: mocks.loadWorkspaces,
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
  mocks.pathParams = {};
  mocks.createSession.mockResolvedValue({ session: activeSession, initial_turn: null } satisfies CreateSessionResult);
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

test('loads and renders an existing chat session with session metadata in the page header', async () => {
  const selected = session({
    session_id: 'session-2',
    client_type: 'claude-code',
    handle: 'second',
    role: 'reviewer',
    description: 'Review dashboard changes',
    execution_profile_id: 'coder',
    execution_profile_version: '1',
    state: 'running',
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
  expect(screen.getByRole('heading', { name: /second · reviewer/i })).toBeInTheDocument();
  expect(screen.getByText('Client: claude-code')).toBeInTheDocument();
  expect(screen.getByText('Profile: coder@1')).toBeInTheDocument();
  expect(screen.getByText('Handle: second')).toBeInTheDocument();
  expect(screen.getByText('Description: Review dashboard changes')).toBeInTheDocument();
  expect(screen.getByText('State: running')).toBeInTheDocument();
  expect(screen.getByText('Workspace: workspace-1')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /new chat/i }).querySelector('svg')).toHaveClass('lucide-square-pen');
  expect(screen.queryByRole('heading', { name: /new chat/i })).not.toBeInTheDocument();
});
