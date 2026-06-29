import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import AppSidebarHost from './components/layout/AppSidebarHost.svelte';
import AppShellHost from './components/layout/AppShellHost.svelte';
import TopBarHost from './components/layout/TopBarHost.svelte';
import SettingsShellHost from './components/settings/SettingsShellHost.svelte';
import SettingsCommonPage from '../src/pages/SettingsCommonPage.svelte';
import { routerConf } from '../src/routes';

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

  return {
    navigate: vi.fn(),
    startEventStream: vi.fn(),
    stopEventStream: vi.fn(),
    sessions: writableStore([]),
    sessionsLoading: writableStore(false),
    loadSessions: vi.fn(async () => []),
    updateSessionTitle: vi.fn(async () => undefined),
    pinSession: vi.fn(async () => undefined),
    unpinSession: vi.fn(async () => undefined),
    archiveSession: vi.fn(async () => undefined),
    terminateSession: vi.fn(async () => undefined),
    workspaces: writableStore([]),
    workspacesLoading: writableStore(false),
  };
});

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate }));
vi.mock('../src/services/eventStream', () => ({
  startEventStream: mocks.startEventStream,
  stopEventStream: mocks.stopEventStream,
}));
vi.mock('../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  loadSessions: mocks.loadSessions,
  updateSessionTitle: mocks.updateSessionTitle,
  pinSession: mocks.pinSession,
  unpinSession: mocks.unpinSession,
  archiveSession: mocks.archiveSession,
  terminateSession: mocks.terminateSession,
}));
vi.mock('../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
}));

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/chat');
  mocks.sessions.set([]);
  mocks.sessionsLoading.set(false);
  mocks.workspaces.set([]);
  mocks.workspacesLoading.set(false);
  vi.clearAllMocks();
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

function chatSession(session_id: string, state: string, updated_at: string, pinned_at: string | null = null) {
  return {
    session_id,
    client_type: 'pi',
    handle: session_id,
    role: null,
    description: null,
    execution_profile_id: null,
    execution_profile_version: null,
    state,
    current_turn_id: null,
    workspace_id: 'workspace-1',
    workspace: null,
    capabilities: {},
    created_at: '2026-05-14T00:00:00Z',
    updated_at,
    pinned_at,
    metadata: {},
  };
}

test('sidebar shows session control items and hides overview and DAG task navigation', () => {
  render(AppSidebarHost);

  expect(screen.queryByText('Workflow')).not.toBeInTheDocument();
  expect(screen.queryByText('External API only')).not.toBeInTheDocument();
  expect(screen.queryByText('Overview')).not.toBeInTheDocument();

  const workflow = screen.getByText('New Chat').closest('[data-slot="sidebar-group"]');
  expect(workflow).not.toBeNull();
  const workflowQueries = within(workflow as HTMLElement);
  expect(workflowQueries.queryByText('Tasks')).not.toBeInTheDocument();
  const newChat = workflowQueries.getByText('New Chat').closest('button');
  expect(newChat).not.toBeNull();
  expect(newChat?.querySelector('svg')).toHaveClass('lucide-square-pen');
  expect(workflowQueries.queryByText('Chat')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('DAG Tasks')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Session Console')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Workspaces')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Agent Profiles')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: /settings/i })).toBeInTheDocument();
});

test('sidebar session action appears on hover or focus-visible, not plain focus-within', () => {
  mocks.sessions.set([chatSession('session-active', 'idle', '2026-05-14T01:00:00Z')]);

  render(AppSidebarHost);

  const action = screen.getByRole('button', { name: /open session actions for session-active/i });
  expect(action).toHaveClass('group-hover/menu-item:opacity-100');
  expect(action).toHaveClass('group-has-[:focus-visible]/menu-item:opacity-100');
  expect(action).not.toHaveClass('group-focus-within/menu-item:opacity-100');

  const status = screen.getByLabelText('idle session');
  expect(status).toHaveClass('group-hover/menu-item:opacity-0');
  expect(status).toHaveClass('group-has-[:focus-visible]/menu-item:opacity-0');
  expect(status).not.toHaveClass('group-focus-within/menu-item:opacity-0');
});

test('sidebar shows recent sessions with semantic status dot except exited sessions, and opens chat for the selected session', async () => {
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
    {
      session_id: 'session-closed',
      client_type: 'pi',
      handle: 'closed',
      role: null,
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'exited',
      current_turn_id: null,
      workspace_id: 'workspace-2',
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T02:00:00Z',
      metadata: {},
    },
  ]);

  render(AppSidebarHost);

  expect(screen.getByText('Recent Sessions')).toBeInTheDocument();
  const activeSessionButton = screen.getByText('main · coder').closest('button');
  const closedSessionButton = screen.getByText('closed').closest('button');
  expect(activeSessionButton).not.toBeNull();
  expect(closedSessionButton).not.toBeNull();
  expect(activeSessionButton?.querySelector('.lucide-message-circle')).not.toBeInTheDocument();
  expect(closedSessionButton?.querySelector('.lucide-message-circle')).not.toBeInTheDocument();
  expect(screen.getByLabelText('idle session')).toBeInTheDocument();
  expect(screen.queryByLabelText('exited session')).not.toBeInTheDocument();
  expect(screen.queryByText('idle')).not.toBeInTheDocument();
  expect(screen.queryByText('exited')).not.toBeInTheDocument();

  await fireEvent.click(screen.getByText('main · coder'));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-active');
});

test('sidebar scrolls recent workspace and session groups together below fixed primary navigation', () => {
  render(AppSidebarHost);

  const newChatGroup = screen.getByText('New Chat').closest('[data-slot="sidebar-group"]');
  const recentWorkspacesGroup = screen.getByText('Recent Workspaces').closest('[data-slot="sidebar-group"]');
  const recentSessionsGroup = screen.getByText('Recent Sessions').closest('[data-slot="sidebar-group"]');
  expect(newChatGroup).not.toBeNull();
  expect(recentWorkspacesGroup).not.toBeNull();
  expect(recentSessionsGroup).not.toBeNull();

  const sharedScrollArea = recentWorkspacesGroup?.parentElement;
  expect(sharedScrollArea).toHaveClass('overflow-y-auto');
  expect(sharedScrollArea).toContainElement(recentWorkspacesGroup as HTMLElement);
  expect(sharedScrollArea).toContainElement(recentSessionsGroup as HTMLElement);
  expect(sharedScrollArea).not.toContainElement(newChatGroup as HTMLElement);

  const recentSessionsContent = recentSessionsGroup?.querySelector('[data-slot="sidebar-group-content"]');
  expect(recentSessionsContent).not.toHaveClass('overflow-y-auto');
});

test('sidebar groups recent sessions under non-empty recent workspaces without changing Recent Sessions', async () => {
  mocks.workspaces.set([
    {
      workspace_id: 'workspace-active',
      canonical_path: '/home/cheny/projects/pontia',
      display_path: '~/projects/pontia',
      name: 'Pontia',
      state: 'active',
      metadata: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      last_used_at: '2026-05-14T01:00:00Z',
    },
    {
      workspace_id: 'workspace-empty',
      canonical_path: '/home/cheny/projects/empty',
      display_path: '~/projects/empty',
      name: 'Empty workspace',
      state: 'active',
      metadata: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      last_used_at: null,
    },
    {
      workspace_id: 'workspace-archived',
      canonical_path: '/tmp/old',
      display_path: '/tmp/old',
      name: 'Old workspace',
      state: 'archived',
      metadata: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      last_used_at: null,
    },
  ]);
  mocks.sessions.set([
    {
      session_id: 'session-newer-unpinned',
      client_type: 'pi',
      title: 'Newer unpinned',
      handle: 'newer',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-active',
      pinned_at: null,
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T03:00:00Z',
      metadata: {},
    },
    {
      session_id: 'session-active',
      client_type: 'pi',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-active',
      pinned_at: '2026-05-14T01:30:00Z',
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);

  render(AppSidebarHost);

  expect(screen.getByText('Recent Workspaces')).toBeInTheDocument();
  const workspaceButton = screen.getByRole('button', { name: /pontia/i });
  expect(workspaceButton).toHaveAttribute('aria-expanded', 'false');
  expect(workspaceButton.querySelector('svg')).toHaveClass('lucide-folder');
  expect(screen.queryByText('Empty workspace')).not.toBeInTheDocument();
  expect(screen.queryByText('Old workspace')).not.toBeInTheDocument();
  expect(screen.getByText('Recent Sessions')).toBeInTheDocument();
  expect(screen.getByText('main · coder')).toBeInTheDocument();
  expect(screen.getByText('Newer unpinned')).toBeInTheDocument();

  await fireEvent.click(workspaceButton);

  const workspaceGroup = workspaceButton.closest('[data-slot="sidebar-workspace-group"]');
  expect(workspaceButton).toHaveAttribute('aria-expanded', 'true');
  const workspaceQueries = within(workspaceGroup as HTMLElement);
  const groupedSessionButton = workspaceQueries.getAllByRole('button', { name: /main · coder/i })
    .find((button) => button.getAttribute('data-sidebar') === 'menu-button');
  expect(groupedSessionButton).toBeInTheDocument();
  expect(groupedSessionButton).toHaveClass('h-8');
  expect(groupedSessionButton).toHaveClass('text-sm');
  expect(groupedSessionButton).toHaveClass('group-has-data-[sidebar=menu-action]/menu-item:pr-8');
  expect(groupedSessionButton).not.toHaveClass('h-7');
  expect(groupedSessionButton).not.toHaveClass('text-xs');
  expect(groupedSessionButton.closest('[data-slot="sidebar-menu"]')).toHaveClass('pl-2');
  expect(groupedSessionButton.closest('[data-slot="sidebar-menu"]')).not.toHaveClass('pl-6');
  expect(workspaceQueries.getByLabelText('Pinned session')).toBeInTheDocument();
  expect(workspaceQueries.getAllByLabelText('idle session')).toHaveLength(2);
  expect(workspaceQueries.getByRole('button', { name: /open session actions for main · coder/i })).toBeInTheDocument();
  const workspaceSessionTitles = workspaceQueries
    .getAllByRole('button')
    .filter((button) => button.getAttribute('data-sidebar') === 'menu-button')
    .map((button) => button.textContent?.trim());
  expect(workspaceSessionTitles).toEqual(['main · coder', 'Newer unpinned']);
});

test('sidebar pinned session uses a solid black pin and vertical ellipsis action icon', () => {
  mocks.sessions.set([
    {
      session_id: 'session-pinned',
      client_type: 'pi',
      title: 'Pinned title',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      pinned_at: '2026-05-14T01:00:00Z',
      archived_at: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);

  render(AppSidebarHost);

  expect(screen.getByLabelText('Pinned session')).toHaveClass('fill-current');
  expect(screen.getByLabelText('Pinned session')).toHaveClass('text-black');
  expect(screen.getByRole('button', { name: /open session actions for pinned title/i }).querySelector('svg')).toHaveClass('lucide-ellipsis-vertical');
});

test('sidebar workspace session actions open only for the clicked workspace item', async () => {
  mocks.workspaces.set([
    {
      workspace_id: 'workspace-active',
      canonical_path: '/home/cheny/projects/pontia',
      display_path: '~/projects/pontia',
      name: 'Pontia',
      state: 'active',
      metadata: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      last_used_at: '2026-05-14T01:00:00Z',
    },
  ]);
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      title: 'Shared session',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-active',
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  const workspaceButton = screen.getByRole('button', { name: /pontia/i });
  await fireEvent.click(workspaceButton);
  const workspaceGroup = workspaceButton.closest('[data-slot="sidebar-workspace-group"]');
  const workspaceAction = within(workspaceGroup as HTMLElement).getByRole('button', { name: /open session actions for shared session/i });

  await fireEvent.click(workspaceAction);

  const actionButtons = screen.getAllByRole('button', { name: /open session actions for shared session/i });
  expect(actionButtons.filter((button) => button.getAttribute('data-state') === 'open')).toEqual([workspaceAction]);
  expect(screen.getByRole('menuitem', { name: /rename/i })).toBeInTheDocument();
});

test('sidebar renames a recent session from the hover edit action without opening it', async () => {
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      title: 'Original title',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /open session actions for original title/i }));
  await fireEvent.click(screen.getByRole('menuitem', { name: /rename/i }));

  const dialog = screen.getByRole('dialog', { name: 'Rename session' });
  const titleInput = within(dialog).getByLabelText('Session title');
  await fireEvent.input(titleInput, { target: { value: 'Renamed session' } });
  await fireEvent.click(within(dialog).getByRole('button', { name: 'Rename session' }));

  expect(mocks.updateSessionTitle).toHaveBeenCalledWith('session-active', 'Renamed session');
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test('sidebar session actions menu pins unpinned sessions without opening them', async () => {
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      title: 'Original title',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /open session actions for original title/i }));
  await fireEvent.click(screen.getByRole('menuitem', { name: /pin/i }));
  expect(mocks.pinSession).toHaveBeenCalledWith('session-active');
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test('sidebar session actions menu exits sessions without opening them', async () => {
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      title: 'Original title',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /open session actions for original title/i }));
  const menu = screen.getByRole('menu');
  const menuItems = within(menu).getAllByRole('menuitem').map((item) => item.textContent?.trim());
  const separator = menu.querySelector('[data-slot="dropdown-menu-separator"]');

  expect(menuItems).toEqual(['Rename', 'Pin', 'Archive', 'Exit']);
  expect(separator).not.toBeNull();
  expect(separator?.compareDocumentPosition(within(menu).getByRole('menuitem', { name: /^exit$/i }))).toBe(Node.DOCUMENT_POSITION_FOLLOWING);

  await fireEvent.click(within(menu).getByRole('menuitem', { name: /^exit$/i }));
  expect(mocks.terminateSession).toHaveBeenCalledWith('session-active');
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test('sidebar session actions menu archives sessions without opening them', async () => {
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      title: 'Original title',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      pinned_at: null,
      archived_at: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /open session actions for original title/i }));
  await fireEvent.click(screen.getByRole('menuitem', { name: /archive/i }));
  expect(mocks.archiveSession).toHaveBeenCalledWith('session-active');
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test('sidebar session actions menu unpins pinned sessions', async () => {
  mocks.sessions.set([
    {
      session_id: 'session-pinned',
      client_type: 'pi',
      title: 'Pinned title',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      pinned_at: '2026-05-14T01:00:00Z',
      archived_at: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
  ]);
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /open session actions for pinned title/i }));
  await fireEvent.click(screen.getByRole('menuitem', { name: /unpin/i }));
  expect(mocks.unpinSession).toHaveBeenCalledWith('session-pinned');
  expect(mocks.navigate).not.toHaveBeenCalled();
});

test('sidebar only marks new chat active on the default route', () => {
  window.history.pushState({}, '', '/dashboard/chat');

  render(AppSidebarHost);

  const chat = screen.getByText('New Chat').closest('button');

  expect(screen.queryByText('Overview')).not.toBeInTheDocument();
  expect(screen.queryByText('Tasks')).not.toBeInTheDocument();
  expect(chat).not.toBeNull();

  expect(chat).toHaveAttribute('data-active', 'true');
});

test('sidebar New Chat notifies mounted route components about the route change', async () => {
  window.history.pushState({}, '', '/dashboard/chat/session-active');
  const popstateListener = vi.fn();
  window.addEventListener('popstate', popstateListener);

  render(AppSidebarHost);
  await fireEvent.click(screen.getByText('New Chat'));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat');
  expect(popstateListener).toHaveBeenCalledTimes(1);
  window.removeEventListener('popstate', popstateListener);
});

test('sidebar highlights the matching recent session on chat and session console routes', () => {
  mocks.sessions.set([
    {
      session_id: 'session-active',
      client_type: 'pi',
      handle: 'main',
      role: 'coder',
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-1',
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T01:00:00Z',
      metadata: {},
    },
    {
      session_id: 'session-other',
      client_type: 'pi',
      handle: 'other',
      role: null,
      description: null,
      execution_profile_id: null,
      execution_profile_version: null,
      state: 'idle',
      current_turn_id: null,
      workspace_id: 'workspace-2',
      workspace: null,
      capabilities: {},
      created_at: '2026-05-14T00:00:00Z',
      updated_at: '2026-05-14T00:30:00Z',
      metadata: {},
    },
  ]);

  window.history.pushState({}, '', '/dashboard/chat/session-active');
  const { unmount } = render(AppSidebarHost);

  expect(screen.getByText('main · coder').closest('button')).toHaveAttribute('data-active', 'true');
  expect(screen.getByText('other').closest('button')).not.toHaveAttribute('data-active');

  unmount();
  window.history.pushState({}, '', '/dashboard/sessions/session-active');
  render(AppSidebarHost);

  expect(screen.getByText('main · coder').closest('button')).toHaveAttribute('data-active', 'true');
  expect(screen.getByText('other').closest('button')).not.toHaveAttribute('data-active');
});

test('top bar is compact and transparent with sidebar trigger and chat help button', () => {
  render(TopBarHost);

  const topBar = screen.getByRole('banner');
  expect(topBar).toHaveClass('h-10');
  expect(topBar).toHaveClass('bg-transparent');
  expect(topBar).not.toHaveClass('h-14');
  expect(topBar).not.toHaveClass('border-b');
  expect(topBar).not.toHaveClass('bg-background/95');

  const buttons = within(topBar).getAllByRole('button');
  expect(buttons).toHaveLength(2);
  expect(buttons[0]).toHaveAttribute('data-sidebar', 'trigger');
  expect(buttons[0]).toHaveClass('hover:bg-muted');
  expect(buttons[1]).toHaveAccessibleName(/keyboard shortcuts/i);
  expect(buttons[1]).toHaveClass('hidden', 'sm:inline-flex');
  expect(buttons[1]).not.toHaveClass('ml-auto');
  expect(within(topBar).queryByRole('link', { name: /new chat/i })).not.toBeInTheDocument();
  expect(within(topBar).queryByText(/sse/i)).not.toBeInTheDocument();
  expect(within(topBar).queryByText(/set api token/i)).not.toBeInTheDocument();
});

test('sidebar footer exposes settings as a section menu without agent profiles', async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /settings/i }));

  expect(await screen.findByRole('menuitem', { name: /^common$/i })).toBeInTheDocument();
  expect(screen.getByRole('menuitem', { name: /^workspaces$/i })).toBeInTheDocument();
  expect(screen.queryByRole('menuitem', { name: /^agent profiles$/i })).not.toBeInTheDocument();
});

test('sidebar settings menu navigates to settings sections without document reload', async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /settings/i }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: /^workspaces$/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/settings/workspaces');
});

test('top bar does not expose new chat navigation', () => {
  render(TopBarHost);

  expect(screen.queryByRole('link', { name: /new chat/i })).not.toBeInTheDocument();
  expect(screen.queryByText('New Chat')).not.toBeInTheDocument();
});

test('settings common page contains controls without owning the section switcher', () => {
  window.history.pushState({}, '', '/dashboard/settings/common');

  render(SettingsCommonPage);

  expect(screen.getByRole('heading', { name: /common settings/i })).toBeInTheDocument();
  expect(screen.getByLabelText(/bearer token/i)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /save token/i })).toBeInTheDocument();
  expect(screen.getByText(/live stream/i)).toBeInTheDocument();
  expect(screen.queryByRole('navigation', { name: /settings sections/i })).not.toBeInTheDocument();
});

test('chat shortcuts switch among active sessions on chat routes', async () => {
  window.history.pushState({}, '', '/dashboard/chat/session-older');
  mocks.sessions.set([
    chatSession('session-busy', 'busy', '2026-05-14T04:00:00Z'),
    chatSession('session-error', 'error', '2026-05-14T03:00:00Z'),
    chatSession('session-older', 'idle', '2026-05-14T02:00:00Z'),
    chatSession('session-exited', 'exited', '2026-05-14T01:00:00Z'),
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: 'j', altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith('/chat/session-busy');

  window.history.pushState({}, '', '/dashboard/chat/session-busy');
  await fireEvent.keyDown(window, { key: 'k', altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith('/chat/session-older');
});

test('sidebar shows new chat kbd hint without showing numeric hints beside sessions', () => {
  mocks.sessions.set([
    chatSession('session-recent', 'idle', '2026-05-14T04:00:00Z'),
    chatSession('session-pinned', 'idle', '2026-05-14T01:00:00Z', '2026-05-14T05:00:00Z'),
  ]);

  render(AppSidebarHost);

  const newChat = screen.getByText('New Chat').closest('button');
  expect(newChat).not.toBeNull();
  expect(within(newChat as HTMLElement).getByText('Alt')).toBeInTheDocument();
  expect(within(newChat as HTMLElement).getByText('N')).toBeInTheDocument();

  const pinnedSession = screen.getByText('session-pinned').closest('button');
  expect(pinnedSession).not.toBeNull();
  expect(within(pinnedSession as HTMLElement).queryByText('1')).not.toBeInTheDocument();
  const recentSession = screen.getByText('session-recent').closest('button');
  expect(recentSession).not.toBeNull();
  expect(within(recentSession as HTMLElement).queryByText('2')).not.toBeInTheDocument();
});

test('chat help shortcut opens a kbd shortcut reference dialog', async () => {
  render(AppShellHost);

  await fireEvent.keyDown(window, { key: '?', altKey: true, shiftKey: true });

  const dialog = screen.getByRole('dialog', { name: /keyboard shortcuts/i });
  expect(within(dialog).getByText(/next active chat/i)).toBeInTheDocument();
  expect(within(dialog).getByText(/focus chat input/i)).toBeInTheDocument();
  expect(within(dialog).getAllByText('Alt').length).toBeGreaterThan(0);
  expect(within(dialog).getByText('?')).toBeInTheDocument();
});

test('chat header help button opens the shortcuts dialog and is hidden on mobile', async () => {
  render(AppShellHost);

  const helpButton = screen.getByRole('button', { name: /keyboard shortcuts/i });
  expect(helpButton).toHaveClass('hidden');
  expect(helpButton).toHaveClass('sm:inline-flex');

  await fireEvent.click(helpButton);

  expect(screen.getByRole('dialog', { name: /keyboard shortcuts/i })).toBeInTheDocument();
});

test('chat numeric shortcuts open active sessions by sidebar order and skip inactive sessions', async () => {
  mocks.sessions.set([
    chatSession('session-recent', 'idle', '2026-05-14T04:00:00Z'),
    chatSession('session-pinned', 'idle', '2026-05-14T01:00:00Z', '2026-05-14T05:00:00Z'),
    chatSession('session-error', 'error', '2026-05-14T06:00:00Z'),
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: '1', altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith('/chat/session-pinned');

  await fireEvent.keyDown(window, { key: '2', altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith('/chat/session-recent');

  await fireEvent.keyDown(window, { key: '3', altKey: true });
  expect(mocks.navigate).not.toHaveBeenLastCalledWith('/chat/session-error');
});

test('chat shortcuts are scoped to chat routes and do not interrupt typing', async () => {
  mocks.sessions.set([chatSession('session-recent', 'idle', '2026-05-14T04:00:00Z')]);

  render(AppShellHost);

  window.history.pushState({}, '', '/dashboard/settings/common');
  await fireEvent.keyDown(window, { key: '1', altKey: true });
  expect(mocks.navigate).not.toHaveBeenCalled();

  window.history.pushState({}, '', '/dashboard/chat');
  const input = document.createElement('textarea');
  document.body.appendChild(input);
  input.focus();
  await fireEvent.keyDown(window, { key: '1', altKey: true });
  expect(mocks.navigate).not.toHaveBeenCalled();
  input.remove();
});

test('chat new and focus shortcuts work on chat routes', async () => {
  render(AppShellHost);
  const input = document.createElement('textarea');
  input.setAttribute('data-chat-shortcut-focus-target', 'true');
  document.body.appendChild(input);

  await fireEvent.keyDown(window, { key: 'n', altKey: true });
  expect(mocks.navigate).toHaveBeenLastCalledWith('/chat');

  await fireEvent.keyDown(window, { key: 'l', altKey: true });
  expect(document.activeElement).toBe(input);
  input.remove();
});

test('chat new shortcut on a session route preserves the current session workspace', async () => {
  window.history.pushState({}, '', '/dashboard/chat/session-current');
  mocks.sessions.set([
    {
      ...chatSession('session-current', 'idle', '2026-05-14T04:00:00Z'),
      workspace_id: 'workspace-current',
    },
  ]);

  render(AppShellHost);

  await fireEvent.keyDown(window, { key: 'n', altKey: true });

  expect(mocks.navigate).toHaveBeenLastCalledWith('/chat', { workspace: 'workspace-current' });
});

test('chat app shell uses the surface background behind the transparent header', () => {
  window.history.pushState({}, '', '/dashboard/chat/session-2');

  render(AppShellHost);

  const topBar = screen.getByRole('banner');
  expect(topBar).toHaveClass('bg-transparent');
  expect(topBar.parentElement).toHaveClass('bg-surface');
  expect(topBar.parentElement).not.toHaveClass('bg-background');
});

test('chat app shell reserves composer space only for session chat routes', () => {
  window.history.pushState({}, '', '/dashboard/chat/session-2');

  const { unmount } = render(AppShellHost);

  const sessionMain = screen.getByText('App shell page content').closest('main');
  expect(sessionMain).not.toBeNull();
  expect(sessionMain).not.toHaveClass('min-h-0');
  expect(sessionMain).not.toHaveClass('overflow-hidden');
  expect(sessionMain).toHaveClass('bg-surface');
  expect(sessionMain).not.toHaveClass('bg-muted/20');
  expect(sessionMain).toHaveClass('pb-40');
  expect(sessionMain?.firstElementChild).not.toHaveClass('h-full');
  expect(sessionMain?.firstElementChild).not.toHaveClass('min-h-0');

  unmount();
  window.history.pushState({}, '', '/dashboard/chat');
  render(AppShellHost);

  const newChatMain = screen.getByText('App shell page content').closest('main');
  expect(newChatMain).not.toBeNull();
  expect(newChatMain).toHaveClass('bg-surface');
  expect(newChatMain).not.toHaveClass('bg-muted/20');
  expect(newChatMain).toHaveClass('p-4');
  expect(newChatMain).not.toHaveClass('pb-40');
  expect(newChatMain).not.toHaveClass('md:pb-44');
});

test('settings app shell removes centered main chrome so the settings nav can align left', () => {
  window.history.pushState({}, '', '/dashboard/settings/common');

  render(AppShellHost);

  const main = screen.getByText('App shell page content').closest('main');
  expect(main).not.toBeNull();
  expect(main).toHaveClass('bg-surface');
  expect(main).not.toHaveClass('bg-muted/20');
  expect(main).not.toHaveClass('p-4');
  expect(main).not.toHaveClass('md:p-6');
  expect(main?.firstElementChild).not.toHaveClass('mx-auto');
  expect(main?.firstElementChild).not.toHaveClass('max-w-7xl');
});

test('settings shell renders a persistent vertical side switcher around page content', () => {
  window.history.pushState({}, '', '/dashboard/settings/workspaces');

  render(SettingsShellHost);

  const nav = screen.getByRole('navigation', { name: /settings sections/i });
  expect(nav).toHaveAttribute('data-settings-shell-nav', 'persistent');
  expect(nav).toHaveClass('md:w-56');
  expect(nav.parentElement).toHaveClass('md:items-start');

  const navList = nav.firstElementChild;
  expect(navList).toHaveClass('bg-transparent');
  expect(navList).not.toHaveClass('border');
  expect(navList).not.toHaveClass('bg-card');

  expect(within(nav).getByRole('link', { name: /^common$/i })).toHaveAttribute('href', '/dashboard/settings/common');
  const activeLink = within(nav).getByRole('link', { name: /^workspaces$/i });
  expect(activeLink).toHaveAttribute('aria-current', 'page');
  expect(activeLink).toHaveClass('aria-[current=page]:bg-muted');
  expect(activeLink).not.toHaveClass('aria-[current=page]:bg-primary');
  expect(within(nav).queryByRole('link', { name: /^agent profiles$/i })).not.toBeInTheDocument();

  const content = screen.getByText('Current settings page content');
  expect(content).toBeInTheDocument();
  expect(content.parentElement).toHaveClass('mx-auto');
});

test('settings shell section switcher uses router navigation instead of a document reload', async () => {
  window.history.pushState({}, '', '/dashboard/settings/common');
  render(SettingsShellHost);

  await fireEvent.click(screen.getByRole('link', { name: /^workspaces$/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/settings/workspaces');
});

test('dashboard routes use chat as the default and remove top-level overview', async () => {
  const paths = routerConf.routes.map((route) => route.path);

  const rootRoute = routerConf.routes.find((route) => route.path === '/');
  const settingsRoute = routerConf.routes.find((route) => route.path === '/settings');

  const newChatPage = await import('../src/pages/NewChatPage.svelte');
  const sessionChatPage = await import('../src/pages/SessionChatPage.svelte');
  const settingsRedirectPage = await import('../src/pages/SettingsRedirectPage.svelte');

  expect(rootRoute).toBeDefined();
  expect((await rootRoute?.render())?.default).toBe(newChatPage.default);
  expect(paths).not.toContain('/overview');
  expect(paths).not.toContain('/tasks');
  expect(paths.some((path) => path.startsWith('/tasks/'))).toBe(false);
  expect(settingsRoute).toBeDefined();
  expect((await settingsRoute?.render())?.default).toBe(settingsRedirectPage.default);
  expect((await routerConf.routes.find((route) => route.path === '/chat')?.render())?.default).toBe(newChatPage.default);
  expect((await routerConf.routes.find((route) => route.path === '/chat/{sessionId}')?.render())?.default).toBe(sessionChatPage.default);
  expect(paths).toContain('/chat/{sessionId}');
  expect(paths).toContain('/sessions/{sessionId}');
  expect(paths).toContain('/settings/common');
  expect(paths).toContain('/settings/workspaces');
  expect(paths).toContain('/settings/agent-profiles');
});
