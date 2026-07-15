import { fireEvent, render, screen, within, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, test, vi } from 'vitest';
import WorkspacePage from '../../src/pages/WorkspacePage.svelte';
import { routerConf } from '../../src/routes';
import type { CreateSessionResult, SessionView, TurnView, WorkspaceView } from '../../src/api/types';

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
      update(updater: (value: T) => T) {
        value = updater(value);
        for (const run of subscribers) run(value);
      },
    };
  }

  const workspaces = writableStore<WorkspaceView[]>([]);
  const sessions = writableStore<SessionView[]>([]);

  return {
    pathParams: { workspaceId: 'workspace-1' } as Record<string, string>,
    navigate: vi.fn(),
    workspaces,
    workspacesLoading: writableStore(false),
    workspacesError: writableStore<string | null>(null),
    sessions,
    sessionsLoading: writableStore(false),
    sessionsError: writableStore<string | null>(null),
    loadWorkspaces: vi.fn(async () => undefined),
    loadSessions: vi.fn(async () => [] as SessionView[]),
    createSession: vi.fn(),
    rememberOptimisticInitialMessage: vi.fn(),
    clearChatDraft: vi.fn(),
    resetTimelineState: vi.fn(),
    loadSessionDetail: vi.fn(async () => null),
    loadSessionTimeline: vi.fn(async () => undefined),
  };
});

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate, getPathParams: () => mocks.pathParams }));
vi.mock('../../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  loadWorkspaces: mocks.loadWorkspaces,
}));
vi.mock('../../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  sessionsError: mocks.sessionsError,
  loadSessions: mocks.loadSessions,
  createSession: mocks.createSession,
  loadSessionDetail: mocks.loadSessionDetail,
}));
vi.mock('../../src/stores/optimisticChat', () => ({ rememberOptimisticInitialMessage: mocks.rememberOptimisticInitialMessage }));
vi.mock('../../src/stores/chatDraft', () => ({ clearChatDraft: mocks.clearChatDraft }));
vi.mock('../../src/stores/timeline', () => ({ resetTimelineState: mocks.resetTimelineState, loadSessionTimeline: mocks.loadSessionTimeline }));

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: 'workspace-1',
  name: 'Pontia Dev',
  canonical_path: '/home/cheny/projects/pontia',
  display_path: '~/projects/pontia',
  state: 'active',
  metadata: {},
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  last_used_at: null,
  ...overrides,
});

const session = (overrides: Partial<SessionView> = {}): SessionView => ({
  session_id: 'session-1',
  client_type: 'pi',
  title: 'Implement workspace page',
  handle: null,
  role: null,
  description: null,
  execution_profile_id: null,
  execution_profile_version: null,
  state: 'idle',
  current_turn_id: null,
  workspace_id: 'workspace-1',
  workspace: null,
  pinned_at: null,
  archived_at: null,
  capabilities: { context_usage: 'unsupported' },
  model: null,
  context_usage: null,
  lineage: null,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-15T00:00:00Z',
  metadata: {},
  ...overrides,
});

const initialTurn = (overrides: Partial<TurnView> = {}): TurnView => ({
  turn_id: 'turn-1',
  session_id: 'session-new',
  state: 'queued',
  input: { summary: 'Start work' },
  output: null,
  failure: null,
  started_at: null,
  completed_at: null,
  metadata: {},
  created_at: '2026-05-15T00:00:00Z',
  ...overrides,
});

beforeEach(() => {
  mocks.pathParams = { workspaceId: 'workspace-1' };
  mocks.workspaces.set([workspace()]);
  mocks.sessions.set([
    session({ session_id: 'session-1', title: 'Workspace session', workspace_id: 'workspace-1' }),
    session({ session_id: 'session-2', title: 'Other session', workspace_id: 'workspace-2' }),
  ]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesError.set(null);
  mocks.sessionsLoading.set(false);
  mocks.sessionsError.set(null);
  mocks.createSession.mockReset();
  mocks.navigate.mockReset();
  mocks.rememberOptimisticInitialMessage.mockReset();
  mocks.clearChatDraft.mockReset();
  mocks.resetTimelineState.mockReset();
  mocks.loadSessionDetail.mockReset();
  mocks.loadSessionTimeline.mockReset();
  vi.clearAllMocks();
});

test('registers a workspace detail route', () => {
  expect(routerConf.routes.some((route) => route.path === '/workspace/{workspaceId}')).toBe(true);
});

test('renders workspace title path and only sessions from that workspace without manual refresh chrome', async () => {
  render(WorkspacePage);

  expect(await screen.findByRole('heading', { name: 'Pontia Dev' })).toBeInTheDocument();
  expect(screen.getByText('/home/cheny/projects/pontia')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /refresh/i })).not.toBeInTheDocument();
  const page = screen.getByTestId('workspace-page');
  expect(page).toHaveClass('max-w-4xl');
  expect(page).not.toHaveClass('max-w-5xl');
  expect(mocks.loadWorkspaces).toHaveBeenCalled();
  expect(mocks.loadSessions).toHaveBeenCalledWith({ includePinned: true, limit: 200 });

  const sessionsRegion = screen.getByRole('region', { name: 'Workspace sessions' });
  expect(within(sessionsRegion).queryByText('Open an existing chat session for this workspace.')).not.toBeInTheDocument();
  const sessionList = within(sessionsRegion).getByTestId('workspace-session-list');
  expect(sessionList).toHaveClass('divide-y');
  expect(sessionList).not.toHaveClass('gap-3');
  const sessionItem = within(sessionList).getByRole('button', { name: /Workspace session/i });
  expect(sessionItem).toBeInTheDocument();
  expect(sessionItem).not.toHaveClass('rounded-xl');
  expect(sessionItem).not.toHaveClass('border');
  expect(sessionItem).not.toHaveClass('bg-card');
  expect(within(sessionsRegion).queryByText('Other session')).not.toBeInTheDocument();
});

test('uses the shared new chat prompt style for creating a workspace session', async () => {
  render(WorkspacePage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  const panel = screen.getByTestId('new-chat-panel');
  expect(panel).toHaveClass('justify-center');
  expect(panel).toContainElement(promptInput);
  expect(screen.queryByRole('heading', { name: 'New session' })).not.toBeInTheDocument();
  expect(screen.queryByText('Create a new chat session in this workspace.')).not.toBeInTheDocument();
  expect(screen.getByText('Start a new agent session in')).toBeInTheDocument();
  expect(screen.getAllByText('Pontia Dev').length).toBeGreaterThan(0);
  expect(within(panel).queryByRole('button', { name: /workspace/i })).not.toBeInTheDocument();
  expect(within(panel).getByLabelText(/client/i)).toHaveTextContent('pi');
});

test('creates a new session in the workspace and opens its chat page', async () => {
  const user = userEvent.setup();
  const createdSession = session({ session_id: 'session-new', title: 'Start work', workspace_id: 'workspace-1' });
  const createdTurn = initialTurn();
  mocks.createSession.mockResolvedValue({ session: createdSession, initial_turn: createdTurn } satisfies CreateSessionResult);

  render(WorkspacePage);

  await user.type(await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Start work');
  await fireEvent.click(screen.getByRole('button', { name: 'Start chat' }));

  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({
    client_type: 'pi',
    workspace_id: 'workspace-1',
    initial_task: { input: 'Start work', metadata: { source: 'dashboard_workspace' } },
    metadata: { source: 'dashboard_workspace' },
  })));
  expect(mocks.rememberOptimisticInitialMessage).toHaveBeenCalledWith('session-new', 'Start work', createdTurn);
  expect(mocks.resetTimelineState).toHaveBeenCalledWith('session-new');
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new');
});

test('opens an existing workspace session in chat', async () => {
  const user = userEvent.setup();
  render(WorkspacePage);

  await user.click(await screen.findByRole('button', { name: /Workspace session/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-1');
});
