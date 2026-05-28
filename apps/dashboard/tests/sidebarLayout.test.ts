import { render, screen, within } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import AppSidebarHost from './components/layout/AppSidebarHost.svelte';
import TopBarHost from './components/layout/TopBarHost.svelte';
import SettingsShellHost from './components/settings/SettingsShellHost.svelte';
import SettingsCommonPage from '../src/pages/SettingsCommonPage.svelte';
import { routerConf } from '../src/routes';

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  startEventStream: vi.fn(),
  stopEventStream: vi.fn(),
}));

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate }));
vi.mock('../src/services/eventStream', () => ({
  startEventStream: mocks.startEventStream,
  stopEventStream: mocks.stopEventStream,
}));

beforeEach(() => {
  window.history.pushState({}, '', '/dashboard/overview');
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
  expect(workflowQueries.getByText('Chat')).toBeInTheDocument();
  expect(workflowQueries.queryByText('DAG Tasks')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Session Console')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Workspaces')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Agent Profiles')).not.toBeInTheDocument();
  expect(screen.queryByText('Settings')).not.toBeInTheDocument();
});

test('sidebar only marks the current route as active', () => {
  window.history.pushState({}, '', '/dashboard/chat');

  render(AppSidebarHost);

  const overview = screen.getByText('Overview').closest('button');
  const tasks = screen.getByText('Tasks').closest('button');
  const chat = screen.getByText('Chat').closest('button');

  expect(overview).not.toBeNull();
  expect(tasks).not.toBeNull();
  expect(chat).not.toBeNull();

  expect(chat).toHaveAttribute('data-active', 'true');
  expect(overview).not.toHaveAttribute('data-active');
  expect(tasks).not.toHaveAttribute('data-active');
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

test('settings shell renders a persistent vertical side switcher around page content', () => {
  window.history.pushState({}, '', '/dashboard/settings/workspaces');

  render(SettingsShellHost);

  const nav = screen.getByRole('navigation', { name: /settings sections/i });
  expect(nav).toHaveAttribute('data-settings-shell-nav', 'persistent');
  expect(nav).toHaveClass('md:w-56');
  expect(within(nav).getByRole('link', { name: /^common$/i })).toHaveAttribute('href', '/dashboard/settings/common');
  expect(within(nav).getByRole('link', { name: /^workspaces$/i })).toHaveAttribute('aria-current', 'page');
  expect(within(nav).getByRole('link', { name: /^agent profiles$/i })).toHaveAttribute('href', '/dashboard/settings/agent-profiles');
  expect(screen.getByText('Current settings page content')).toBeInTheDocument();
});

test('settings routes redirect hub and include section paths', () => {
  const paths = routerConf.routes.map((route) => route.path);

  const settingsRoute = routerConf.routes.find((route) => route.path === '/settings');

  expect(settingsRoute).toBeDefined();
  expect(String(settingsRoute?.render)).toContain('SettingsRedirectPage');
  expect(paths).toContain('/settings/common');
  expect(paths).toContain('/settings/workspaces');
  expect(paths).toContain('/settings/agent-profiles');
});
