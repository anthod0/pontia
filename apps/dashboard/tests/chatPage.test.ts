import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import ChatPage from '../src/pages/ChatPage.svelte';

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

  const session = {
    session_id: 'session-1',
    client_type: 'pi',
    handle: null,
    role: null,
    description: null,
    execution_profile_id: null,
    execution_profile_version: null,
    state: 'idle',
    current_turn_id: null,
    workspace_id: 'workspace-1',
    workspace: '/work/project',
    capabilities: {},
    created_at: '2026-05-14T00:00:00Z',
    updated_at: '2026-05-14T01:00:00Z',
    metadata: {},
  };

  return {
    navigate: vi.fn(),
    subscribeDashboardEvents: vi.fn(() => vi.fn()),
    sessions: writableStore([session]),
    sessionDetail: writableStore({ session, turns: [], inboxMessages: [], events: [], artifacts: [] }),
    sessionDetailLoading: writableStore(false),
    sessionDetailError: writableStore(null),
    sessionsError: writableStore(null),
    workspaces: writableStore([]),
    workspacesLoading: writableStore(false),
    workspacesError: writableStore(null),
    workspaceGitStatuses: writableStore({}),
    workspaceGitStatusErrors: writableStore({}),
    agentProfiles: writableStore([]),
    agentProfilesLoading: writableStore(false),
    agentProfilesError: writableStore(null),
    taskProposals: writableStore([]),
    taskProposalsLoading: writableStore(false),
    taskProposalsError: writableStore(null),
    loadSessions: vi.fn(async () => undefined),
    loadSessionDetail: vi.fn(async () => null),
    loadWorkspaces: vi.fn(async () => undefined),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    loadAgentProfiles: vi.fn(async () => undefined),
    loadTaskProposals: vi.fn(async () => undefined),
    createSession: vi.fn(),
    submitInboxMessage: vi.fn(),
    restartSession: vi.fn(),
    resumeSession: vi.fn(),
    terminateSession: vi.fn(),
    createDagTask: vi.fn(),
  };
});

vi.mock('svelte-mini-router', () => ({
  navigate: mocks.navigate,
  getPathParams: () => ({ sessionId: window.location.pathname.split('/').pop() }),
}));
vi.mock('svelte-sonner', () => ({ toast: { error: vi.fn() } }));
vi.mock('../src/services/eventStream', () => ({ subscribeDashboardEvents: mocks.subscribeDashboardEvents }));
vi.mock('../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionDetail: mocks.sessionDetail,
  sessionDetailLoading: mocks.sessionDetailLoading,
  sessionDetailError: mocks.sessionDetailError,
  sessionsError: mocks.sessionsError,
  loadSessions: mocks.loadSessions,
  loadSessionDetail: mocks.loadSessionDetail,
  createSession: mocks.createSession,
  submitInboxMessage: mocks.submitInboxMessage,
  restartSession: mocks.restartSession,
  resumeSession: mocks.resumeSession,
  terminateSession: mocks.terminateSession,
}));
vi.mock('../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  workspaceGitStatuses: mocks.workspaceGitStatuses,
  workspaceGitStatusErrors: mocks.workspaceGitStatusErrors,
  loadWorkspaces: mocks.loadWorkspaces,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
}));
vi.mock('../src/stores/agentProfiles', async () => {
  const actual = await vi.importActual<typeof import('../src/stores/agentProfiles')>('../src/stores/agentProfiles');
  return {
    ...actual,
    agentProfiles: mocks.agentProfiles,
    agentProfilesLoading: mocks.agentProfilesLoading,
    agentProfilesError: mocks.agentProfilesError,
    loadAgentProfiles: mocks.loadAgentProfiles,
  };
});
vi.mock('../src/stores/tasks', () => ({
  taskProposals: mocks.taskProposals,
  taskProposalsLoading: mocks.taskProposalsLoading,
  taskProposalsError: mocks.taskProposalsError,
  loadTaskProposals: mocks.loadTaskProposals,
  createDagTask: mocks.createDagTask,
}));

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/chat/session-1');
  mocks.workspaceGitStatuses.set({});
  mocks.workspaceGitStatusErrors.set({});
  vi.clearAllMocks();
});

test('chat composer metadata uses desktop pills and a compact mobile summary', async () => {
  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  const desktopMetadata = within(toolbar).getByTestId('session-status-desktop-metadata');
  const mobileMetadata = within(toolbar).getByTestId('session-status-mobile-metadata');

  expect(within(toolbar).getByText('idle')).toBeInTheDocument();
  expect(within(desktopMetadata).getByText('/work/project')).toBeInTheDocument();
  expect(within(desktopMetadata).getByLabelText('Client: pi')).toBeInTheDocument();
  expect(within(desktopMetadata).queryByText('Client: pi')).not.toBeInTheDocument();
  expect(within(mobileMetadata).getByRole('button', { name: 'Session details: /work/project +1' })).toBeInTheDocument();
  expect(within(toolbar).getByRole('button', { name: /exit session/i })).toBeInTheDocument();
  expect(within(toolbar).getByRole('button', { name: /advanced session controls/i })).toBeInTheDocument();
});

test('mobile session summary expands current metadata details', async () => {
  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  const mobileMetadata = within(toolbar).getByTestId('session-status-mobile-metadata');

  await fireEvent.click(within(mobileMetadata).getByRole('button', { name: 'Session details: /work/project +1' }));

  const details = within(mobileMetadata).getByRole('dialog', { name: 'Session details' });
  expect(within(details).getByText('Workspace')).toBeInTheDocument();
  expect(within(details).getByText('/work/project')).toBeInTheDocument();
  expect(within(details).getByText('Client')).toBeInTheDocument();
  expect(within(details).getByText('pi')).toBeInTheDocument();
});

test('chat session refreshes and shows workspace git status', async () => {
  mocks.workspaceGitStatuses.set({
    'workspace-1': {
      workspace_id: 'workspace-1',
      repo_root: '/work/project',
      branch: 'main',
      upstream: 'origin/main',
      ahead: 1,
      behind: 2,
      staged_count: 3,
      unstaged_count: 4,
      untracked_count: 5,
      conflicted_count: 6,
      clean: false,
      state: 'observed',
      failure: null,
      observed_at: '2026-05-14T01:30:00Z',
      updated_at: '2026-05-14T01:30:00Z',
    },
  });

  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  const desktopMetadata = within(toolbar).getByTestId('session-status-desktop-metadata');

  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  const gitBadge = within(desktopMetadata).getByLabelText('Git status: main, dirty');
  expect(gitBadge).toBeInTheDocument();
  expect(within(gitBadge).getByText('main')).toBeInTheDocument();
  expect(within(gitBadge).getByLabelText('dirty git status')).toBeInTheDocument();
  expect(within(gitBadge).queryByText('dirty')).not.toBeInTheDocument();
  expect(within(gitBadge).getByLabelText('Git branch')).toHaveClass('text-amber-600');
  expect(within(desktopMetadata).getByText('↑1')).toHaveClass('text-blue-600');
  expect(within(desktopMetadata).getByText('↓2')).toHaveClass('text-violet-600');
  expect(within(desktopMetadata).getByText('+3')).toHaveClass('text-emerald-600');
  expect(within(desktopMetadata).getByText('~4')).toHaveClass('text-amber-600');
  expect(within(desktopMetadata).getByText('?5')).toHaveClass('text-cyan-600');
  expect(within(desktopMetadata).getByText('!6')).toHaveClass('text-destructive');
});

test('chat composer session status pill uses semantic color classes', async () => {
  mocks.sessions.update((sessions) => sessions.map((session) => ({ ...session, state: 'busy' })));
  mocks.sessionDetail.update((detail) => detail ? { ...detail, session: { ...detail.session, state: 'busy' } } : detail);

  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  expect(within(toolbar).getByText('busy').closest('[data-slot="badge"]')).toHaveClass('border-blue-500/30', 'bg-blue-500/10', 'text-blue-700');
});

test('advanced controls menu opens above when there is not enough space below', async () => {
  const originalGetBoundingClientRect = HTMLElement.prototype.getBoundingClientRect;
  const originalOffsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
  HTMLElement.prototype.getBoundingClientRect = function () {
    if (this.getAttribute('aria-label') === 'Advanced session controls') {
      return { x: 360, y: 720, top: 720, right: 388, bottom: 748, left: 360, width: 28, height: 28, toJSON: () => ({}) } as DOMRect;
    }
    return originalGetBoundingClientRect.call(this);
  };
  Object.defineProperty(HTMLElement.prototype, 'offsetHeight', { configurable: true, get: () => 160 });

  try {
    render(ChatPage);

    const toolbar = await screen.findByLabelText('Session status and controls');
    await fireEvent.click(within(toolbar).getByRole('button', { name: /advanced session controls/i }));

    const menu = await screen.findByRole('menu');
    expect(menu).toHaveAttribute('data-placement', 'top');
  } finally {
    HTMLElement.prototype.getBoundingClientRect = originalGetBoundingClientRect;
    if (originalOffsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', originalOffsetHeight);
  }
});

test('chat composer hides missing profile and handle metadata', async () => {
  render(ChatPage);

  await screen.findByLabelText('Session status and controls');

  expect(screen.queryByText(/^Profile:/)).not.toBeInTheDocument();
  expect(screen.queryByText(/^Handle:/)).not.toBeInTheDocument();
  expect(screen.queryByText('—')).not.toBeInTheDocument();
});
