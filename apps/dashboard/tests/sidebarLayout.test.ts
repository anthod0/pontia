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
}));

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/overview');
  mocks.sessions.set([]);
  mocks.sessionsLoading.set(false);
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

test('sidebar shows workflow items and omits settings from navigation', () => {
  render(AppSidebarHost);

  expect(screen.queryByText('Workflow')).not.toBeInTheDocument();
  expect(screen.queryByText('External API only')).not.toBeInTheDocument();

  const workflow = screen.getByText('Overview').closest('[data-slot="sidebar-group"]');
  expect(workflow).not.toBeNull();
  const workflowQueries = within(workflow as HTMLElement);
  expect(workflowQueries.getByText('Overview')).toBeInTheDocument();
  expect(workflowQueries.getByText('Tasks')).toBeInTheDocument();
  const newChat = workflowQueries.getByText('New Chat').closest('button');
  expect(newChat).not.toBeNull();
  expect(newChat?.querySelector('svg')).toHaveClass('lucide-square-pen');
  expect(workflowQueries.queryByText('Chat')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('DAG Tasks')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Session Console')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Workspaces')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Agent Profiles')).not.toBeInTheDocument();
  expect(screen.queryByText('Settings')).not.toBeInTheDocument();
});

test('sidebar shows recent sessions with active dot, including exited sessions, and opens chat for the selected session', async () => {
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
  expect(screen.getByText('main · coder')).toBeInTheDocument();
  expect(screen.getByLabelText('Active session')).toBeInTheDocument();
  expect(screen.getByText('closed')).toBeInTheDocument();
  expect(screen.queryByText('idle')).not.toBeInTheDocument();
  expect(screen.queryByText('exited')).not.toBeInTheDocument();

  await fireEvent.click(screen.getByText('main · coder'));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-active');
});

test('sidebar only marks the current route as active', () => {
  window.history.pushState({}, '', '/dashboard/chat/session-active');

  render(AppSidebarHost);

  const overview = screen.getByText('Overview').closest('button');
  const tasks = screen.getByText('Tasks').closest('button');
  const chat = screen.getByText('New Chat').closest('button');

  expect(overview).not.toBeNull();
  expect(tasks).not.toBeNull();
  expect(chat).not.toBeNull();

  expect(chat).toHaveAttribute('data-active', 'true');
  expect(overview).not.toHaveAttribute('data-active');
  expect(tasks).not.toHaveAttribute('data-active');
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

test('top bar omits static dashboard title and description copy and links settings button to common settings', () => {
  render(TopBarHost);

  expect(screen.queryByText('Dashboard v2')).not.toBeInTheDocument();
  expect(screen.queryByText('DAG tasks, workspaces, profiles, and execution diagnostics')).not.toBeInTheDocument();
  expect(screen.getByRole('link', { name: /settings/i })).toHaveAttribute('href', '/dashboard/settings/common');
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

test('settings routes redirect hub and include section paths', () => {
  const paths = routerConf.routes.map((route) => route.path);

  const settingsRoute = routerConf.routes.find((route) => route.path === '/settings');

  expect(settingsRoute).toBeDefined();
  expect(String(settingsRoute?.render)).toContain('SettingsRedirectPage');
  expect(paths).toContain('/chat/{sessionId}');
  expect(paths).toContain('/sessions/{sessionId}');
  expect(paths).toContain('/settings/common');
  expect(paths).toContain('/settings/workspaces');
  expect(paths).toContain('/settings/agent-profiles');
});
