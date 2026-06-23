import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import AppSidebarHost from './components/layout/AppSidebarHost.svelte';
import AppShellHost from './components/layout/AppShellHost.svelte';
import TopBarHost from './components/layout/TopBarHost.svelte';
import SettingsShellHost from './components/settings/SettingsShellHost.svelte';
import SettingsCommonPage from '../src/pages/SettingsCommonPage.svelte';
import { routerConf } from '../src/routes';

const chatPageSource = readFileSync(resolve(__dirname, '../src/pages/ChatPage.svelte'), 'utf8');

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

  await fireEvent.click(workspaceButton);

  const workspaceGroup = workspaceButton.closest('[data-slot="sidebar-menu-item"]');
  expect(workspaceButton).toHaveAttribute('aria-expanded', 'true');
  const groupedSessionButton = within(workspaceGroup as HTMLElement).getByRole('button', { name: /main · coder/i });
  expect(groupedSessionButton).toBeInTheDocument();
  expect(groupedSessionButton).toHaveClass('h-8');
  expect(groupedSessionButton).toHaveClass('text-sm');
  expect(groupedSessionButton).not.toHaveClass('h-7');
  expect(groupedSessionButton).not.toHaveClass('text-xs');
  expect(groupedSessionButton.closest('[data-slot="sidebar-menu"]')).toHaveClass('pl-2');
  expect(groupedSessionButton.closest('[data-slot="sidebar-menu"]')).not.toHaveClass('pl-6');
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

  await fireEvent.click(screen.getByRole('button', { name: /rename session original title/i }));

  const dialog = screen.getByRole('dialog', { name: 'Rename session' });
  const titleInput = within(dialog).getByLabelText('Session title');
  await fireEvent.input(titleInput, { target: { value: 'Renamed session' } });
  await fireEvent.click(within(dialog).getByRole('button', { name: 'Rename session' }));

  expect(mocks.updateSessionTitle).toHaveBeenCalledWith('session-active', 'Renamed session');
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

test('top bar omits static dashboard title, description copy, and settings control', () => {
  render(TopBarHost);

  expect(screen.queryByText('Dashboard v2')).not.toBeInTheDocument();
  expect(screen.queryByText('DAG tasks, workspaces, profiles, and execution diagnostics')).not.toBeInTheDocument();
  expect(screen.getByRole('link', { name: /new chat/i })).toHaveAttribute('href', '/dashboard/chat');
  expect(screen.queryByRole('link', { name: /browse tasks/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /settings/i })).not.toBeInTheDocument();
});

test('sidebar footer exposes settings as a section menu', async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /settings/i }));

  expect(await screen.findByRole('menuitem', { name: /^common$/i })).toBeInTheDocument();
  expect(screen.getByRole('menuitem', { name: /^workspaces$/i })).toBeInTheDocument();
  expect(screen.getByRole('menuitem', { name: /^agent profiles$/i })).toBeInTheDocument();
});

test('sidebar settings menu navigates to settings sections without document reload', async () => {
  render(AppSidebarHost);

  await fireEvent.click(screen.getByRole('button', { name: /settings/i }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: /^agent profiles$/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/settings/agent-profiles');
});

test('top bar New Chat uses SPA navigation and notifies mounted route components', async () => {
  window.history.pushState({}, '', '/dashboard/chat/session-active');
  const popstateListener = vi.fn();
  window.addEventListener('popstate', popstateListener);

  render(TopBarHost);
  await fireEvent.click(screen.getByRole('link', { name: /new chat/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat');
  expect(popstateListener).toHaveBeenCalledTimes(1);
  window.removeEventListener('popstate', popstateListener);
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

test('new chat route uses small viewport units to avoid mobile browser chrome overflow', () => {
  expect(chatPageSource).toContain('min-h-[calc(100svh-5.5rem)]');
  expect(chatPageSource).toContain('md:min-h-[calc(100svh-6.5rem)]');
  expect(chatPageSource).not.toContain('min-h-[calc(100vh-');
});

test('chat app shell reserves composer space only for session chat routes', () => {
  window.history.pushState({}, '', '/dashboard/chat/session-2');

  const { unmount } = render(AppShellHost);

  const sessionMain = screen.getByText('App shell page content').closest('main');
  expect(sessionMain).not.toBeNull();
  expect(sessionMain).not.toHaveClass('min-h-0');
  expect(sessionMain).not.toHaveClass('overflow-hidden');
  expect(sessionMain).toHaveClass('pb-40');
  expect(sessionMain?.firstElementChild).not.toHaveClass('h-full');
  expect(sessionMain?.firstElementChild).not.toHaveClass('min-h-0');

  unmount();
  window.history.pushState({}, '', '/dashboard/chat');
  render(AppShellHost);

  const newChatMain = screen.getByText('App shell page content').closest('main');
  expect(newChatMain).not.toBeNull();
  expect(newChatMain).toHaveClass('p-4');
  expect(newChatMain).not.toHaveClass('pb-40');
  expect(newChatMain).not.toHaveClass('md:pb-44');
});

test('settings app shell removes centered main chrome so the settings nav can align left', () => {
  window.history.pushState({}, '', '/dashboard/settings/common');

  render(AppShellHost);

  const main = screen.getByText('App shell page content').closest('main');
  expect(main).not.toBeNull();
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
  expect(within(nav).getByRole('link', { name: /^agent profiles$/i })).toHaveAttribute('href', '/dashboard/settings/agent-profiles');

  const content = screen.getByText('Current settings page content');
  expect(content).toBeInTheDocument();
  expect(content.parentElement).toHaveClass('mx-auto');
});

test('settings shell section switcher uses router navigation instead of a document reload', async () => {
  window.history.pushState({}, '', '/dashboard/settings/common');
  render(SettingsShellHost);

  await fireEvent.click(screen.getByRole('link', { name: /^agent profiles$/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/settings/agent-profiles');
});

test('dashboard routes use chat as the default and remove top-level overview', () => {
  const paths = routerConf.routes.map((route) => route.path);

  const rootRoute = routerConf.routes.find((route) => route.path === '/');
  const settingsRoute = routerConf.routes.find((route) => route.path === '/settings');

  expect(rootRoute).toBeDefined();
  expect(String(rootRoute?.render)).toContain('ChatPage');
  expect(paths).not.toContain('/overview');
  expect(settingsRoute).toBeDefined();
  expect(String(settingsRoute?.render)).toContain('SettingsRedirectPage');
  expect(paths).toContain('/chat/{sessionId}');
  expect(paths).toContain('/sessions/{sessionId}');
  expect(paths).toContain('/settings/common');
  expect(paths).toContain('/settings/workspaces');
  expect(paths).toContain('/settings/agent-profiles');
});
