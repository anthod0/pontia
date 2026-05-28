import { render, screen, within } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import AppSidebarHost from './components/layout/AppSidebarHost.svelte';
import SettingsPage from '../src/pages/SettingsPage.svelte';

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

test('sidebar shows workflow items and keeps settings in the footer', () => {
  render(AppSidebarHost);

  expect(screen.queryByText('Workflow')).not.toBeInTheDocument();
  expect(screen.queryByText('External API only')).not.toBeInTheDocument();

  const workflow = screen.getByText('Overview').closest('[data-slot="sidebar-group"]');
  expect(workflow).not.toBeNull();
  const footer = screen.getByText('Settings').closest('[data-slot="sidebar-footer"]');
  expect(footer).not.toBeNull();

  const workflowQueries = within(workflow as HTMLElement);
  expect(workflowQueries.getByText('Overview')).toBeInTheDocument();
  expect(workflowQueries.getByText('Tasks')).toBeInTheDocument();
  expect(workflowQueries.getByText('Chat')).toBeInTheDocument();
  expect(workflowQueries.queryByText('DAG Tasks')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Session Console')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Workspaces')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Agent Profiles')).not.toBeInTheDocument();
  expect(workflowQueries.queryByText('Settings')).not.toBeInTheDocument();

  expect(within(footer as HTMLElement).getByText('Settings')).toBeInTheDocument();
});

test('settings page exposes administration links moved out of the sidebar', () => {
  render(SettingsPage);

  expect(screen.getByRole('link', { name: /workspaces/i })).toHaveAttribute('href', '/dashboard/workspaces');
  expect(screen.getByRole('link', { name: /agent profiles/i })).toHaveAttribute('href', '/dashboard/agent-profiles');
});
