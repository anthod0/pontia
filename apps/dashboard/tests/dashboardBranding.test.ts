import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { render, screen } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import AppSidebarHost from './components/layout/AppSidebarHost.svelte';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appHtml = readFileSync(resolve(__dirname, '../src/app.html'), 'utf8');
const manifest = JSON.parse(
  readFileSync(resolve(__dirname, '../public/manifest.webmanifest'), 'utf8'),
) as {
  id: string;
  name: string;
  start_url: string;
  scope: string;
  display: string;
  icons: Array<{ src: string; sizes: string; type: string }>;
};

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

vi.mock('$lib/navigation', () => ({ navigate: mocks.navigate }));
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

test('dashboard packages the approved SVG asset', () => {
  const svg = readFileSync(resolve(__dirname, '../public/logo.svg'));
  expect(createHash('sha256').update(svg).digest('hex')).toBe('39b8075d58adab952b8f98339460510704cef28fc8d388e2a01f490b43d8a846');
});

test('dashboard head advertises packaged logo icons', () => {
  expect(appHtml).toContain('<link rel="icon" type="image/svg+xml" href="%sveltekit.assets%/logo.svg" />');
  expect(appHtml).toContain('<link rel="icon" href="%sveltekit.assets%/favicon.ico" sizes="any" />');
  expect(appHtml).toContain('<link rel="icon" type="image/png" sizes="32x32" href="%sveltekit.assets%/logo-32.png" />');
  expect(appHtml).toContain('<link rel="icon" type="image/png" sizes="192x192" href="%sveltekit.assets%/logo-192.png" />');
  expect(appHtml).toContain('<link rel="apple-touch-icon" sizes="180x180" href="%sveltekit.assets%/logo-180.png" />');
  expect(appHtml).toContain('<link rel="manifest" href="%sveltekit.assets%/manifest.webmanifest" />');
  expect(appHtml).toContain('<meta name="theme-color" content="#fafaf8" />');
});

test('dashboard manifest is installable within the dashboard scope', () => {
  expect(manifest).toMatchObject({
    id: '/dashboard/',
    name: 'Pontia Dashboard',
    start_url: '/dashboard/',
    scope: '/dashboard/',
    display: 'standalone',
  });
  expect(manifest.icons).toEqual(expect.arrayContaining([
    { src: '/dashboard/logo-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/dashboard/logo-512.png', sizes: '512x512', type: 'image/png' },
  ]));
});

test('dashboard sidebar renders the SVG logo asset and brand text', () => {
  render(AppSidebarHost);

  const logo = screen.getByRole('button', { name: 'Open new chat' }).querySelector('img');
  expect(logo).toHaveAttribute('src', '/dashboard/logo.svg');
  expect(logo).toHaveAttribute('alt', '');
});
