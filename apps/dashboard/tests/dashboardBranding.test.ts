import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { render, screen } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import AppSidebarHost from './components/layout/AppSidebarHost.svelte';
import TopBarHost from './components/layout/TopBarHost.svelte';

const __dirname = dirname(fileURLToPath(import.meta.url));
const indexHtml = readFileSync(resolve(__dirname, '../index.html'), 'utf8');

const mocks = vi.hoisted(() => {
  function readableStore<T>(value: T) {
    return {
      subscribe(run: (value: T) => void) {
        run(value);
        return () => {};
      },
    };
  }

  return {
    navigate: vi.fn(),
    sessions: readableStore([]),
    sessionsLoading: readableStore(false),
    workspaces: readableStore([]),
    workspacesLoading: readableStore(false),
    token: readableStore('test-token'),
    sseStatus: readableStore('open'),
    lastConnectionError: readableStore(null),
  };
});

vi.mock('svelte-mini-router', () => ({ navigate: mocks.navigate }));
vi.mock('../src/stores/sessions', () => ({
  sessions: mocks.sessions,
  sessionsLoading: mocks.sessionsLoading,
  updateSessionTitle: vi.fn(async () => undefined),
  pinSession: vi.fn(async () => undefined),
  unpinSession: vi.fn(async () => undefined),
  archiveSession: vi.fn(async () => undefined),
}));
vi.mock('../src/stores/workspaces', () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
}));
vi.mock('../src/stores/auth', () => ({ token: mocks.token }));
vi.mock('../src/stores/connection', () => ({
  sseStatus: mocks.sseStatus,
  lastConnectionError: mocks.lastConnectionError,
}));

test('dashboard head advertises packaged logo icons', () => {
  expect(indexHtml).toContain('<link rel="icon" type="image/svg+xml" href="/dashboard/logo.svg" />');
  expect(indexHtml).toContain('<link rel="icon" href="/dashboard/favicon.ico" sizes="any" />');
  expect(indexHtml).toContain('<link rel="icon" type="image/png" sizes="32x32" href="/dashboard/logo-32.png" />');
  expect(indexHtml).toContain('<link rel="icon" type="image/png" sizes="192x192" href="/dashboard/logo-192.png" />');
  expect(indexHtml).toContain('<link rel="apple-touch-icon" sizes="180x180" href="/dashboard/logo-180.png" />');
});

test('dashboard sidebar renders the SVG logo asset and brand text', () => {
  render(AppSidebarHost);

  const logo = screen.getByRole('button', { name: 'Open new chat' }).querySelector('img');
  expect(logo).toHaveAttribute('src', '/dashboard/logo.svg');
  expect(logo).toHaveAttribute('alt', '');
  expect(logo).toHaveClass('size-6');
  expect(logo).toHaveClass('object-contain');
  expect(screen.getByText('PONTIA')).toHaveClass('group-data-[collapsible=icon]:hidden');
});

test('top bar omits SSE status chrome', () => {
  render(TopBarHost);

  const topBar = screen.getByRole('banner');
  expect(topBar).not.toHaveTextContent('SSE live');
  expect(topBar.querySelector('.lucide-wifi')).not.toBeInTheDocument();
  expect(topBar.querySelector('.lucide-wifi-off')).not.toBeInTheDocument();
  expect(screen.queryByTitle('SSE live')).not.toBeInTheDocument();
});
