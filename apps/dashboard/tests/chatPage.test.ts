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
    capabilities: { accept_task: true },
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
  mocks.workspaces.set([{ workspace_id: 'workspace-1', canonical_path: '/work/project', display_path: '/work/project', name: 'project', state: 'active', metadata: {}, created_at: '2026-05-14T00:00:00Z', updated_at: '2026-05-14T01:00:00Z', last_used_at: null }]);
  mocks.workspaceGitStatuses.set({});
  mocks.workspaceGitStatusErrors.set({});
  vi.clearAllMocks();
});

test('chat composer metadata uses the compact summary for all viewports', async () => {
  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  const metadata = within(toolbar).getByTestId('session-status-mobile-metadata');

  expect(within(toolbar).getByLabelText('Session state: idle')).toBeInTheDocument();
  expect(within(toolbar).queryByText('idle')).not.toBeInTheDocument();
  expect(within(toolbar).queryByTestId('session-status-desktop-metadata')).not.toBeInTheDocument();
  const detailsButton = within(metadata).getByRole('button', { name: 'Session details: project · pi' });
  expect(detailsButton).toBeInTheDocument();
  expect(within(metadata).queryByLabelText('Workspace: /work/project')).not.toBeInTheDocument();
  expect(within(metadata).queryByLabelText('Client: pi')).not.toBeInTheDocument();
  expect(within(toolbar).getByRole('button', { name: /exit session/i })).toBeInTheDocument();
  expect(within(toolbar).getByRole('button', { name: /advanced session controls/i })).toBeInTheDocument();
});

test('mobile session summary expands current metadata details', async () => {
  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  const mobileMetadata = within(toolbar).getByTestId('session-status-mobile-metadata');

  await fireEvent.click(within(mobileMetadata).getByRole('button', { name: 'Session details: project · pi' }));

  const details = within(mobileMetadata).getByRole('dialog', { name: 'Session details' });
  expect(details).toHaveAttribute('data-slot', 'popover-content');
  expect(details).toHaveAttribute('data-side', 'top');
  expect(within(details).queryByText('Session details')).not.toBeInTheDocument();
  expect(within(details).getByLabelText('Workspace')).toBeInTheDocument();
  expect(within(details).queryByText('Workspace')).not.toBeInTheDocument();
  expect(within(details).getByText('project')).toBeInTheDocument();
  expect(within(details).getByLabelText('Client')).toBeInTheDocument();
  expect(within(details).queryByText('Client')).not.toBeInTheDocument();
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
  const mobileMetadata = within(toolbar).getByTestId('session-status-mobile-metadata');

  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  expect(within(toolbar).queryByTestId('session-status-desktop-metadata')).not.toBeInTheDocument();
  expect(within(mobileMetadata).queryByLabelText('Workspace: /work/project')).not.toBeInTheDocument();
  expect(within(mobileMetadata).queryByLabelText('Client: pi')).not.toBeInTheDocument();
  expect(within(mobileMetadata).queryByLabelText('Git status: main, dirty')).not.toBeInTheDocument();
  expect(within(mobileMetadata).queryByLabelText('Git branch')).not.toBeInTheDocument();
  const mobileSummary = mobileMetadata.querySelector('[data-chat-session-details-summary]');
  expect(mobileSummary).toHaveClass('truncate');
  expect(mobileSummary).toHaveTextContent('project · main ↑1 ↓2 +3 ~4 ?5 !6 · pi');
  expect(within(mobileMetadata).getByText('main')).not.toHaveClass('text-amber-600');
  expect(within(mobileMetadata).getByText('↑1')).toHaveClass('text-blue-600');

  await fireEvent.click(within(mobileMetadata).getByRole('button', { name: 'Session details: project · pi · main · dirty' }));
  const details = within(mobileMetadata).getByRole('dialog', { name: 'Session details' });
  const gitRow = within(details).getByLabelText('Git').closest('div');
  expect(within(details).queryByText('Git')).not.toBeInTheDocument();
  expect(gitRow).not.toBeNull();
  expect(within(gitRow as HTMLElement).getByText('main')).not.toHaveClass('text-amber-600');
  expect(within(gitRow as HTMLElement).getByText('↑1')).toHaveClass('text-blue-600');
});

test('chat refreshes workspace git status when the composer receives focus', async () => {
  render(ChatPage);
  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  mocks.refreshWorkspaceGitStatus.mockClear();

  await fireEvent.focus(await screen.findByPlaceholderText('Send a follow-up message…'));

  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1');
});

test('chat refreshes workspace git status when the page becomes visible', async () => {
  render(ChatPage);
  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  mocks.refreshWorkspaceGitStatus.mockClear();
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });

  document.dispatchEvent(new Event('visibilitychange'));

  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1');
});

test('chat refreshes workspace git status when the selected session becomes idle', async () => {
  render(ChatPage);
  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  mocks.refreshWorkspaceGitStatus.mockClear();
  const listener = mocks.subscribeDashboardEvents.mock.calls[0][0];

  listener({
    kind: 'session_event',
    event: {
      session_id: 'session-1',
      type: 'session.ready',
      payload: {},
      created_at: '2026-05-14T01:31:00Z',
    },
  });

  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1');
});

test('chat composer session status pill uses semantic color classes', async () => {
  mocks.sessions.update((sessions) => sessions.map((session) => ({ ...session, state: 'busy' })));
  mocks.sessionDetail.update((detail) => detail ? { ...detail, session: { ...detail.session, state: 'busy' } } : detail);

  render(ChatPage);

  const toolbar = await screen.findByLabelText('Session status and controls');
  const statusBadge = within(toolbar).getByLabelText('Session state: busy');
  expect(statusBadge).toHaveClass('border-amber-500/30', 'bg-amber-500/10', 'text-amber-700');
  expect(within(statusBadge).queryByText('busy')).not.toBeInTheDocument();
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
    expect(menu).toHaveAttribute('data-slot', 'dropdown-menu-content');
    expect(menu).toHaveAttribute('data-side', 'top');
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
